use serde::{Deserialize, Serialize};

pub const MAX_LOOKUPS: usize = 1_000;
// Keep batches comfortably below the 1,000-query Workers Paid invocation limit
// while avoiding unnecessarily frequent network round trips.
pub const MAX_INDEX_BATCH: usize = 250;
pub const MAX_CHECK_BATCH: usize = 5;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupRequest {
    pub packages: Vec<LookupPackage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupPackage {
    pub package_base: String,
    pub commits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupResponse {
    pub results: Vec<LookupResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupResult {
    pub package_base: String,
    pub commits: Vec<LookupCommitResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LookupCommitResult {
    pub commit: String,
    pub assessment: Option<Assessment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Assessment {
    pub verdict: String,
    pub explanation: Option<String>,
    pub provider: String,
    pub model: String,
    pub checked_at: i64,
    pub version: String,
    pub details_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IndexBeginRequest {
    pub seen_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IndexBatchRequest {
    pub seen_at: i64,
    pub packages: Vec<PackageVersion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageVersion {
    pub package_name: String,
    pub version: String,
    pub package_base: String,
    pub aur_package_id: i64,
    pub aur_package_base_id: i64,
    pub submitter: Option<String>,
    pub last_modified: i64,
    pub popularity: f64,
    pub url_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CandidateRequest {
    pub provider: String,
    pub model: String,
    pub since: Option<i64>,
    pub filters: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CandidateResponse {
    pub packages: Vec<Candidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Candidate {
    pub package_name: String,
    pub package_base: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckBatchRequest {
    pub checks: Vec<CheckResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckResult {
    pub package_name: String,
    pub version: String,
    pub provider: String,
    pub model: String,
    pub pkgbuild_commit: String,
    pub verdict: String,
    pub explanation: Option<String>,
    pub checked_at: i64,
    pub commit_diff: String,
    pub pkgbuild: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AcceptedResponse {
    pub accepted: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecentChecksRequest {
    pub page: i64,
    pub search: String,
    pub verdict: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositoryChecksRequest {
    pub repository: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckDetailRequest {
    pub repository: String,
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchPackagesRequest {
    pub query: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageBaseExistsRequest {
    pub package_base: String,
}
