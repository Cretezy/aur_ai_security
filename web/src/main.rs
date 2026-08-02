use std::path::PathBuf;

use anyhow::{Context, Result};
use aur_ai_security_db as db;
use clap::Parser;
use sqlx::SqlitePool;
use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::{app_context, Cx},
    router::{Router, RouterBuilderDiscoverExt},
};
use tracing::info;
use tracing_subscriber::EnvFilter;

mod api;
mod layout;
mod pages;
mod ui;

#[derive(Debug, Parser)]
#[command(version, about = "Browse AUR package security checks")]
struct Args {
    /// SQLite database path shared with the indexer CLI.
    #[arg(long, env = "AUR_AI_SECURITY_DATABASE", default_value = "sqlite.db")]
    database: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let args = Args::parse();
    info!(database = %args.database.display(), "starting AUR AI security web server");
    let pool = db::connect(&args.database, false).await?;

    let assets = AssetBundle::load().context("failed to load Topcoat assets")?;
    topcoat::start(
        Router::builder()
            .discover()
            .assets(assets)
            .app_context(pool)
            .build(),
    )
    .await
    .context("web server failed")
}

pub(crate) fn database(cx: &Cx) -> &SqlitePool {
    app_context(cx)
}

fn init_logging() {
    let filter = logging_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn logging_filter() -> EnvFilter {
    let configured = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned());
    let includes_sqlx = configured.split(',').any(|directive| {
        directive
            .split_once('=')
            .is_some_and(|(target, _)| target.trim().starts_with("sqlx"))
    });
    let filter = EnvFilter::new(configured);
    if includes_sqlx {
        filter
    } else {
        filter.add_directive("sqlx=info".parse().expect("valid SQLx log directive"))
    }
}
