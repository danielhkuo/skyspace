//! `skyspace archive diff`: what did Rice change? A unified diff of two
//! archived bodies of one URL, from disk, with no request.

use clap::{Subcommand, ValueEnum};
use similar::TextDiff;
use skyspace_ingest::RawRow;
use skyspace_ingest::fetch::decode;

use crate::config::Config;

/// An archive action.
#[derive(Debug, Subcommand)]
pub enum ArchiveCommand {
    /// Diff the newest archived body of a URL against an older one.
    Diff {
        /// The exact URL as `raw_responses.url` holds it.
        #[arg(long)]
        url: String,
        /// Which older body: the newest one from a run that ended `ok`, or
        /// simply the previous one.
        #[arg(long, value_enum, default_value_t = Against::LastGood)]
        against: Against,
    },
}

/// The older side of the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Against {
    /// The newest body fetched by a run whose outcome is `ok`, other than
    /// the newest body itself.
    LastGood,
    /// The body fetched just before the newest.
    Previous,
}

/// Pick the newest row and the row to compare it with.
#[must_use]
pub fn pick(rows: &[RawRow], against: Against) -> Option<(&RawRow, &RawRow)> {
    let newest = rows.first()?;
    let older = match against {
        Against::Previous => rows.get(1)?,
        Against::LastGood => rows
            .iter()
            .skip(1)
            .find(|r| r.run_outcome.as_deref() == Some("ok"))?,
    };
    Some((newest, older))
}

/// Run one archive action.
///
/// # Errors
/// Connection, store or archive errors.
pub async fn run(command: ArchiveCommand, config: &Config) -> anyhow::Result<i32> {
    let ArchiveCommand::Diff { url, against } = command;
    let ctx = config.job_ctx().await?;
    let rows = ctx.raw_history(&url, 50).await?;
    let Some((newest, older)) = pick(&rows, against) else {
        println!(
            "{} archived bodies for {url}; need a newer one and an older one ({against:?})",
            rows.len()
        );
        return Ok(1);
    };
    if newest.sha256 == older.sha256 {
        println!(
            "identical bodies: {} (run {}) and {} (run {})",
            newest.fetched_at, newest.run_id, older.fetched_at, older.run_id
        );
        return Ok(0);
    }
    let old_text = decode(
        &ctx.archive.get(&older.key()?)?,
        older.content_type.as_deref(),
    );
    let new_text = decode(
        &ctx.archive.get(&newest.key()?)?,
        newest.content_type.as_deref(),
    );
    let diff = TextDiff::from_lines(&old_text, &new_text);
    let old_name = format!(
        "{} run {} ({})",
        older.fetched_at,
        older.run_id,
        older.run_outcome.as_deref().unwrap_or("running")
    );
    let new_name = format!(
        "{} run {} ({})",
        newest.fetched_at,
        newest.run_id,
        newest.run_outcome.as_deref().unwrap_or("running")
    );
    print!(
        "{}",
        diff.unified_diff()
            .context_radius(3)
            .header(&old_name, &new_name)
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use skyspace_ingest::RawRow;
    use time::OffsetDateTime;

    use super::{Against, pick};

    fn row(id: i64, outcome: &str) -> RawRow {
        RawRow {
            id,
            run_id: id,
            url: "u".to_owned(),
            term_code: None,
            fetched_at: OffsetDateTime::UNIX_EPOCH,
            content_type: None,
            sha256: vec![0; 32],
            run_outcome: Some(outcome.to_owned()),
        }
    }

    #[test]
    fn last_good_skips_quarantined_bodies() {
        let rows = vec![row(3, "quarantined"), row(2, "quarantined"), row(1, "ok")];
        let (newest, older) = pick(&rows, Against::LastGood).unwrap();
        assert_eq!((newest.id, older.id), (3, 1));
        let (_, older) = pick(&rows, Against::Previous).unwrap();
        assert_eq!(older.id, 2);
        assert!(pick(&rows[..1], Against::Previous).is_none());
    }
}
