//! Polite fetch: one permit per host, a 150 ms gap measured from the end of
//! the previous response, retries with backoff, an identified User-Agent,
//! decoding from the `Content-Type` charset, and the archive write before
//! anything interprets the bytes.
//!
//! Caching is content hashing only. Banner's `ETag` is not content-derived
//! and it sends no `Last-Modified`, so there is no conditional GET.

use std::collections::BTreeMap;
use std::future::Future;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bytes::Bytes;
use skyspace_store::RawSource;
use time::OffsetDateTime;
use url::Url;

use crate::archive::{Archive, sha256};
use crate::error::FetchError;

/// Base path for every `courses.rice.edu` request. The `courses/courses/`
/// segment is doubled on purpose: requests fail without it.
pub const COURSES_BASE: &str = "https://courses.rice.edu/courses/courses/!SWKSCAT";
/// Origin of the General Announcements.
pub const GA_BASE: &str = "https://ga.rice.edu";

/// Minimum gap between two requests to a host, measured from the end of
/// the previous response (the conservative reading of "150 ms apart").
pub const MIN_GAP: Duration = Duration::from_millis(150);
/// Connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Whole-request timeout: a hung socket must not stall a 5,000-request job.
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Retries after the first attempt, so four attempts in all.
pub const RETRIES: u32 = 3;
/// Backoff before each retry, plus up to `JITTER` of jitter.
pub const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
/// Jitter ceiling, so parallel timers do not retry in lockstep.
pub const JITTER: Duration = Duration::from_millis(500);
/// A job stops after this many failures in a row on a host.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// The User-Agent a polite client sends: identified, with a real address.
#[must_use]
pub fn user_agent(site: &str, contact: &str) -> String {
    format!("skyspace/0.1 (+https://{site}; contact: {contact})")
}

/// What kind of document is being fetched. Matches the `raw_responses.source`
/// check constraint; seats are absent on purpose, because their XML is never
/// archived (`Fetch::get_live`). No `clap` derive: the CLI maps `--source`
/// through `FromStr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// A subject listing page.
    Listing,
    /// A `CATALIST` page.
    Catalog,
    /// `ASSOCIATED-SECTIONS` XML.
    SectionXml,
    /// A section detail page.
    Detail,
    /// A reference list.
    Reference,
    /// The GA program index.
    ProgramIndex,
    /// A GA program page.
    Program,
}

impl Source {
    /// Every source, in the order of the check constraint.
    pub const ALL: [Self; 7] = [
        Self::Listing,
        Self::Catalog,
        Self::SectionXml,
        Self::Detail,
        Self::Reference,
        Self::ProgramIndex,
        Self::Program,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Listing => "listing",
            Self::Catalog => "catalog",
            Self::SectionXml => "section_xml",
            Self::Detail => "detail",
            Self::Reference => "reference",
            Self::ProgramIndex => "program_index",
            Self::Program => "program",
        }
    }

    /// The store's twin of this enum.
    #[must_use]
    pub const fn raw(self) -> RawSource {
        match self {
            Self::Listing => RawSource::Listing,
            Self::Catalog => RawSource::Catalog,
            Self::SectionXml => RawSource::SectionXml,
            Self::Detail => RawSource::Detail,
            Self::Reference => RawSource::Reference,
            Self::ProgramIndex => RawSource::ProgramIndex,
            Self::Program => RawSource::Program,
        }
    }
}

impl FromStr for Source {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| {
                format!(
                    "unknown source {text:?}; one of {}",
                    Self::ALL.map(Self::as_str).join(", ")
                )
            })
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One fetched document.
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    /// Raw bytes, before decoding.
    pub body: Bytes,
    /// Decoded with the `Content-Type` charset, UTF-8 when absent.
    pub text: String,
    /// The archive key: SHA-256 of `body`.
    pub sha256: [u8; 32],
    /// HTTP status.
    pub status: u16,
    /// The `Content-Type` header, kept for `raw_responses`.
    pub content_type: Option<String>,
    /// True when `sha256` matches the last body this fetcher archived for
    /// the same URL. Tracked in process only: the store keeps no per-URL
    /// hash lookup, so a new process starts with every URL "changed".
    pub unchanged: bool,
    /// When the response finished.
    pub fetched_at: OffsetDateTime,
}

