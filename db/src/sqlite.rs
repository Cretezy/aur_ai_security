use std::{path::Path, str::FromStr};

use anyhow::{Context, Result};
use async_trait::async_trait;
use aur_ai_security_protocol as protocol;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    QueryBuilder, Row, Sqlite, SqlitePool,
};

use crate::{
    CheckDetail, CheckLookupResult, CheckSummary, Database, LookupKey, PackageSearchResult,
    PAGE_SIZE,
};

#[derive(Clone, Debug)]
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

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

async fn lookup_checks_keys(
    pool: &SqlitePool,
    packages: &[LookupKey],
) -> Result<Vec<CheckLookupResult>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT pv.package_base, c.pkgbuild_commit, pv.version,
                  c.provider, c.model, c.verdict, c.explanation, c.checked_at
           FROM checks c
           JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE "#,
    );
    for (index, key) in packages.iter().enumerate() {
        if index > 0 {
            query.push(" OR ");
        }
        query
            .push("(pv.package_base = ")
            .push_bind(&key.package_base)
            .push(" AND c.pkgbuild_commit = ")
            .push_bind(&key.commit)
            .push(")");
    }
    query.push(" ORDER BY c.checked_at DESC, c.id DESC");
    let checks = query.build_query_as().fetch_all(pool).await?;
    Ok(checks)
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
        r#"SELECT COUNT(*) FROM checks c
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
           FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE (? = '' OR pv.package_name LIKE ? OR pv.package_base LIKE ?)
             AND (? = '' OR c.verdict = ?)
           ORDER BY c.checked_at DESC, c.id DESC LIMIT ? OFFSET ?"#,
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
    Ok((checks, total))
}

pub async fn repository_checks(
    pool: &SqlitePool,
    repository: &str,
) -> Result<Vec<CheckSummary>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version,
                  c.provider, c.model, c.pkgbuild_commit,
                  c.verdict, c.explanation, c.checked_at
           FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE pv.package_base = ? ORDER BY c.checked_at DESC, c.id DESC"#,
    )
    .bind(repository)
    .fetch_all(pool)
    .await
}

pub async fn check_detail(
    pool: &SqlitePool,
    repository: &str,
    commit: &str,
) -> Result<Option<CheckDetail>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version,
                  c.provider, c.model, c.pkgbuild_commit,
                  c.verdict, c.explanation, c.commit_diff, c.pkgbuild, c.checked_at
           FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id
           WHERE pv.package_base = ? AND c.pkgbuild_commit = ?
           ORDER BY c.checked_at DESC, c.id DESC LIMIT 1"#,
    )
    .bind(repository)
    .bind(commit)
    .fetch_optional(pool)
    .await
}

pub async fn search_packages(
    pool: &SqlitePool,
    query: &str,
) -> Result<Vec<PackageSearchResult>, sqlx::Error> {
    let pattern = format!("%{query}%");
    sqlx::query_as(
        r#"SELECT pv.package_name, pv.package_base, pv.version, pv.popularity
           FROM package_versions pv
           WHERE pv.last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1)
             AND (pv.package_name LIKE ? OR pv.package_base LIKE ?)
           ORDER BY pv.popularity DESC, pv.package_name LIMIT 100"#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await
}

pub async fn current_package_base_exists(
    pool: &SqlitePool,
    package_base: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM package_versions
           WHERE package_base = ?
             AND last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1))"#,
    )
    .bind(package_base)
    .fetch_one(pool)
    .await
}

#[async_trait]
impl Database for SqliteBackend {
    async fn lookup_checks(&self, packages: &[LookupKey]) -> Result<Vec<CheckLookupResult>> {
        Ok(lookup_checks_keys(&self.pool, packages).await?)
    }

    async fn recent_checks(
        &self,
        page: i64,
        search: &str,
        verdict: Option<&str>,
    ) -> Result<(Vec<CheckSummary>, i64)> {
        Ok(recent_checks(&self.pool, page, search, verdict).await?)
    }

    async fn repository_checks(&self, repository: &str) -> Result<Vec<CheckSummary>> {
        Ok(repository_checks(&self.pool, repository).await?)
    }

    async fn check_detail(&self, repository: &str, commit: &str) -> Result<Option<CheckDetail>> {
        Ok(check_detail(&self.pool, repository, commit).await?)
    }

