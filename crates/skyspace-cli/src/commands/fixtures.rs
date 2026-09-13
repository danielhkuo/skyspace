//! `skyspace fixtures sync`: restore parser fixtures from the archive by
//! hash. Pulled data is never committed, so a manifest of
//! `file name -> sha256` is what git holds and the bytes come from the
//! archive. On this branch the fixtures are synthetic and live in git, so
//! there is no manifest and the command says so.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use skyspace_ingest::archive::from_hex;

use crate::config::Config;

/// A fixtures action.
#[derive(Debug, Subcommand)]
pub enum FixturesCommand {
    /// Write every fixture the manifest names from the archive.
    Sync {
        /// The manifest: a JSON object of file name to sha256 hex.
        #[arg(
            long,
            default_value = "crates/skyspace-parse/tests/fixtures/manifest.json"
        )]
        manifest: PathBuf,
    },
}

/// Run one fixtures action.
///
/// # Errors
/// An unreadable manifest, or a hash the archive does not hold.
pub fn run(command: FixturesCommand, config: &Config) -> anyhow::Result<i32> {
    let FixturesCommand::Sync { manifest } = command;
    sync(&manifest, config)
}

fn sync(manifest: &Path, config: &Config) -> anyhow::Result<i32> {
    if !manifest.exists() {
        println!(
            "no fixture manifest at {}; the fixtures on this branch are synthetic and committed, nothing to sync",
            manifest.display()
        );
        return Ok(0);
    }
    let text = std::fs::read_to_string(manifest)?;
    let entries: std::collections::BTreeMap<String, String> = serde_json::from_str(&text)?;
    let dir = manifest.parent().unwrap_or(Path::new("."));
    let archive = config.archive();
    let mut written = 0usize;
    for (name, hash) in &entries {
        let key = from_hex(hash)
            .ok_or_else(|| anyhow::anyhow!("{name}: {hash:?} is not a sha256 hex string"))?;
        let bytes = archive
            .get(&key)
            .map_err(|e| anyhow::anyhow!("{name}: archive has no blob {hash}: {e}"))?;
        std::fs::write(dir.join(name), bytes)?;
        written += 1;
    }
    println!("restored {written} fixtures into {}", dir.display());
    Ok(0)
}
