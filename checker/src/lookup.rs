use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use aur_security_protocol::{LookupPackage, LookupRequest, LookupResponse, LookupResult};
use reqwest::Url;

const LOOKUP_PATH: &str = "/api/v1/checks/lookup";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Fetches stored assessments for the requested package commits.
///
/// Requests larger than the lookup API's per-request limit are split into
/// sequential batches and reassembled in the same package and commit order as
/// the input request.
pub async fn lookup(remote_url: &str, request: &LookupRequest) -> Result<LookupResponse> {
    let base_url = Url::parse(remote_url).context("lookup API URL is invalid")?;
    if !matches!(base_url.scheme(), "http" | "https") {
        bail!("lookup API URL must use http or https");
    }
    let endpoint = base_url
        .join(LOOKUP_PATH)
        .context("failed to construct lookup API URL")?;

    if request.packages.is_empty() {
        return Ok(LookupResponse {
            results: Vec::new(),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!(
            "aur-security-checker/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .context("failed to create lookup API client")?;

    let batches = lookup_batches(&request.packages)?;
    let mut results = request
        .packages
        .iter()
        .map(|package| LookupResult {
            package_base: package.package_base.clone(),
            commits: Vec::with_capacity(package.commits.len()),
        })
        .collect::<Vec<_>>();

    for batch in batches {
        let response = lookup_batch(&client, &endpoint, &batch.packages).await?;
        for (index, result) in batch.indices.into_iter().zip(response.results) {
            results[index].commits.extend(result.commits);
        }
    }

    Ok(LookupResponse { results })
}

#[derive(Debug)]
struct LookupBatch {
    indices: Vec<usize>,
    packages: Vec<LookupPackage>,
}

fn lookup_batches(packages: &[LookupPackage]) -> Result<Vec<LookupBatch>> {
    let mut batches = Vec::new();
    let mut batch = LookupBatch {
        indices: Vec::new(),
        packages: Vec::new(),
    };
    let mut available = aur_security_protocol::MAX_LOOKUPS;

    for (index, package) in packages.iter().enumerate() {
        ensure!(
            !package.commits.is_empty(),
            "lookup package '{}' must contain at least one commit",
            package.package_base
        );

        let mut offset = 0;
        while offset < package.commits.len() {
            if available == 0 {
                batches.push(batch);
                batch = LookupBatch {
                    indices: Vec::new(),
                    packages: Vec::new(),
                };
                available = aur_security_protocol::MAX_LOOKUPS;
            }

            let end = (offset + available).min(package.commits.len());
            batch.indices.push(index);
            batch.packages.push(LookupPackage {
                package_base: package.package_base.clone(),
                commits: package.commits[offset..end].to_vec(),
            });
            available -= end - offset;
            offset = end;
        }
    }

    if !batch.packages.is_empty() {
        batches.push(batch);
    }

    Ok(batches)
}

async fn lookup_batch(
    client: &reqwest::Client,
    endpoint: &Url,
    packages: &[LookupPackage],
) -> Result<LookupResponse> {
    let response = client
        .post(endpoint.clone())
        .json(&LookupRequest {
            packages: packages.to_vec(),
        })
        .send()
        .await
        .context("failed to contact the lookup API")?
        .error_for_status()
        .context("the lookup API returned an error")?
        .json::<LookupResponse>()
        .await
        .context("the lookup API returned invalid JSON")?;

    validate_response(packages, &response.results)?;
    Ok(response)
}

fn validate_response(packages: &[LookupPackage], results: &[LookupResult]) -> Result<()> {
    ensure!(
        packages.len() == results.len(),
        "the lookup API returned an incomplete response"
    );

    for (package, result) in packages.iter().zip(results) {
        ensure!(
            package.package_base == result.package_base
                && package.commits.len() == result.commits.len(),
            "the lookup API returned mismatched package data"
        );
        for (commit, result) in package.commits.iter().zip(&result.commits) {
            ensure!(
                commit.eq_ignore_ascii_case(&result.commit),
                "the lookup API returned mismatched package data"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use aur_security_protocol::LookupCommitResult;

    fn package(package_base: &str, commits: &[&str]) -> LookupPackage {
        LookupPackage {
            package_base: package_base.to_owned(),
            commits: commits.iter().map(|commit| (*commit).to_owned()).collect(),
        }
    }

    fn commit() -> &'static str {
        "0123456789abcdef0123456789abcdef01234567"
    }

    #[test]
    fn batches_by_total_commit_count_and_preserves_package_indices() {
        let commits = vec![commit(); aur_security_protocol::MAX_LOOKUPS + 1];
        let packages = vec![package("paru", &commits), package("yay", &[commit()])];

        let batches = lookup_batches(&packages).unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].indices, vec![0]);
        assert_eq!(
            batches[0].packages[0].commits.len(),
            aur_security_protocol::MAX_LOOKUPS
        );
        assert_eq!(batches[1].indices, vec![0, 1]);
        assert_eq!(batches[1].packages[0].commits.len(), 1);
        assert_eq!(batches[1].packages[1].package_base, "yay");
    }

    #[test]
    fn rejects_empty_commit_lists() {
        let error = lookup_batches(&[package("paru", &[])]).unwrap_err();
        assert!(error
            .to_string()
            .contains("must contain at least one commit"));
    }

    #[test]
    fn validates_ordered_exact_response_and_case_insensitive_commits() {
        let packages = vec![package("paru", &[commit()])];
        let results = vec![LookupResult {
            package_base: "paru".to_owned(),
            commits: vec![LookupCommitResult {
                commit: commit().to_ascii_uppercase(),
                assessment: None,
            }],
        }];

        assert!(validate_response(&packages, &results).is_ok());
    }

    #[test]
    fn rejects_incomplete_or_mismatched_responses() {
        let packages = vec![package("paru", &[commit()])];
        assert!(validate_response(&packages, &[]).is_err());

        let results = vec![LookupResult {
            package_base: "yay".to_owned(),
            commits: vec![LookupCommitResult {
                commit: commit().to_owned(),
                assessment: None,
            }],
        }];
        assert!(validate_response(&packages, &results).is_err());
    }

    #[tokio::test]
    async fn posts_to_lookup_endpoint_and_decodes_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = serde_json::to_string(&LookupResponse {
            results: vec![LookupResult {
                package_base: "paru".to_owned(),
                commits: vec![LookupCommitResult {
                    commit: commit().to_owned(),
                    assessment: Some(aur_security_protocol::Assessment {
                        verdict: "safe".to_owned(),
                        explanation: None,
                        provider: "codex".to_owned(),
                        model: "test".to_owned(),
                        checked_at: 1,
                        version: "1-1".to_owned(),
                        details_path: "/checks/paru/commit".to_owned(),
                    }),
                }],
            }],
        })
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let length = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(request.starts_with("POST /api/v1/checks/lookup HTTP/1.1"));
            assert!(request.contains("\"package_base\":\"paru\""));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = lookup(
            &format!("http://{address}/base"),
            &LookupRequest {
                packages: vec![package("paru", &[commit()])],
            },
        )
        .await
        .unwrap();

        server.await.unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].commits.len(), 1);
        assert_eq!(
            response.results[0].commits[0]
                .assessment
                .as_ref()
                .unwrap()
                .verdict,
            "safe"
        );
    }

    #[tokio::test]
    async fn returns_empty_response_without_contacting_server() {
        let response = lookup("http://127.0.0.1:1", &LookupRequest { packages: vec![] })
            .await
            .unwrap();

        assert!(response.results.is_empty());
    }

    #[test]
    fn serializes_protocol_request_shape() {
        assert_eq!(
            serde_json::to_value(LookupRequest {
                packages: vec![package("paru", &[commit()])],
            })
            .unwrap(),
            json!({"packages": [{"package_base": "paru", "commits": [commit()]}]})
        );
    }
}
