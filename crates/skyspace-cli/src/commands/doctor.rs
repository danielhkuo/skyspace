//! `skyspace doctor`: config, database, archive, the age of every job's
//! last good run against 1.5x its interval, and the one guard on the
//! jsonb plan document. Exit 2 when anything is overdue, so the hourly
//! timer's `OnFailure=` unit alerts; a run that never started is the
//! case an exit code from the job itself cannot catch.

use std::time::Duration;

use skyspace_ingest::job_names as j;
use skyspace_store::Store;
use time::OffsetDateTime;

use crate::config::Config;

const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const WEEK: Duration = Duration::from_hours(7 * 24);
const MONTH: Duration = Duration::from_hours(30 * 24);

/// The jobs the timers run and how often, in timer order.
pub const INTERVALS: [(&str, Duration); 7] = [
    (j::REFERENCE, WEEK),
    (j::CATALOG, MONTH),
    (j::LISTINGS, DAY),
    (j::SECTIONS, WEEK),
    (j::DETAIL, WEEK),
    (j::SEATS, Duration::from_mins(15)),
    (j::REQUIREMENTS, MONTH),
];

/// Overdue when the last good run is older than this multiple of the
/// interval.
pub const GRACE: f64 = 1.5;

/// Whether a job is overdue: never ran, or its last good run is older than
/// `GRACE` times its interval.
#[must_use]
pub fn overdue(last_ok: Option<OffsetDateTime>, interval: Duration, now: OffsetDateTime) -> bool {
    last_ok.is_none_or(|at| (now - at).as_seconds_f64() > interval.as_secs_f64() * GRACE)
}

fn age_text(last_ok: Option<OffsetDateTime>, now: OffsetDateTime) -> String {
    match last_ok {
        None => "never ran ok".to_owned(),
        Some(at) => {
            let age = now - at;
            let seconds = age.whole_seconds().max(0);
            if seconds < 3600 {
                format!("{} min ago", seconds / 60)
            } else if seconds < 2 * DAY.as_secs().cast_signed() {
                format!("{:.1} h ago", age.as_seconds_f64() / HOUR.as_secs_f64())
            } else {
                format!("{:.1} days ago", age.as_seconds_f64() / DAY.as_secs_f64())
            }
        }
    }
}

fn mask(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(scheme), Some(at)) if at > scheme => {
            let creds = &url[scheme + 3..at];
            let user = creds.split(':').next().unwrap_or("");
            format!("{}://{user}:***{}", &url[..scheme], &url[at..])
        }
        _ => url.to_owned(),
    }
}

async fn check_jobs(store: &Store, now: OffsetDateTime) -> anyhow::Result<bool> {
    let current = store.current_term().await?;
    let mut any_overdue = false;
    for (job, interval) in INTERVALS {
        let seats_window = if job == j::SEATS {
            match current {
                Some(term) => store.poll_windows_active(term, now).await?,
                None => None,
            }
        } else {
            None
        };
        if job == j::SEATS && seats_window.is_none() {
            let last = store
                .last_ok_run(job, None)
                .await?
                .and_then(|r| r.finished_at);
            println!(
                "  {job:<13} {:<20} no poll window open; not checked",
                age_text(last, now)
            );
            continue;
        }
        let interval = seats_window.map_or(interval, |w| {
            Duration::from_mins(u64::try_from(w.interval_minutes).unwrap_or(15))
        });
        let last = store
            .last_ok_run(job, if job == j::SEATS { current } else { None })
            .await?
            .and_then(|r| r.finished_at);
        let late = overdue(last, interval, now);
        any_overdue |= late;
        println!(
            "  {job:<13} {:<20} interval {:.1} h  {}",
            age_text(last, now),
            interval.as_secs_f64() / HOUR.as_secs_f64(),
            if late { "OVERDUE" } else { "ok" }
        );
    }
    Ok(any_overdue)
}

/// Run every check and print one line each.
///
/// # Errors
/// Only when the checks themselves cannot run; a failing check is an
/// exit code, not an error.
pub async fn run(config: &Config) -> anyhow::Result<i32> {
    let now = OffsetDateTime::now_utc();
    let mut problems: Vec<String> = Vec::new();
    println!("config");
    println!("  DATABASE_URL            {}", mask(&config.database_url));
    println!("  SKYSPACE_ARCHIVE_DIR    {}", config.archive_dir.display());
    println!(
        "  SKYSPACE_CONTACT_EMAIL  {}",
        if config.contact_email.is_some() {
            "set"
        } else {
            "unset (pulls will refuse to start)"
        }
    );
    println!("  SKYSPACE_USER_AGENT_SITE {}", config.user_agent_site);
    if config.contact_email.is_none() {
        problems.push("SKYSPACE_CONTACT_EMAIL is unset".to_owned());
    }

    match config.archive().check_writable() {
        Ok(()) => println!("archive   writable"),
        Err(e) => {
            println!("archive   NOT writable: {e}");
            problems.push(format!("archive not writable: {e}"));
        }
    }

    let store = match config.store().await {
        Ok(store) => store,
        Err(e) => {
            println!("database  unreachable: {e}");
            println!("doctor: database unreachable");
            return Ok(2);
        }
    };
    match sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(store.pool())
        .await
    {
        Ok(_) => println!("database  reachable"),
        Err(e) => {
            println!("database  query failed: {e}");
            println!("doctor: database query failed");
            return Ok(2);
        }
    }

    println!("jobs (last good run against 1.5x interval)");
    match check_jobs(&store, now).await {
        Ok(true) => problems.push("a job is overdue or never ran".to_owned()),
        Ok(false) => {}
        Err(e) => {
            // Migrations not applied yet: the tables are missing.
            println!("  cannot read ingest_runs: {e}");
            problems.push(format!("cannot read ingest_runs: {e}"));
        }
    }

    match store.orphaned_requirement_choices().await {
        Ok(0) => println!("plans     0 orphaned requirement choices"),
        Ok(n) => {
            println!("plans     {n} orphaned requirement choices (re-extraction bug)");
            problems.push(format!("{n} orphaned requirement choices"));
        }
        Err(e) => {
            println!("plans     cannot count orphaned choices: {e}");
            problems.push(format!("cannot count orphaned choices: {e}"));
        }
    }
    if let Ok(drafts) = store.drafts(skyspace_store::DraftState::Pending).await {
        println!("review    {} pending drafts", drafts.len());
    }

    if problems.is_empty() {
        println!("doctor: ok");
        Ok(0)
    } else {
        println!("doctor: {}", problems.join("; "));
        Ok(2)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use time::OffsetDateTime;

    use super::{mask, overdue};

    #[test]
    fn overdue_is_one_and_a_half_intervals_or_never() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::from_secs(100_000);
        let day = Duration::from_hours(24);
        assert!(overdue(None, day, now));
        assert!(!overdue(Some(now - Duration::from_hours(24)), day, now));
        assert!(!overdue(Some(now - Duration::from_mins(2150)), day, now));
        assert!(overdue(Some(now - Duration::from_mins(2170)), day, now));
    }

    #[test]
    fn passwords_are_masked() {
        assert_eq!(
            mask("postgres://postgres:skyspace@127.0.0.1:55432/postgres"),
            "postgres://postgres:***@127.0.0.1:55432/postgres"
        );
        assert_eq!(mask("postgres:///local"), "postgres:///local");
    }
}
