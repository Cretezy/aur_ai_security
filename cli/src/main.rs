use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

mod aur;
mod check;
mod remote;
mod since;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ProviderArg {
    Openai,
    Anthropic,
    Openrouter,
    Claude,
    Codex,
}

impl From<ProviderArg> for aur_security_checker::Provider {
    fn from(provider: ProviderArg) -> Self {
        match provider {
            ProviderArg::Openai => Self::Openai,
            ProviderArg::Anthropic => Self::Anthropic,
            ProviderArg::Openrouter => Self::Openrouter,
            ProviderArg::Claude => Self::Claude,
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

    /// Remote Worker URL. When set, database commands use the authenticated HTTP API.
    #[arg(long, env = "AUR_SECURITY_REMOTE_URL", global = true)]
    remote_url: Option<String>,

    /// Bearer token for authenticated remote database commands.
    #[arg(long, env = "AUR_SECURITY_API_TOKEN", global = true)]
    api_token: Option<String>,

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

        /// Number of package checks to run concurrently. Defaults to available CPU cores.
        #[arg(
            long,
            alias = "jobs",
            env = "AUR_SECURITY_CHECK_PARALLELISM",
            value_name = "N",
            default_value_t = default_parallelism(),
            value_parser = parse_parallelism
        )]
        parallelism: usize,
    },
}

fn default_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn parse_parallelism(value: &str) -> Result<usize, String> {
    let parallelism = value
        .parse::<usize>()
        .map_err(|_| format!("{value:?} is not a positive integer"))?;
    if parallelism == 0 {
        return Err("parallelism must be at least 1".to_owned());
    }
    Ok(parallelism)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_logging();
    let cli = Cli::parse();
    if let Some(remote_url) = cli.remote_url {
        let token = cli
            .api_token
            .context("--api-token or AUR_SECURITY_API_TOKEN is required with --remote-url")?;
        let client = remote::RemoteClient::new(&remote_url, token)?;
        info!(remote_url, "starting AUR Security CLI in remote mode");
        return match cli.command {
            Command::UpdateIndex => aur::update_index(&client).await,
            Command::Check {
                dry_run,
                filter,
                since,
                provider,
                model,
                parallelism,
            } => {
                check::run(
                    &client,
                    dry_run,
                    &filter,
                    since.map(since::Since::timestamp),
                    provider.into(),
                    &model,
                    parallelism,
                )
                .await
            }
        };
    }

    info!(database = %cli.database.display(), "starting AUR Security CLI");
    let pool = aur_security_db::connect(&cli.database, true).await?;
    let database = aur_security_db::SqliteBackend::new(pool);
    match cli.command {
        Command::UpdateIndex => aur::update_index(&database).await,
        Command::Check {
            dry_run,
            filter,
            since,
            provider,
            model,
            parallelism,
        } => {
            check::run(
                &database,
                dry_run,
                &filter,
                since.map(since::Since::timestamp),
                provider.into(),
                &model,
                parallelism,
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
    let includes_target = |target: &str| {
        configured.split(',').any(|directive| {
            directive
                .split_once('=')
                .is_some_and(|(configured_target, _)| {
                    let configured_target = configured_target.trim();
                    configured_target == target
                        || configured_target
                            .strip_prefix(target)
                            .is_some_and(|suffix| suffix.starts_with("::"))
                })
        })
    };
    let mut filter = EnvFilter::new(configured.clone());
    for (target, level) in [("sqlx", "info"), ("rig", "warn"), ("rig_core", "warn")] {
        if !includes_target(target) {
            filter = filter.add_directive(
                format!("{target}={level}")
                    .parse()
                    .expect("valid dependency log directive"),
            );
        }
    }
    filter
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_parallelism_override() {
        let cli = Cli::try_parse_from([
            "aur_security",
            "check",
            "--provider",
            "openai",
            "--model",
            "test",
            "--parallelism",
            "3",
        ])
        .unwrap();

        let Command::Check { parallelism, .. } = cli.command else {
            panic!("expected check command");
        };
        assert_eq!(parallelism, 3);
    }

    #[test]
    fn rejects_zero_parallelism() {
        let error = Cli::try_parse_from([
            "aur_security",
            "check",
            "--provider",
            "openai",
            "--model",
            "test",
            "--parallelism",
            "0",
        ])
        .unwrap_err();

        assert!(error.to_string().contains("parallelism must be at least 1"));
    }
}
