use std::{
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use sqlx::SqlitePool;
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

pub async fn update_index(pool: &SqlitePool) -> Result<()> {
    info!(url = INDEX_URL, "downloading AUR package index");
    println!("Downloading {INDEX_URL}");
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
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let mut transaction = pool.begin().await?;

    sqlx::query("UPDATE package_versions SET is_current = 0")
        .execute(&mut *transaction)
        .await?;

    for package in &packages {
        sqlx::query(
            r#"INSERT INTO package_versions (
                    package_name, version, package_base, aur_package_id,
                    aur_package_base_id, submitter, last_modified, popularity,
                    url_path, is_current, first_seen_at, last_seen_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
                ON CONFLICT(package_name, version) DO UPDATE SET
                    package_base = excluded.package_base,
                    aur_package_id = excluded.aur_package_id,
                    aur_package_base_id = excluded.aur_package_base_id,
                    submitter = excluded.submitter,
                    last_modified = excluded.last_modified,
                    popularity = excluded.popularity,
                    url_path = excluded.url_path,
                    is_current = 1,
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
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    info!(packages = packages.len(), "updated package version index");
    println!("Recorded {} current package versions", packages.len());
    Ok(())
}
