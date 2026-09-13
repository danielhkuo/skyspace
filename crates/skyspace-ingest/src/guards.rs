//! The statistical guards, as pure functions over counts, so they are
//! unit-tested without a page or a database. A scraper breaks quietly:
//! zero rows, a successful run, an empty catalog. Each guard turns that
//! into a quarantine, with the bytes already on disk for a person to look.

use std::collections::BTreeMap;

/// Volume may move this fraction from the last good run before quarantine.
pub const VOLUME_TOLERANCE: f64 = 0.20;
/// A field's fill rate may move this many points from its trailing median.
pub const FILL_TOLERANCE: f64 = 0.20;
/// How many good runs the fill-rate median looks back over.
pub const FILL_HISTORY_RUNS: usize = 7;

/// A subject that had rows in the last good run returns none now: a
/// changed row selector, or Rice returned the search form.
#[must_use]
pub const fn zero_rows(previous_rows: u64, rows_now: u64) -> bool {
    previous_rows > 0 && rows_now == 0
}

/// The count moved more than `VOLUME_TOLERANCE` from the last good run.
/// No history means no guard: the first run is always accepted.
#[must_use]
pub fn volume_drifts(previous_rows: u64, rows_now: u64) -> bool {
    if previous_rows == 0 {
        return false;
    }
    // Row counts are in the thousands; f64 holds them exactly.
    let previous = u32::try_from(previous_rows).map_or(f64::MAX, f64::from);
    let now = u32::try_from(rows_now).map_or(f64::MAX, f64::from);
    ((now - previous) / previous).abs() > VOLUME_TOLERANCE
}

/// The middle value; the mean of the two middles for an even count.
#[must_use]
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    Some(if sorted.len().is_multiple_of(2) {
        f64::midpoint(sorted[mid - 1], sorted[mid])
    } else {
        sorted[mid]
    })
}

/// A field's filled fraction moved more than `FILL_TOLERANCE` from the
/// median of its history. An empty history never fires.
#[must_use]
pub fn fill_rate_drifts(history: &[f64], rate_now: f64) -> bool {
    median(history).is_some_and(|m| (rate_now - m).abs() > FILL_TOLERANCE)
}

/// The fraction of kept rows that filled a field; zero when nothing was kept.
#[must_use]
pub fn fill_rate(rows_kept: u32, rows_filled: u32) -> f64 {
    if rows_kept == 0 {
        0.0
    } else {
        f64::from(rows_filled) / f64::from(rows_kept)
    }
}

/// Every field whose fill rate drifted, with `(median, now)`. `history`
/// maps a field to its rates over past good runs; `now` maps a field to
/// this run's rate. A field absent from history is new and never fires.
#[must_use]
pub fn drifting_fields(
    history: &BTreeMap<String, Vec<f64>>,
    now: &BTreeMap<String, f64>,
) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    for (field, rates) in history {
        let rate_now = now.get(field).copied().unwrap_or(0.0);
        if let Some(m) = median(rates)
            && fill_rate_drifts(rates, rate_now)
        {
            out.push((field.clone(), m, rate_now));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{drifting_fields, fill_rate, fill_rate_drifts, median, volume_drifts, zero_rows};

    #[test]
    fn zero_rows_only_after_a_good_run() {
        assert!(zero_rows(40, 0));
        assert!(!zero_rows(0, 0));
        assert!(!zero_rows(40, 1));
    }

    #[test]
    fn volume_moves_twenty_percent() {
        assert!(!volume_drifts(0, 5000));
        assert!(!volume_drifts(1000, 1200));
        assert!(volume_drifts(1000, 1201));
        assert!(volume_drifts(1000, 799));
        assert!(!volume_drifts(1000, 800));
    }

    #[test]
    fn medians_and_fill_rates() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert!(!fill_rate_drifts(&[], 0.0));
        assert!(!fill_rate_drifts(&[0.9, 0.95, 0.92], 0.75));
        assert!(fill_rate_drifts(&[0.9, 0.95, 0.92], 0.6));
        assert!((fill_rate(10, 4) - 0.4).abs() < f64::EPSILON);
        assert!(fill_rate(0, 0).abs() < f64::EPSILON);
    }

    #[test]
    fn drifting_fields_names_the_field() {
        let history = BTreeMap::from([
            ("meeting".to_owned(), vec![0.4, 0.42, 0.41]),
            ("title".to_owned(), vec![1.0, 1.0]),
        ]);
        let now = BTreeMap::from([("meeting".to_owned(), 0.0), ("title".to_owned(), 0.99)]);
        let drift = drifting_fields(&history, &now);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].0, "meeting");
    }
}
