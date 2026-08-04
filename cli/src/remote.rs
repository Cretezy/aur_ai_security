use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use aur_security_db as db;
use aur_security_protocol as protocol;
use reqwest::{Client, RequestBuilder, Url};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct RemoteBackend {
    client: Client,
    base_url: Url,
    token: String,
}

impl RemoteBackend {
    pub fn new(base_url: &str, token: String) -> Result<Self> {
        let base_url = Url::parse(base_url).context("remote URL is not a valid URL")?;
        if !matches!(base_url.scheme(), "http" | "https") {
            bail!("remote URL must use http or https");
        }
        Ok(Self {
            client: Client::new(),
            base_url,
            token,
        })
    }

    pub async fn index_begin(&self, request: &protocol::IndexBeginRequest) -> Result<()> {
        self.post::<_, protocol::AcceptedResponse>("/api/v1/agent/index/begin", request)
            .await
            .map(|_| ())
    }

    pub async fn index_batch(&self, request: &protocol::IndexBatchRequest) -> Result<()> {
        self.post::<_, protocol::AcceptedResponse>("/api/v1/agent/index/batch", request)
            .await
            .map(|_| ())
    }

    pub async fn index_finish(&self, request: &protocol::IndexBeginRequest) -> Result<()> {
        self.post::<_, protocol::AcceptedResponse>("/api/v1/agent/index/finish", request)
            .await
            .map(|_| ())
    }

    pub async fn candidates(
        &self,
        request: &protocol::CandidateRequest,
    ) -> Result<Vec<protocol::Candidate>> {
        Ok(self
            .post::<_, protocol::CandidateResponse>("/api/v1/agent/candidates", request)
            .await?
            .packages)
    }

    pub async fn submit_checks(&self, request: &protocol::CheckBatchRequest) -> Result<()> {
        self.post::<_, protocol::AcceptedResponse>("/api/v1/agent/checks", request)
            .await
            .map(|_| ())
    }

    pub async fn lookup(
        &self,
        request: &protocol::LookupRequest,
    ) -> Result<protocol::LookupResponse> {
        self.post("/api/v1/checks/lookup", request).await
    }

    async fn fetch_recent_checks(
        &self,
        page: i64,
        search: &str,
        verdict: Option<&str>,
    ) -> Result<(Vec<db::CheckSummary>, i64)> {
        self.post(
            "/api/v1/agent/history/recent",
            &protocol::RecentChecksRequest {
                page,
                search: search.to_owned(),
                verdict: verdict.map(str::to_owned),
            },
        )
        .await
    }

    async fn fetch_repository_checks(&self, repository: &str) -> Result<Vec<db::CheckSummary>> {
        self.post(
            "/api/v1/agent/history/repository",
            &protocol::RepositoryChecksRequest {
                repository: repository.to_owned(),
            },
        )
        .await
    }

    async fn fetch_check_detail(
        &self,
        repository: &str,
        commit: &str,
    ) -> Result<Option<db::CheckDetail>> {
        self.post(
            "/api/v1/agent/history/detail",
            &protocol::CheckDetailRequest {
                repository: repository.to_owned(),
                commit: commit.to_owned(),
            },
        )
        .await
    }

    async fn fetch_search_packages(&self, query: &str) -> Result<Vec<db::PackageSearchResult>> {
        self.post(
            "/api/v1/agent/packages/search",
            &protocol::SearchPackagesRequest {
                query: query.to_owned(),
            },
        )
        .await
    }

    async fn fetch_package_base_exists(&self, package_base: &str) -> Result<bool> {
        self.post(
            "/api/v1/agent/packages/exists",
            &protocol::PackageBaseExistsRequest {
                package_base: package_base.to_owned(),
            },
        )
        .await
    }

    fn authenticated(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(&self.token)
    }

    async fn post<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .context("failed to construct remote API URL")?;
        let response = self
            .authenticated(self.client.post(url))
            .json(body)
            .send()
            .await
            .context("remote API request failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<response body unavailable>".to_owned());
            bail!("remote API returned {status}: {body}");
        }
        response
            .json()
            .await
            .context("remote API returned invalid JSON")
    }
}

pub type RemoteClient = RemoteBackend;

#[async_trait]
impl db::Database for RemoteBackend {
    async fn lookup_checks(
        &self,
        packages: &[db::LookupKey],
    ) -> Result<Vec<db::CheckLookupResult>> {
        let response = self
            .lookup(&protocol::LookupRequest {
                packages: packages
                    .iter()
                    .map(|key| protocol::LookupPackage {
                        package_base: key.package_base.clone(),
                        commits: vec![key.commit.clone()],
                    })
                    .collect(),
            })
            .await?;
        Ok(response
            .results
            .into_iter()
            .flat_map(|package| {
                package.commits.into_iter().filter_map(move |commit| {
                    commit.assessment.map(|assessment| db::CheckLookupResult {
                        package_base: package.package_base.clone(),
                        pkgbuild_commit: commit.commit,
                        version: assessment.version,
                        provider: assessment.provider,
                        model: assessment.model,
                        verdict: assessment.verdict,
                        explanation: assessment.explanation,
                        checked_at: assessment.checked_at,
                    })
                })
            })
            .collect())
    }

    async fn recent_checks(
        &self,
        _page: i64,
        _search: &str,
        _verdict: Option<&str>,
    ) -> Result<(Vec<db::CheckSummary>, i64)> {
        self.fetch_recent_checks(_page, _search, _verdict).await
    }

    async fn repository_checks(&self, _repository: &str) -> Result<Vec<db::CheckSummary>> {
        self.fetch_repository_checks(_repository).await
    }

    async fn check_detail(
        &self,
        _repository: &str,
        _commit: &str,
    ) -> Result<Option<db::CheckDetail>> {
        self.fetch_check_detail(_repository, _commit).await
    }

    async fn search_packages(&self, _query: &str) -> Result<Vec<db::PackageSearchResult>> {
        self.fetch_search_packages(_query).await
    }

    async fn current_package_base_exists(&self, _package_base: &str) -> Result<bool> {
        self.fetch_package_base_exists(_package_base).await
    }

    async fn begin_index(&self, seen_at: i64) -> Result<()> {
        self.index_begin(&protocol::IndexBeginRequest { seen_at })
            .await
    }

    async fn upsert_index_batch(
        &self,
        seen_at: i64,
        packages: &[protocol::PackageVersion],
    ) -> Result<usize> {
        self.index_batch(&protocol::IndexBatchRequest {
            seen_at,
            packages: packages.to_vec(),
        })
        .await?;
        Ok(packages.len())
    }

    async fn finish_index(&self, seen_at: i64) -> Result<()> {
        self.index_finish(&protocol::IndexBeginRequest { seen_at })
            .await
    }

    async fn candidates(
        &self,
        request: &protocol::CandidateRequest,
    ) -> Result<Vec<protocol::Candidate>> {
        self.candidates(request).await
    }

    async fn upsert_checks(&self, checks: &[protocol::CheckResult]) -> Result<usize> {
        self.submit_checks(&protocol::CheckBatchRequest {
            checks: checks.to_vec(),
        })
        .await?;
        Ok(checks.len())
    }
}
