use std::future::Future;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use aur_security_db::{
    CheckDetail, CheckLookupResult, CheckSummary, Database, LookupKey, PackageSearchResult,
    PAGE_SIZE,
};
use aur_security_protocol as protocol;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsValue;
use worker::{
    d1::{D1Database, D1PreparedStatement},
    send::IntoSendFuture,
};

#[derive(Debug)]
pub struct D1Backend {
    database: D1Database,
}

impl D1Backend {
    pub fn new(database: D1Database) -> Self {
        Self { database }
    }

    fn bind(
        &self,
        statement: D1PreparedStatement,
        values: Vec<JsValue>,
    ) -> Result<D1PreparedStatement> {
        statement
            .bind(&values)
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn all<T: DeserializeOwned>(
        &self,
        statement: D1PreparedStatement,
    ) -> impl Future<Output = Result<Vec<T>>> + Send {
        worker::send::SendFuture::new(async move {
            let result = statement
                .all()
                .into_send()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            result.results().map_err(|error| anyhow!(error.to_string()))
        })
    }

    fn run(&self, statement: D1PreparedStatement) -> impl Future<Output = Result<()>> + Send + '_ {
        worker::send::SendFuture::new(async move {
            self.database
                .batch(vec![statement])
                .into_send()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(())
        })
    }
}

fn text(value: &str) -> JsValue {
    JsValue::from_str(value)
}

fn integer(value: i64) -> JsValue {
    JsValue::from_f64(value as f64)
}

fn real(value: f64) -> JsValue {
    JsValue::from_f64(value)
}

fn optional_text(value: Option<&str>) -> JsValue {
    value.map_or(JsValue::NULL, text)
}

const D1_PACKAGE_PARAMS_PER_ROW: usize = 11;
const D1_PACKAGE_ROWS_PER_STATEMENT: usize = 100 / D1_PACKAGE_PARAMS_PER_ROW;

fn package_upsert_sql(row_count: usize) -> String {
    let values = std::iter::repeat_n("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", row_count)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO package_versions (package_name, version, package_base, aur_package_id, aur_package_base_id, submitter, last_modified, popularity, url_path, first_seen_at, last_seen_at) VALUES {values} ON CONFLICT(package_name, version) DO UPDATE SET package_base = excluded.package_base, aur_package_id = excluded.aur_package_id, aur_package_base_id = excluded.aur_package_base_id, submitter = excluded.submitter, last_modified = excluded.last_modified, popularity = excluded.popularity, url_path = excluded.url_path, last_seen_at = excluded.last_seen_at"
    )
}