/// Implemented by [`Fetcher`] for real pulls and by a fixture reader in
/// tests, so job tests need no HTTP mock crate. Written as a returned
/// `impl Future` on purpose: a bare `async fn` in a public trait fires
/// rustc's `async_fn_in_trait` warning and CI runs `-D warnings`.
pub trait Fetch {
    /// Fetch politely, with retries, decode, archive the bytes.
    fn get(
        &self,
        url: &Url,
        source: Source,
    ) -> impl Future<Output = Result<FetchOutcome, FetchError>> + Send;

    /// Fetch politely without archiving: the seats poll, whose XML is
    /// lossless and whose archive is `seat_snapshots`.
    fn get_live(&self, url: &Url) -> impl Future<Output = Result<FetchOutcome, FetchError>> + Send;
}

/// One permit at a time, so concurrency is one by construction: the guard
/// is held across the sleep and across the request.
#[derive(Debug)]
pub struct HostLimiter {
    min_gap: Duration,
    last_finished: tokio::sync::Mutex<Option<Instant>>,
}

/// Held for the duration of one request. Dropping it stamps the end time
/// the next request's gap is measured from.
pub struct Permit<'a> {
    slot: tokio::sync::MutexGuard<'a, Option<Instant>>,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.slot = Some(Instant::now());
    }
}

impl HostLimiter {
    /// A limiter with the given gap.
    #[must_use]
    pub const fn new(min_gap: Duration) -> Self {
        Self {
            min_gap,
            last_finished: tokio::sync::Mutex::const_new(None),
        }
    }

    /// Wait for the previous request to end plus the gap, then hold the
    /// only permit.
    pub async fn acquire(&self) -> Permit<'_> {
        let slot = self.last_finished.lock().await;
        if let Some(last) = *slot {
            let elapsed = last.elapsed();
            if elapsed < self.min_gap {
                tokio::time::sleep(self.min_gap.saturating_sub(elapsed)).await;
            }
        }
        Permit { slot }
    }
}

/// The real client: one shared by every job.
pub struct Fetcher {
    http: reqwest::Client,
    limiter: HostLimiter,
    archive: Archive,
    last_sha: Mutex<BTreeMap<String, [u8; 32]>>,
}

impl std::fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fetcher")
            .field("archive", &self.archive)
            .finish_non_exhaustive()
    }
}

/// Whether a response status is worth another attempt.
#[must_use]
pub const fn retryable_status(status: u16) -> bool {
    status == 408 || status == 429 || status >= 500
}

impl Fetcher {
    /// Build the client with the politeness settings and `user_agent`.
    ///
    /// # Errors
    /// `FetchError::Transport` when the TLS backend cannot initialise.
    pub fn new(archive: Archive, user_agent: String) -> Result<Self, FetchError> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            limiter: HostLimiter::new(MIN_GAP),
            archive,
            last_sha: Mutex::new(BTreeMap::new()),
        })
    }

    /// The archive this fetcher writes to.
    #[must_use]
    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    /// One attempt after the other, with backoff. The host permit is held
    /// from the request until the body has arrived, so the next gap is
    /// measured from the end of this response.
    async fn fetch_bytes(
        &self,
        url: &Url,
    ) -> Result<(Bytes, u16, Option<String>, OffsetDateTime), FetchError> {
        let mut attempt = 0u32;
        loop {
            let permit = self.limiter.acquire().await;
            let result = self.attempt(url).await;
            drop(permit);
            let retry = match &result {
                Ok(Attempt::Retry(_)) => true,
                Ok(Attempt::Done(_) | Attempt::Failed(_)) => false,
                Err(e) => e.is_connect() || e.is_timeout(),
            };
            if !retry || attempt >= RETRIES {
                return match result? {
                    Attempt::Done(done) => Ok(done),
                    Attempt::Retry(status) | Attempt::Failed(status) => {
                        Err(FetchError::Status(status))
                    }
                };
            }
            let backoff = BACKOFF[usize::try_from(attempt).unwrap_or(2).min(2)] + jitter();
            tracing::warn!(url = %url, attempt, backoff_ms = backoff.as_millis(), "retrying");
            tokio::time::sleep(backoff).await;
            attempt += 1;
        }
    }

    /// One request, body included.
    async fn attempt(&self, url: &Url) -> Result<Attempt, reqwest::Error> {
        let response = self.http.get(url.clone()).send().await?;
        let status = response.status().as_u16();
        if retryable_status(status) {
            return Ok(Attempt::Retry(status));
        }
        if !response.status().is_success() {
            return Ok(Attempt::Failed(status));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = response.bytes().await?;
        Ok(Attempt::Done((
            body,
            status,
            content_type,
            OffsetDateTime::now_utc(),
        )))
    }
}

