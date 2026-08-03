use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use aur_ai_security_checker::{check_package, Provider};
use aur_ai_security_db::Database;
use aur_ai_security_protocol as protocol;
use futures::{stream, StreamExt};
use tracing::{debug, error, info};

pub async fn run<B: Database>(
    database: &B,
    dry_run: bool,
    filters: &[String],
    since: Option<i64>,
    provider: Provider,
    model: &str,
    parallelism: usize,
) -> Result<()> {
    let candidates = database
        .candidates(&protocol::CandidateRequest {
            provider: provider.as_str().to_owned(),
            model: model.to_owned(),
            since,
            filters: filters.to_vec(),
        })
        .await?;
    debug!(
        candidates = candidates.len(),
        filters = filters.len(),
        ?since,
        provider = provider.as_str(),
        model,
        dry_run,
        parallelism,
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

    stream::iter(candidates)
        .for_each_concurrent(parallelism, |package| async move {
            info!(
                package = package.package_name,
                version = package.version,
                provider = provider.as_str(),
                model,
                "checking package"
            );
            if let Err(error) = check_one(database, &package, provider, model).await {
                error!(
                    package = package.package_name,
                    version = package.version,
                    provider = provider.as_str(),
                    model,
                    error = ?error,
                    "failed to check package"
                );
            }
        })
        .await;

    Ok(())
}

async fn check_one<B: Database>(
    database: &B,
    package: &protocol::Candidate,
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
    let (verdict, explanation) = checked.assessment.verdict.database_fields();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let check = protocol::CheckResult {
        package_name: package.package_name.clone(),
        version: package.version.clone(),
        provider: provider.as_str().to_owned(),
        model: model.to_owned(),
        pkgbuild_commit: checked.commit_id.clone(),
        verdict: verdict.to_owned(),
        explanation: explanation.map(str::to_owned),
        checked_at: now,
        commit_diff: checked.commit_diff,
        pkgbuild: checked.pkgbuild,
    };
    database.upsert_checks(&[check]).await?;
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