    async fn search_packages(&self, query: &str) -> Result<Vec<PackageSearchResult>> {
        Ok(search_packages(&self.pool, query).await?)
    }

    async fn current_package_base_exists(&self, package_base: &str) -> Result<bool> {
        Ok(current_package_base_exists(&self.pool, package_base).await?)
    }

    async fn begin_index(&self, _seen_at: i64) -> Result<()> {
        Ok(())
    }

    async fn upsert_index_batch(
        &self,
        seen_at: i64,
        packages: &[protocol::PackageVersion],
    ) -> Result<usize> {
        let mut transaction = self.pool.begin().await?;
        for package in packages {
            sqlx::query(
                r#"INSERT INTO package_versions (
                    package_name, version, package_base, aur_package_id,
                    aur_package_base_id, submitter, last_modified, popularity,
                    url_path, first_seen_at, last_seen_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(package_name, version) DO UPDATE SET
                    package_base = excluded.package_base,
                    aur_package_id = excluded.aur_package_id,
                    aur_package_base_id = excluded.aur_package_base_id,
                    submitter = excluded.submitter,
                    last_modified = excluded.last_modified,
                    popularity = excluded.popularity,
                    url_path = excluded.url_path,
                    last_seen_at = excluded.last_seen_at"#,
            )
            .bind(&package.package_name)
            .bind(&package.version)
            .bind(&package.package_base)
            .bind(package.aur_package_id)
            .bind(package.aur_package_base_id)
            .bind(&package.submitter)
            .bind(package.last_modified)
            .bind(package.popularity)
            .bind(&package.url_path)
            .bind(seen_at)
            .bind(seen_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(packages.len())
    }

    async fn finish_index(&self, seen_at: i64) -> Result<()> {
        sqlx::query("UPDATE index_state SET current_seen_at = ? WHERE id = 1")
            .bind(seen_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn candidates(
        &self,
        request: &protocol::CandidateRequest,
    ) -> Result<Vec<protocol::Candidate>> {
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"SELECT pv.package_name, pv.package_base, pv.version
               FROM package_versions pv
               WHERE pv.last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1)
                 AND NOT EXISTS (SELECT 1 FROM checks c
                   WHERE c.package_version_id = pv.id AND c.provider = "#,
        );
        query.push_bind(&request.provider);
        query.push(" AND c.model = ");
        query.push_bind(&request.model);
        query.push(")");
        if let Some(since) = request.since {
            query.push(" AND pv.last_modified >= ");
            query.push_bind(since);
        }
        if !request.filters.is_empty() {
            query.push(" AND pv.package_name IN (");
            let mut separated = query.separated(", ");
            for filter in &request.filters {
                separated.push_bind(filter);
            }
            separated.push_unseparated(")");
        }
        query.push(" ORDER BY pv.last_modified, pv.package_name");
        Ok(query
            .build()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                Ok(protocol::Candidate {
                    package_name: row.try_get("package_name")?,
                    package_base: row.try_get("package_base")?,
                    version: row.try_get("version")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?)
    }

    async fn upsert_checks(&self, checks: &[protocol::CheckResult]) -> Result<usize> {
        let mut transaction = self.pool.begin().await?;
        for check in checks {
            sqlx::query(
                r#"INSERT INTO checks (
                    package_version_id, provider, model, pkgbuild_commit,
                    verdict, explanation, commit_diff, pkgbuild, checked_at
                ) SELECT id, ?, ?, ?, ?, ?, ?, ?, ? FROM package_versions
                WHERE package_name = ? AND version = ?
                ON CONFLICT(package_version_id, provider, model) DO UPDATE SET
                    pkgbuild_commit = excluded.pkgbuild_commit,
                    verdict = excluded.verdict,
                    explanation = excluded.explanation,
                    commit_diff = excluded.commit_diff,
                    pkgbuild = excluded.pkgbuild,
                    checked_at = excluded.checked_at"#,
            )
            .bind(&check.provider)
            .bind(&check.model)
            .bind(&check.pkgbuild_commit)
            .bind(&check.verdict)
            .bind(&check.explanation)
            .bind(&check.commit_diff)
            .bind(&check.pkgbuild)
            .bind(check.checked_at)
            .bind(&check.package_name)
            .bind(&check.version)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(checks.len())
    }
}
