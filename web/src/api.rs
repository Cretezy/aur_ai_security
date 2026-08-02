use std::collections::HashMap;

use aur_ai_security_db as db;
use serde::{Deserialize, Serialize};
use topcoat::{
    context::Cx,
    router::{content::Json, error::bad_request, route},
    Result,
};

use crate::database;

const MAX_LOOKUPS: usize = 100;

#[derive(Debug, Deserialize)]
struct LookupRequest {
    packages: Vec<LookupPackage>,
}

#[derive(Debug, Deserialize)]
struct LookupPackage {
    package_base: String,
    commit: String,
}

#[derive(Debug, Serialize)]
struct LookupResponse {
    results: Vec<LookupResult>,
}

#[derive(Debug, Serialize)]
struct LookupResult {
    package_base: String,
    commit: String,
    assessment: Option<Assessment>,
}

#[derive(Clone, Debug, Serialize)]
struct Assessment {
    verdict: String,
    explanation: Option<String>,
    provider: String,
    model: String,
    checked_at: i64,
    version: String,
    details_path: String,
}

#[route(POST "/api/v1/checks/lookup")]
async fn lookup_checks(
    cx: &Cx,
    Json(request): Json<LookupRequest>,
) -> Result<Json<LookupResponse>> {
    validate_request(&request)?;

    let normalized = request
        .packages
        .iter()
        .map(|package| {
            (
                package.package_base.as_str(),
                package.commit.to_ascii_lowercase(),
            )
        })
        .collect::<Vec<_>>();
    let requested = normalized
        .iter()
        .map(|(package_base, commit)| (*package_base, commit.as_str()))
        .collect::<Vec<_>>();
    let matches = db::lookup_checks(database(cx), &requested).await?;

    // The database orders newest-first, so the first row encountered for a pair wins.
    let mut assessments = HashMap::with_capacity(matches.len());
    for check in matches {
        assessments
            .entry((
                check.package_base.clone(),
                check.pkgbuild_commit.to_ascii_lowercase(),
            ))
            .or_insert_with(|| Assessment {
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
            let key = (
                package.package_base.clone(),
                package.commit.to_ascii_lowercase(),
            );
            LookupResult {
                package_base: package.package_base,
                commit: package.commit,
                assessment: assessments.get(&key).cloned(),
            }
        })
        .collect();

    Ok(Json(LookupResponse { results }))
}

fn validate_request(request: &LookupRequest) -> Result<()> {
    if request.packages.len() > MAX_LOOKUPS {
        return Err(bad_request(format!(
            "packages must contain at most {MAX_LOOKUPS} entries"
        ))
        .into());
    }

    for (index, package) in request.packages.iter().enumerate() {
        if !valid_package_base(&package.package_base) {
            return Err(bad_request(format!(
                "packages[{index}].package_base is not a valid package-base name"
            ))
            .into());
        }
        if package.commit.len() != 40
            || !package.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(bad_request(format!(
                "packages[{index}].commit must be a 40-character hexadecimal commit"
            ))
            .into());
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
            .app_context(pool)
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
                    { "package_base": "paru", "commit": commit },
                    { "package_base": "yay", "commit": "ffffffffffffffffffffffffffffffffffffffff" },
                    { "package_base": "paru", "commit": uppercase_commit }
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
                        "package_base": "yay",
                        "commit": "ffffffffffffffffffffffffffffffffffffffff",
                        "assessment": null
                    },
                    {
                        "package_base": "paru",
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
            json!({ "packages": [{ "package_base": "Bad Name", "commit": valid_commit }] }),
            json!({ "packages": [{ "package_base": "paru", "commit": "not-a-commit" }] }),
            json!({
                "packages": (0..101)
                    .map(|_| json!({ "package_base": "paru", "commit": valid_commit }))
                    .collect::<Vec<_>>()
            }),
        ];

        for request in invalid_requests {
            assert_eq!(post(&router, request).await.0, StatusCode::BAD_REQUEST);
        }
    }
}
