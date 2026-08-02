use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use aur_ai_security_checker::{check_package, Provider};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::{debug, error, info};

#[derive(Debug)]
struct Candidate {
    id: i64,
    package_name: String,
    package_base: String,
    version: String,
}

pub async fn run(
    pool: &SqlitePool,
    dry_run: bool,
    filters: &[String],
    since: Option<i64>,
    provider: Provider,
    model: &str,
) -> Result<()> {
    let candidates = candidates(pool, filters, since, provider, model).await?;
    debug!(
        candidates = candidates.len(),
        filters = filters.len(),
        ?since,
        provider = provider.as_str(),
        model,
        dry_run,
        "selected package check candidates"
    );

    if candidates.is_empty() {
        println!("No unchecked package versions matched");
        return Ok(());
    }

    if dry_run {
        for package in &candidates {
            println!("{} {}", package.package_name, package.version);
        }
        println!("Would check {} package versions", candidates.len());
        return Ok(());
    }

    for package in candidates {
        info!(
            package = package.package_name,
            version = package.version,
            provider = provider.as_str(),
            model,
            "checking package"
        );
        if let Err(error) = check_one(pool, &package, provider, model).await {
            error!(
                package = package.package_name,
                version = package.version,
                provider = provider.as_str(),
                model,
                error = ?error,
                "failed to check package"
            );
        }
    }

    Ok(())
}

async fn candidates(
    pool: &SqlitePool,
    filters: &[String],
    since: Option<i64>,
    provider: Provider,
    model: &str,
) -> Result<Vec<Candidate>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT pv.id, pv.package_name, pv.package_base, pv.version
           FROM package_versions pv
           WHERE pv.is_current = 1
             AND NOT EXISTS (
                 SELECT 1 FROM checks c
                 WHERE c.package_version_id = pv.id
                   AND c.provider = "#,
    );
    query.push_bind(provider.as_str());
    query.push(" AND c.model = ");
    query.push_bind(model);
    query.push(")");

    if let Some(since) = since {
        query.push(" AND pv.last_modified >= ");
        query.push_bind(since);
    }

    if !filters.is_empty() {
        query.push(" AND pv.package_name IN (");
        let mut separated = query.separated(", ");
        for package in filters {
            separated.push_bind(package);
        }
        separated.push_unseparated(")");
    }
    query.push(" ORDER BY pv.last_modified, pv.package_name");

    query
        .build()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(Candidate {
                id: row.try_get("id")?,
                package_name: row.try_get("package_name")?,
                package_base: row.try_get("package_base")?,
                version: row.try_get("version")?,
            })
        })
        .collect()
}

async fn check_one(
    pool: &SqlitePool,
    package: &Candidate,
    provider: Provider,
    model: &str,
) -> Result<()> {
    let checked = check_package(
        provider,
        model,
        &package.package_name,
        &package.package_base,
    )
    .await?;
    let assessment = &checked.assessment;
    let (verdict, explanation) = assessment.verdict.database_fields();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

    sqlx::query(
        r#"INSERT INTO checks (
               package_version_id, provider, model, pkgbuild_commit,
               verdict, explanation, commit_diff, pkgbuild, checked_at
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(package_version_id, provider, model) DO UPDATE SET
               pkgbuild_commit = excluded.pkgbuild_commit,
               verdict = excluded.verdict,
               explanation = excluded.explanation,
               commit_diff = excluded.commit_diff,
               pkgbuild = excluded.pkgbuild,
               checked_at = excluded.checked_at"#,
    )
    .bind(package.id)
    .bind(provider.as_str())
    .bind(model)
    .bind(&checked.commit_id)
    .bind(verdict)
    .bind(explanation)
    .bind(&checked.commit_diff)
    .bind(&checked.pkgbuild)
    .bind(now)
    .execute(pool)
    .await?;
    debug!(
        package = package.package_name,
        version = package.version,
        commit = checked.commit_id,
        verdict,
        "stored package assessment"
    );

    info!(
        package = package.package_name,
        version = package.version,
        provider = provider.as_str(),
        model,
        verdict,
        ?explanation,
        "completed package check"
    );
    Ok(())
}