/// What one attempt produced: a body, or a status worth retrying.
enum Attempt {
    Done((Bytes, u16, Option<String>, OffsetDateTime)),
    Retry(u16),
    Failed(u16),
}

impl Fetch for Fetcher {
    // The trait spells its methods as returned `impl Future` (see the trait
    // doc); an implementation may refine that with `async fn`.
    async fn get(&self, url: &Url, source: Source) -> Result<FetchOutcome, FetchError> {
        let (body, status, content_type, fetched_at) = self.fetch_bytes(url).await?;
        let sha = self.archive.put(&body)?;
        let unchanged = {
            let mut last = self
                .last_sha
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            last.insert(url.to_string(), sha) == Some(sha)
        };
        tracing::debug!(url = %url, source = %source, status, bytes = body.len(), unchanged, "fetched");
        let text = decode(&body, content_type.as_deref());
        Ok(FetchOutcome {
            body,
            text,
            sha256: sha,
            status,
            content_type,
            unchanged,
            fetched_at,
        })
    }

    async fn get_live(&self, url: &Url) -> Result<FetchOutcome, FetchError> {
        let (body, status, content_type, fetched_at) = self.fetch_bytes(url).await?;
        let text = decode(&body, content_type.as_deref());
        Ok(FetchOutcome {
            sha256: sha256(&body),
            text,
            body,
            status,
            content_type,
            unchanged: false,
            fetched_at,
        })
    }
}

/// Up to `JITTER`, from the clock's nanoseconds: enough to break lockstep
/// without a random-number dependency.
fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let ceiling = u64::try_from(JITTER.as_millis()).unwrap_or(500);
    Duration::from_millis(u64::from(nanos) % (ceiling + 1))
}

/// Decode a body with the charset in its `Content-Type`, falling back to
/// UTF-8. Banner does not always send UTF-8.
#[must_use]
pub fn decode(body: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(charset_label)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(body);
    text.into_owned()
}

fn charset_label(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.trim().split_once('=')?;
        (key.trim().eq_ignore_ascii_case("charset"))
            .then(|| value.trim().trim_matches('"').to_owned())
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{HostLimiter, Source, charset_label, decode, retryable_status};

    #[tokio::test]
    async fn limiter_spaces_three_requests() {
        let limiter = HostLimiter::new(Duration::from_millis(150));
        let start = Instant::now();
        for _ in 0..3 {
            let permit = limiter.acquire().await;
            drop(permit);
        }
        // The first is immediate; the next two each wait the full gap.
        assert!(start.elapsed() >= Duration::from_millis(300));
    }

    #[test]
    fn source_round_trips_text() {
        for source in Source::ALL {
            assert_eq!(source.as_str().parse::<Source>().unwrap(), source);
        }
        assert!("seats".parse::<Source>().is_err());
    }

    #[test]
    fn decodes_by_charset() {
        assert_eq!(
            charset_label("text/html; charset=ISO-8859-1").as_deref(),
            Some("ISO-8859-1")
        );
        assert_eq!(
            decode(b"caf\xe9", Some("text/html; charset=iso-8859-1")),
            "café"
        );
        assert_eq!(decode("café".as_bytes(), None), "café");
        assert!(retryable_status(503) && retryable_status(429) && !retryable_status(404));
    }
}
