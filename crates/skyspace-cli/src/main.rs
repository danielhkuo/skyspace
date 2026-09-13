//! `skyspace`: everything an operator runs by hand or from a timer. Each
//! command is one shot; systemd timers call the binary and read its exit
//! code: 0 ok, 1 failed, 2 quarantined (or, for `doctor`, overdue).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod commands;
mod config;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use skyspace_core::program::CatalogYear;
use skyspace_core::term::TermCode;
use skyspace_ingest::Source;

use crate::commands::{archive, doctor, fixtures, import, pull, review};
use crate::config::Config;

/// Skyspace operations: pull Rice data, review requirement drafts, check
/// health.
#[derive(Debug, Parser)]
#[command(name = "skyspace", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Apply the store's migrations.
    Migrate,
    /// Term settings.
    Term {
        #[command(subcommand)]
        command: TermCommand,
    },
    /// Fetch one kind of Rice document and store it.
    Pull {
        #[command(subcommand)]
        job: pull::PullJob,
    },
    /// Load hand-encoded requirements into the review queue.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Re-parse archived documents after a parser fix. No network.
    Replay {
        /// Which parser: `listing`, `catalog`, `section_xml`, `detail`,
        /// `reference`, `program_index`, `program`.
        #[arg(long, value_parser = source)]
        source: Source,
        /// Documents fetched on or after this date (UTC).
        #[arg(long, value_parser = date)]
        since: time::Date,
    },
    /// The requirement review queue.
    Review {
        #[command(subcommand)]
        command: review::ReviewCommand,
    },
    /// Inspect the raw archive.
    Archive {
        #[command(subcommand)]
        command: archive::ArchiveCommand,
    },
    /// Parser fixtures.
    Fixtures {
        #[command(subcommand)]
        command: fixtures::FixturesCommand,
    },
    /// Config, database, archive, and the age of every job's last good run.
    /// Exits 2 when a job is past 1.5x its interval.
    Doctor,
}

#[derive(Debug, Subcommand)]
enum TermCommand {
    /// Make a term the one the site opens on.
    SetCurrent {
        /// Rice's six-digit code, such as 202710.
        #[arg(value_parser = term_code)]
        code: TermCode,
    },
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// The university-wide requirements for one catalog year, from TOML.
    University {
        /// Path to `data/university/<year>.toml`.
        #[arg(long)]
        file: std::path::PathBuf,
    },
}

/// `clap` needs this because `TermCode` lives in `skyspace-core`, which
/// cannot depend on `clap`.
fn term_code(raw: &str) -> Result<TermCode, String> {
    TermCode::parse(raw).map_err(|e| e.to_string())
}

fn source(raw: &str) -> Result<Source, String> {
    raw.parse()
}

fn date(raw: &str) -> Result<time::Date, String> {
    time::Date::parse(raw, &time::format_description::well_known::Iso8601::DATE)
        .map_err(|e| format!("{raw:?} is not YYYY-MM-DD: {e}"))
}

/// Install the log subscriber. Reads `SKYSPACE_LOG`, default
/// `info,skyspace=debug`; JSON when `SKYSPACE_LOG_JSON=1`.
fn init_tracing(json: bool) -> anyhow::Result<()> {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("SKYSPACE_LOG")
        .or_else(|_| EnvFilter::try_new("info,skyspace=debug"))?;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if json {
        builder.json().try_init().map_err(|e| anyhow::anyhow!(e))?;
    } else {
        builder
            .compact()
            .try_init()
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    // A missing .env is fine; an unreadable one is not.
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(e) if e.not_found() => {}
        Err(e) => return Err(e.into()),
    }
    let cli = Cli::parse();
    let config = Config::from_env()?;
    init_tracing(config.log_json)?;
    let code = run(cli.command, &config).await?;
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

async fn run(command: Command, config: &Config) -> anyhow::Result<i32> {
    match command {
        Command::Migrate => {
            let store = config.store().await?;
            store.migrate().await?;
            println!("migrations applied");
            Ok(0)
        }
        Command::Term {
            command: TermCommand::SetCurrent { code },
        } => {
            let store = config.store().await?;
            if store.set_current_term(code).await? {
                println!("current term is now {code}");
                Ok(0)
            } else {
                println!("term {code} is not held; run `skyspace pull reference` first");
                Ok(1)
            }
        }
        Command::Pull { job } => pull::run(job, config).await,
        Command::Import {
            command: ImportCommand::University { file },
        } => import::university(&file, config).await,
        Command::Replay { source, since } => {
            let ctx = config.job_ctx().await?;
            let summary = skyspace_ingest::replay(&ctx, source, since).await;
            pull::report("replay", summary)
        }
        Command::Review { command } => review::run(command, config).await,
        Command::Archive { command } => archive::run(command, config).await,
        Command::Fixtures { command } => fixtures::run(command, config),
        Command::Doctor => doctor::run(config).await,
    }
}

/// Every year flag reads into the core type.
fn catalog_year(raw: &str) -> Result<CatalogYear, String> {
    let year: u16 = raw
        .parse()
        .map_err(|_| format!("{raw:?} is not a four-digit year"))?;
    if !(2000..=2100).contains(&year) {
        return Err(format!("{year} is outside 2000-2100"));
    }
    Ok(CatalogYear(year))
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::{Cli, date, term_code};

    #[test]
    fn command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn value_parsers_read_rice_spellings() {
        assert!(term_code("202710").is_ok());
        assert!(term_code("2027").is_err());
        assert_eq!(
            date("2026-09-01").unwrap(),
            time::macros::date!(2026 - 09 - 01)
        );
        assert!(date("yesterday").is_err());
    }
}