#[async_trait]
impl Database for D1Backend {
    async fn lookup_checks(&self, packages: &[LookupKey]) -> Result<Vec<CheckLookupResult>> {
        let mut results = Vec::new();
        for chunk in packages.chunks(40) {
            let mut sql = String::from(
                "SELECT pv.package_base, c.pkgbuild_commit, pv.version, c.provider, c.model, c.verdict, c.explanation, c.checked_at FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id WHERE ",
            );
            let mut values = Vec::with_capacity(chunk.len() * 2);
            for (index, key) in chunk.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("(pv.package_base = ? AND c.pkgbuild_commit = ?)");
                values.push(text(&key.package_base));
                values.push(text(&key.commit));
            }
            sql.push_str(" ORDER BY c.checked_at DESC, c.id DESC");
            let statement = self.bind(self.database.prepare(sql), values)?;
            results.extend(self.all(statement).await?);
        }
        Ok(results)
    }

    async fn recent_checks(
        &self,
        page: i64,
        search: &str,
        verdict: Option<&str>,
    ) -> Result<(Vec<CheckSummary>, i64)> {
        let pattern = format!("%{search}%");
        let verdict = verdict.unwrap_or("");
        let total_statement = self.bind(
            self.database.prepare(
                "SELECT COUNT(*) AS total FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id WHERE (? = '' OR pv.package_name LIKE ? OR pv.package_base LIKE ?) AND (? = '' OR c.verdict = ?)",
            ),
            vec![text(search), text(&pattern), text(&pattern), text(verdict), text(verdict)],
        )?;
        let total = total_statement
            .first::<i64>(Some("total"))
            .into_send()
            .await
            .map_err(|error| anyhow!(error.to_string()))?
            .unwrap_or_default();
        let statement = self.bind(
            self.database.prepare(
                "SELECT pv.package_name, pv.package_base, pv.version, c.provider, c.model, c.pkgbuild_commit, c.verdict, c.explanation, c.checked_at FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id WHERE (? = '' OR pv.package_name LIKE ? OR pv.package_base LIKE ?) AND (? = '' OR c.verdict = ?) ORDER BY c.checked_at DESC, c.id DESC LIMIT ? OFFSET ?",
            ),
            vec![
                text(search),
                text(&pattern),
                text(&pattern),
                text(verdict),
                text(verdict),
                integer(PAGE_SIZE),
                integer((page - 1) * PAGE_SIZE),
            ],
        )?;
        Ok((self.all(statement).await?, total))
    }

    async fn repository_checks(&self, repository: &str) -> Result<Vec<CheckSummary>> {
        let statement = self.bind(
            self.database.prepare(
                "SELECT pv.package_name, pv.package_base, pv.version, c.provider, c.model, c.pkgbuild_commit, c.verdict, c.explanation, c.checked_at FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id WHERE pv.package_base = ? ORDER BY c.checked_at DESC, c.id DESC",
            ),
            vec![text(repository)],
        )?;
        self.all(statement).await
    }

    async fn check_detail(&self, repository: &str, commit: &str) -> Result<Option<CheckDetail>> {
        let statement = self.bind(
            self.database.prepare(
                "SELECT pv.package_name, pv.package_base, pv.version, c.provider, c.model, c.pkgbuild_commit, c.verdict, c.explanation, c.commit_diff, c.pkgbuild, c.checked_at FROM checks c JOIN package_versions pv ON pv.id = c.package_version_id WHERE pv.package_base = ? AND c.pkgbuild_commit = ? ORDER BY c.checked_at DESC, c.id DESC LIMIT 1",
            ),
            vec![text(repository), text(commit)],
        )?;
        Ok(self.all::<CheckDetail>(statement).await?.into_iter().next())
    }

    async fn search_packages(&self, query: &str) -> Result<Vec<PackageSearchResult>> {
        let pattern = format!("%{query}%");
        let statement = self.bind(
            self.database.prepare(
                "SELECT pv.package_name, pv.package_base, pv.version, pv.popularity FROM package_versions pv WHERE pv.last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1) AND (pv.package_name LIKE ? OR pv.package_base LIKE ?) ORDER BY pv.popularity DESC, pv.package_name LIMIT 100",
            ),
            vec![text(&pattern), text(&pattern)],
        )?;
        self.all(statement).await
    }

    async fn current_package_base_exists(&self, package_base: &str) -> Result<bool> {
        let statement = self.bind(
            self.database.prepare(
                "SELECT EXISTS (SELECT 1 FROM package_versions WHERE package_base = ? AND last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1)) AS present",
            ),
            vec![text(package_base)],
        )?;
        Ok(self
            .all::<ExistsRow>(statement)
            .await?
            .first()
            .is_some_and(|row| row.present != 0))
    }

    async fn begin_index(&self, _seen_at: i64) -> Result<()> {
        Ok(())
    }

    async fn upsert_index_batch(
        &self,
        seen_at: i64,
        packages: &[protocol::PackageVersion],
    ) -> Result<usize> {
        for api_chunk in packages.chunks(protocol::MAX_INDEX_BATCH) {
            let mut statements = Vec::new();
            for chunk in api_chunk.chunks(D1_PACKAGE_ROWS_PER_STATEMENT) {
                let mut values = Vec::with_capacity(chunk.len() * D1_PACKAGE_PARAMS_PER_ROW);
                for package in chunk {
                    values.extend([
                        text(&package.package_name),
                        text(&package.version),
                        text(&package.package_base),
                        integer(package.aur_package_id),
                        integer(package.aur_package_base_id),
                        optional_text(package.submitter.as_deref()),
                        integer(package.last_modified),
                        real(package.popularity),
                        text(&package.url_path),
                        integer(seen_at),
                        integer(seen_at),
                    ]);
                }
                statements.push(self.bind(
                    self.database.prepare(package_upsert_sql(chunk.len())),
                    values,
                )?);
            }
            self.database
                .batch(statements)
                .into_send()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
        }
        Ok(packages.len())
    }

    async fn finish_index(&self, seen_at: i64) -> Result<()> {
        self.run(
            self.bind(
                self.database
                    .prepare("UPDATE index_state SET current_seen_at = ? WHERE id = 1"),
                vec![integer(seen_at)],
            )?,
        )
        .await
    }

    async fn candidates(
        &self,
        request: &protocol::CandidateRequest,
    ) -> Result<Vec<protocol::Candidate>> {
        let mut sql = String::from(
            "SELECT pv.package_name, pv.package_base, pv.version FROM package_versions pv WHERE pv.last_seen_at = (SELECT current_seen_at FROM index_state WHERE id = 1) AND NOT EXISTS (SELECT 1 FROM checks c WHERE c.package_version_id = pv.id AND c.provider = ? AND c.model = ?)",
        );
        let mut values = vec![text(&request.provider), text(&request.model)];
        if let Some(since) = request.since {
            sql.push_str(" AND pv.last_modified >= ?");
            values.push(integer(since));
        }
        if !request.filters.is_empty() {
            sql.push_str(" AND pv.package_name IN (");
            for (index, filter) in request.filters.iter().enumerate() {
                if index > 0 {
                    sql.push_str(", ");
                }
                sql.push('?');
                values.push(text(filter));
            }
            sql.push(')');
        }
        sql.push_str(" ORDER BY pv.last_modified, pv.package_name");
        let statement = self.bind(self.database.prepare(sql), values)?;
        self.all(statement).await
    }

    async fn upsert_checks(&self, checks: &[protocol::CheckResult]) -> Result<usize> {
        let statements = checks
            .iter()
            .map(|check| {
                self.bind(
                    self.database.prepare(
                        "INSERT INTO checks (package_version_id, provider, model, pkgbuild_commit, verdict, explanation, commit_diff, pkgbuild, checked_at) SELECT id, ?, ?, ?, ?, ?, ?, ?, ? FROM package_versions WHERE package_name = ? AND version = ? ON CONFLICT(package_version_id, provider, model) DO UPDATE SET pkgbuild_commit = excluded.pkgbuild_commit, verdict = excluded.verdict, explanation = excluded.explanation, commit_diff = excluded.commit_diff, pkgbuild = excluded.pkgbuild, checked_at = excluded.checked_at",
                    ),
                    vec![
                        text(&check.provider),
                        text(&check.model),
                        text(&check.pkgbuild_commit),
                        text(&check.verdict),
                        optional_text(check.explanation.as_deref()),
                        text(&check.commit_diff),
                        text(&check.pkgbuild),
                        integer(check.checked_at),
                        text(&check.package_name),
                        text(&check.version),
                    ],
                )
            })
            .collect::<Result<Vec<_>>>()?;
        self.database
            .batch(statements)
            .into_send()
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(checks.len())
    }
}

#[derive(serde::Deserialize)]
struct ExistsRow {
    present: i64,
}
