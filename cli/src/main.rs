use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

mod aur;
mod check;
mod since;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ProviderArg {
    Openai,
    Anthropic,
    Openrouter,
    Codex,
}

impl From<ProviderArg> for aur_ai_security_checker::Provider {
    fn from(provider: ProviderArg) -> Self {
        match provider {
            ProviderArg::Openai => Self::Openai,
            ProviderArg::Anthropic => Self::Anthropic,
            ProviderArg::Openrouter => Self::Openrouter,
            ProviderArg::Codex => Self::Codex,
        }
    }
}

#[derive(Debug, Parser)]
#[command(version, about = "Review AUR package build files with AI")]
struct Cli {
    /// SQLite database path.
    #[arg(long, default_value = "sqlite.db", global = true)]
    database: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Download the latest AUR metadata index and record its package versions.
    UpdateIndex,

    /// Check current, unchecked package versions.
    Check {
        /// Print the packages that would be checked without cloning or calling AI.
        #[arg(long)]
        dry_run: bool,

        /// Only check these packages. May also be supplied more than once.
        #[arg(long, value_name = "PACKAGE", num_args = 1..)]
        filter: Vec<String>,

        /// Only check packages modified since this Unix, ISO-8601, or relative time.
        #[arg(long, value_name = "TIME", allow_hyphen_values = true)]
        since: Option<since::Since>,

        /// AI provider to use.
        #[arg(long, value_enum)]
        provider: ProviderArg,

        /// Provider model name.
        #[arg(long)]
        model: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    info!(database = %cli.database.display(), "starting AUR AI security CLI");
    let pool = aur_ai_security_db::connect(&cli.database, true).await?;

    match cli.command {
        Command::UpdateIndex => aur::update_index(&pool).await,
        Command::Check {
            dry_run,
            filter,
            since,
            provider,
            model,
        } => {
            check::run(
                &pool,
                dry_run,
                &filter,
                since.map(since::Since::timestamp),
                provider.into(),
                &model,
            )
            .await
        }
    }
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
