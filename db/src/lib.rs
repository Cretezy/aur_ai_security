use anyhow::Result;
use async_trait::async_trait;
use aur_security_protocol as protocol;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "sqlite")]
pub use sqlite::{connect, SqliteBackend};

pub const PAGE_SIZE: i64 = 25;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupKey {
    pub package_base: String,
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "sqlite", derive(sqlx::FromRow))]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "sqlite", derive(sqlx::FromRow))]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "sqlite", derive(sqlx::FromRow))]
pub struct PackageSearchResult {
    pub package_name: String,
    pub package_base: String,
    pub version: String,
    pub popularity: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "sqlite", derive(sqlx::FromRow))]
pub struct CheckLookupResult {
    pub package_base: String,
    pub pkgbuild_commit: String,
    pub version: String,
    pub provider: String,
    pub model: String,
    pub verdict: String,
    pub explanation: Option<String>,
    pub checked_at: i64,
}

#[async_trait]
pub trait Database: Send + Sync {
    async fn lookup_checks(&self, packages: &[LookupKey]) -> Result<Vec<CheckLookupResult>>;
    async fn recent_checks(
        &self,
        page: i64,
        search: &str,
        verdict: Option<&str>,
    ) -> Result<(Vec<CheckSummary>, i64)>;
    async fn repository_checks(&self, repository: &str) -> Result<Vec<CheckSummary>>;
    async fn check_detail(&self, repository: &str, commit: &str) -> Result<Option<CheckDetail>>;
    async fn search_packages(&self, query: &str) -> Result<Vec<PackageSearchResult>>;
    async fn current_package_base_exists(&self, package_base: &str) -> Result<bool>;

    async fn begin_index(&self, seen_at: i64) -> Result<()>;

    async fn upsert_index_batch(
        &self,
        seen_at: i64,
        packages: &[protocol::PackageVersion],
    ) -> Result<usize>;

    async fn finish_index(&self, seen_at: i64) -> Result<()>;

    async fn candidates(
        &self,
        request: &protocol::CandidateRequest,
    ) -> Result<Vec<protocol::Candidate>>;

    async fn upsert_checks(&self, checks: &[protocol::CheckResult]) -> Result<usize>;
}
