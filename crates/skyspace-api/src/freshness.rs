//! When data is stale. Postgres is the cache: the pull jobs write and the
//! API reads, so when Rice is unreachable the API answers from the last pull
//! and `Freshness.stale` turns true. A stale response is still served:
//! silence is worse than an old number with a date on it.

use time::{Duration, OffsetDateTime};

use crate::dto::{DataSource, Freshness, JobFreshness};

/// A job's name in `ingest_runs.job` and its staleness threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobSchedule {
    /// `ingest_runs.job`.
    pub job: &'static str,
    /// The data source the job feeds.
    pub source: DataSource,
    /// Marked stale after this long since the last good run.
    pub stale_after: Duration,
}

/// The staleness table from `06-api.md`. Seats inside a poll window use
/// `SEATS_IN_WINDOW`; outside one, `SEATS_OUTSIDE_WINDOW`.
pub const JOBS: &[JobSchedule] = &[
    JobSchedule {
        job: "reference",
        source: DataSource::Reference,
        stale_after: Duration::days(120),
    },
    JobSchedule {
        job: "listings",
        source: DataSource::SectionListing,
        stale_after: Duration::hours(36),
    },
    JobSchedule {
        job: "sections",
        source: DataSource::SectionDetail,
        stale_after: Duration::days(10),
    },
    JobSchedule {
        job: "detail",
        source: DataSource::SectionDetail,
        stale_after: Duration::days(10),
    },
    JobSchedule {
        job: "catalog",
        source: DataSource::SectionDetail,
        stale_after: Duration::days(45),
    },
    JobSchedule {
        job: "requirements",
        source: DataSource::GeneralAnnouncements,
        stale_after: Duration::days(400),
    },
];

/// Seats polled every 15 minutes inside a registration window.
pub const SEATS_IN_WINDOW: Duration = Duration::minutes(60);
/// Seats polled daily outside one.
pub const SEATS_OUTSIDE_WINDOW: Duration = Duration::hours(36);

/// The threshold for a job by name; unknown jobs use the listings threshold.
#[must_use]
pub fn threshold(job: &str) -> Duration {
    JOBS.iter()
        .find(|j| j.job == job)
        .map_or(Duration::hours(36), |j| j.stale_after)
}

/// Whether `last_ok` is older than `stale_after`, or missing.
#[must_use]
pub fn is_stale(
    last_ok: Option<OffsetDateTime>,
    stale_after: Duration,
    now: OffsetDateTime,
) -> bool {
    last_ok.is_none_or(|t| now - t > stale_after)
}

/// A stamp for a catalog response. `pulled_at` falls back to the Unix epoch
/// when the job has never run, which is honest: the data is as old as it looks.
#[must_use]
pub fn stamp(
    source: DataSource,
    last_ok: Option<OffsetDateTime>,
    rice_as_of: Option<OffsetDateTime>,
    stale_after: Duration,
    now: OffsetDateTime,
) -> Freshness {
    Freshness {
        source,
        rice_as_of,
        pulled_at: last_ok.unwrap_or(OffsetDateTime::UNIX_EPOCH),
        stale: is_stale(last_ok, stale_after, now),
    }
}

/// The per-job rows for `MetaBody.jobs`.
#[must_use]
pub fn job_rows(
    last_ok: &[(String, Option<OffsetDateTime>)],
    now: OffsetDateTime,
) -> Vec<JobFreshness> {
    JOBS.iter()
        .map(|schedule| {
            let last = last_ok
                .iter()
                .find(|(job, _)| job == schedule.job)
                .and_then(|(_, t)| *t);
            JobFreshness {
                job: schedule.job.to_owned(),
                last_ok: last,
                stale: is_stale(last, schedule.stale_after, now),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_go_stale_after_36_hours() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(100);
        let fresh = Some(now - Duration::hours(35));
        let old = Some(now - Duration::hours(37));
        assert!(!is_stale(fresh, threshold("listings"), now));
        assert!(is_stale(old, threshold("listings"), now));
        assert!(is_stale(None, threshold("listings"), now));
    }

    #[test]
    fn stamp_never_hides_a_missing_run() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let s = stamp(
            DataSource::SectionListing,
            None,
            None,
            Duration::hours(1),
            now,
        );
        assert!(s.stale);
        assert_eq!(s.pulled_at, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn meta_lists_every_job() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let rows = job_rows(&[("listings".to_owned(), Some(now))], now);
        assert_eq!(rows.len(), JOBS.len());
        assert!(!rows.iter().find(|r| r.job == "listings").unwrap().stale);
        assert!(rows.iter().find(|r| r.job == "detail").unwrap().stale);
    }
}
