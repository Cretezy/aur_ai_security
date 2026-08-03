use std::{
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use aur_ai_security_db::Database;
use aur_ai_security_protocol as protocol;
use flate2::read::GzDecoder;
use serde::Deserialize;
use tracing::{debug, info};

const INDEX_URL: &str = "https://aur.archlinux.org/packages-meta-v1.json.gz";

#[derive(Debug, Deserialize)]
struct PackageMetadata {
    #[serde(rename = "ID")]
    aur_package_id: i64,
    #[serde(rename = "Name")]
    package_name: String,
    #[serde(rename = "PackageBaseID")]
    aur_package_base_id: i64,
    #[serde(rename = "PackageBase")]
    package_base: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Submitter")]
    submitter: Option<String>,
    #[serde(rename = "LastModified")]
    last_modified: i64,
    #[serde(rename = "Popularity")]
    popularity: f64,
    #[serde(rename = "URLPath")]
    url_path: String,
}

pub async fn update_index<B: Database>(database: &B) -> Result<()> {
    let packages = download_index().await?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    database.begin_index(now).await?;
    for chunk in packages.chunks(protocol::MAX_INDEX_BATCH) {
        database.upsert_index_batch(now, chunk).await?;
    }
    database.finish_index(now).await?;
    info!(packages = packages.len(), "updated package version index");
    Ok(())
}

async fn download_index() -> Result<Vec<protocol::PackageVersion>> {
    info!(url = INDEX_URL, "downloading AUR package index");
    let compressed = reqwest::get(INDEX_URL)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    debug!(
        compressed_bytes = compressed.len(),
        "downloaded compressed AUR index"
    );
    let packages: Vec<PackageMetadata> =
        serde_json::from_reader(GzDecoder::new(Cursor::new(compressed)))
            .context("failed to decode the AUR package index")?;
    debug!(packages = packages.len(), "decoded AUR package index");
    Ok(packages
        .into_iter()
        .map(|package| protocol::PackageVersion {
            package_name: package.package_name,
            version: package.version,
            package_base: package.package_base,
            aur_package_id: package.aur_package_id,
            aur_package_base_id: package.aur_package_base_id,
            submitter: package.submitter,
            last_modified: package.last_modified,
            popularity: package.popularity,
            url_path: package.url_path,
        })
        .collect())
}
