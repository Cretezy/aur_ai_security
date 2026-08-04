use std::collections::HashMap;

use aur_security_db as db;
use aur_security_protocol as protocol;
use topcoat::router::content::Json;
use topcoat::{
    context::Cx,
    router::{error::bad_request, route},
    Result,
};

use crate::database;

#[route(POST "/api/v1/checks/lookup")]
async fn lookup_checks(
    cx: &Cx,
    Json(request): Json<protocol::LookupRequest>,
) -> Result<Json<protocol::LookupResponse>> {
    validate_request(&request)?;

    let normalized = request
        .packages
        .iter()
        .flat_map(|package| {
            package
                .commits
                .iter()
                .map(|commit| (package.package_base.as_str(), commit.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();
    let requested = normalized
        .iter()
        .map(|(package_base, commit)| (*package_base, commit.as_str()))
        .collect::<Vec<_>>();
    let keys = requested
        .into_iter()
        .map(|(package_base, commit)| db::LookupKey {
            package_base: package_base.to_owned(),
            commit: commit.to_owned(),
        })
        .collect::<Vec<_>>();
    let matches = database(cx).lookup_checks(&keys).await?;

    // The database orders newest-first, so the first row encountered for a pair wins.
    let mut assessments = HashMap::with_capacity(matches.len());
    for check in matches {
        assessments
            .entry((
                check.package_base.clone(),
                check.pkgbuild_commit.to_ascii_lowercase(),
            ))
            .or_insert_with(|| protocol::Assessment {
                verdict: check.verdict,
                explanation: check.explanation,
                provider: check.provider,
                model: check.model,
                checked_at: check.checked_at,
                version: check.version,
                details_path: format!("/checks/{}/{}", check.package_base, check.pkgbuild_commit),
            });
    }

    let results = request
        .packages
        .into_iter()
        .map(|package| {
            let commits = package
                .commits
                .into_iter()
                .map(|commit| {
                    let key = (package.package_base.clone(), commit.to_ascii_lowercase());
                    protocol::LookupCommitResult {
                        commit,
                        assessment: assessments.get(&key).cloned(),
                    }
                })
                .collect();
            protocol::LookupResult {
                package_base: package.package_base,
                commits,
            }
        })
        .collect();

    Ok(Json(protocol::LookupResponse { results }))
}

fn validate_request(request: &protocol::LookupRequest) -> Result<()> {
    let mut total_commits = 0usize;

    for (index, package) in request.packages.iter().enumerate() {
        if !valid_package_base(&package.package_base) {
            return Err(bad_request(format!(
                "packages[{index}].package_base is not a valid package-base name"
            ))
            .into());
        }
        if package.commits.is_empty() {
            return Err(bad_request(format!(
                "packages[{index}].commits must contain at least one commit"
            ))
            .into());
        }
        total_commits = total_commits.saturating_add(package.commits.len());
        if total_commits > protocol::MAX_LOOKUPS {
            return Err(bad_request(format!(
                "packages must contain at most {} commits",
                protocol::MAX_LOOKUPS
            ))
            .into());
        }
        for (commit_index, commit) in package.commits.iter().enumerate() {
            if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(bad_request(format!(
                    "packages[{index}].commits[{commit_index}] must be a 40-character hexadecimal commit"
                ))
                .into());
            }
        }
    }

    Ok(())
}

fn valid_package_base(package_base: &str) -> bool {
    let mut bytes = package_base.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if first == b'.' || first == b'-' || !valid_package_base_byte(first) {
        return false;
    }
    bytes.all(valid_package_base_byte)
}

fn valid_package_base_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase()
        || byte.is_ascii_digit()
        || matches!(byte, b'@' | b'.' | b'_' | b'+' | b'-')
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use sqlx::SqlitePool;
    use tempfile::TempDir;
    use topcoat::router::{to_bytes, Body, Request, Router, StatusCode};

    use super::*;

    async fn test_database() -> (TempDir, SqlitePool) {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let pool = db::connect(&directory.path().join("test.db"), true)
            .await
            .expect("test database should connect");
        (directory, pool)
    }

    fn router(pool: SqlitePool) -> Router {
        Router::builder()
            .route(lookup_checks)
            .app_context(db::SqliteBackend::new(pool))
            .build()
    }

    async fn post(router: &Router, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/checks/lookup")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&body).expect("body should serialize"),
            ))
            .expect("request should build");
        let response = router.handle(request).await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, body)
    }

    async fn insert_package(pool: &SqlitePool, package_base: &str, version: &str) -> i64 {
        sqlx::query_scalar(
            r#"INSERT INTO package_versions (
                   package_name, version, package_base, aur_package_id,
                   aur_package_base_id, last_modified, popularity, url_path,
                   first_seen_at, last_seen_at
               ) VALUES (?, ?, ?, 1, 1, 1, 0, '/unused', 1, 1)
               RETURNING id"#,
        )
        .bind(package_base)
        .bind(version)
        .bind(package_base)
        .fetch_one(pool)
        .await
        .expect("package should insert")
    }

    async fn insert_check(
        pool: &SqlitePool,
        package_version_id: i64,
        commit: &str,
        model: &str,
        verdict: &str,
        checked_at: i64,
    ) {
        sqlx::query(
            r#"INSERT INTO checks (
                   package_version_id, provider, model, pkgbuild_commit, verdict,
                   explanation, checked_at, commit_diff, pkgbuild
               ) VALUES (?, 'codex', ?, ?, ?, NULL, ?, 'diff', 'pkgbuild')"#,
        )
        .bind(package_version_id)
        .bind(model)
        .bind(commit)
        .bind(verdict)
        .bind(checked_at)
        .execute(pool)
        .await
        .expect("check should insert");
    }

    #[tokio::test]
    async fn returns_latest_exact_matches_and_null_for_missing_entries() {
        let (_directory, pool) = test_database().await;
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let uppercase_commit = commit.to_ascii_uppercase();
        let package_version_id = insert_package(&pool, "paru", "2.1.0-1").await;
        insert_check(&pool, package_version_id, commit, "old", "suspicious", 10).await;
        insert_check(
            &pool,
            package_version_id,
            commit,
            "same-time-old",
            "safe",
            20,
        )
        .await;
        insert_check(
            &pool,
            package_version_id,
            commit,
            "same-time-new",
            "dangerous",
            20,
        )
        .await;

        let (status, body) = post(
            &router(pool),
            json!({
                "packages": [
                    {
                        "package_base": "paru",
                        "commits": [commit, "ffffffffffffffffffffffffffffffffffffffff"]
                    },
                    { "package_base": "paru", "commits": [uppercase_commit] }
                ]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({
                "results": [
                    {
                        "package_base": "paru",
                        "commits": [
                            {
                                "commit": commit,
                                "assessment": {
                                    "verdict": "dangerous",
                                    "explanation": null,
                                    "provider": "codex",
                                    "model": "same-time-new",
                                    "checked_at": 20,
                                    "version": "2.1.0-1",
                                    "details_path": format!("/checks/paru/{commit}")
                                }
                            },
                            {
                                "commit": "ffffffffffffffffffffffffffffffffffffffff",
                                "assessment": null
                            }
                        ]
                    },
                    {
                        "package_base": "paru",
                        "commits": [{
                            "commit": uppercase_commit,
                            "assessment": {
                                "verdict": "dangerous",
                                "explanation": null,
                                "provider": "codex",
                                "model": "same-time-new",
                                "checked_at": 20,
                                "version": "2.1.0-1",
                                "details_path": format!("/checks/paru/{commit}")
                            }
                        }]
                    }
                ]
            })
        );
    }

    #[tokio::test]
    async fn rejects_invalid_and_oversized_requests() {
        let (_directory, pool) = test_database().await;
        let router = router(pool);
        let valid_commit = "0123456789abcdef0123456789abcdef01234567";
        let invalid_requests = [
            json!({ "packages": [{ "package_base": "Bad Name", "commits": [valid_commit] }] }),
            json!({ "packages": [{ "package_base": "paru", "commits": ["not-a-commit"] }] }),
            json!({ "packages": [{ "package_base": "paru", "commits": [] }] }),
            json!({
                "packages": [{
                    "package_base": "paru",
                    "commits": (0..1_001).map(|_| valid_commit).collect::<Vec<_>>()
                }]
            }),
        ];

        for request in invalid_requests {
            assert_eq!(post(&router, request).await.0, StatusCode::BAD_REQUEST);
        }
    }
}
