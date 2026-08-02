use std::{path::Path, str::FromStr};

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    FromRow, SqlitePool,
};
use tracing::debug;

pub async fn connect(path: &Path, create_if_missing: bool) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))?
        .create_if_missing(create_if_missing)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("failed to open SQLite database")?;

    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to migrate database")?;

    Ok(pool)
}

pub const PAGE_SIZE: i64 = 25;

#[derive(Debug, FromRow)]
pub struct CheckSummary {
    pub package_name: String,
    pub package_base: String,
    pub version: String,
    pub provider: String,
    pub model: String,
    pub pkgbuild_commit: String,
    pub verdict: String,
    pub explanation: Option<String>,
    pub checked_at: i64,
}

#[derive(Debug, FromRow)]
pub struct CheckDetail {
    pub package_name: String,
    pub package_base: String,
    pub version: String,
    pub provider: String,
    pub model: String,
    pub pkgbuild_commit: String,
    pub verdict: String,
    pub explanation: Option<String>,
    pub commit_diff: String,
    pub pkgbuild: String,
    pub checked_at: i64,
}

#[derive(Debug, FromRow)]
pub struct PackageSearchResult {
    pub package_name: String,
    pub package_base: String,
    pub version: String,
    pub popularity: f64,
}

pub async fn recent_checks(
    pool: &SqlitePool,
    page: i64,
    search: &str,
    verdict: Option<&str>,
) -> Result<(Vec<CheckSummary>, i64), sqlx::Error> {
    let pattern = format!("%{search}%");
    let verdict = verdict.unwrap_or("");
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM checks c
           JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE (? = '' OR pv.package_name LIKE ? OR pv.package_base LIKE ?)
             AND (? = '' OR c.verdict = ?)"#,
    )
    .bind(search)
    .bind(&pattern)
    .bind(&pattern)
    .bind(verdict)
    .bind(verdict)
    .fetch_one(pool)
    .await?;
    let checks = sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version,
                  c.provider, c.model, c.pkgbuild_commit,
                  c.verdict, c.explanation, c.checked_at
           FROM checks c
           JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE (? = '' OR pv.package_name LIKE ? OR pv.package_base LIKE ?)
             AND (? = '' OR c.verdict = ?)
           ORDER BY c.checked_at DESC, c.id DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(search)
    .bind(&pattern)
    .bind(&pattern)
    .bind(verdict)
    .bind(verdict)
    .bind(PAGE_SIZE)
    .bind((page - 1) * PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    debug!(
        page,
        search,
        verdict,
        returned = checks.len(),
        total,
        "loaded recent checks"
    );
    Ok((checks, total))
}

pub async fn repository_checks(
    pool: &SqlitePool,
    repository: &str,
) -> Result<Vec<CheckSummary>, sqlx::Error> {
    let checks = sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version,
                  c.provider, c.model, c.pkgbuild_commit,
                  c.verdict, c.explanation, c.checked_at
           FROM checks c
           JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE pv.package_base = ?
           ORDER BY c.checked_at DESC, c.id DESC"#,
    )
    .bind(repository)
    .fetch_all(pool)
    .await?;
    debug!(
        repository,
        returned = checks.len(),
        "loaded repository checks"
    );
    Ok(checks)
}

pub async fn check_detail(
    pool: &SqlitePool,
    repository: &str,
    commit: &str,
) -> Result<Option<CheckDetail>, sqlx::Error> {
    let check = sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version,
                  c.provider, c.model, c.pkgbuild_commit,
                  c.verdict, c.explanation, c.commit_diff, c.pkgbuild, c.checked_at
           FROM checks c
           JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE pv.package_base = ? AND c.pkgbuild_commit = ?
           ORDER BY c.checked_at DESC, c.id DESC
           LIMIT 1"#,
    )
    .bind(repository)
    .bind(commit)
    .fetch_optional(pool)
    .await?;
    debug!(
        repository,
        commit,
        found = check.is_some(),
        "loaded check detail"
    );
    Ok(check)
}

pub async fn search_packages(
    pool: &SqlitePool,
    query: &str,
) -> Result<Vec<PackageSearchResult>, sqlx::Error> {
    let pattern = format!("%{query}%");
    let packages = sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version, pv.popularity
           FROM package_versions pv
           WHERE pv.is_current = 1
             AND (pv.package_name LIKE ? OR pv.package_base LIKE ?)
           ORDER BY pv.popularity DESC, pv.package_name
           LIMIT 100"#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    debug!(
        query,
        returned = packages.len(),
        "searched current packages"
    );
    Ok(packages)
}

pub async fn current_package_base_exists(
    pool: &SqlitePool,
    package_base: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM package_versions
               WHERE package_base = ? AND is_current = 1
           )"#,
    )
    .bind(package_base)
    .fetch_one(pool)
    .await
}
