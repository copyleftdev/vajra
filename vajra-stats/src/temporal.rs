//! Temporal pattern analysis for date/time data.
//!
//! Provides date detection from string values, monotonicity checks,
//! gap detection, and interval statistics for timestamp sequences.

use crate::numeric::{compute_numeric_stats, NumericStats};
use serde::{Deserialize, Serialize};

/// A parsed temporal value represented as epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TemporalValue {
    /// Seconds since Unix epoch (1970-01-01T00:00:00Z).
    pub epoch_seconds: i64,
}

/// Information about a detected gap in a timestamp sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapInfo {
    /// Index of the earlier timestamp in the original sequence.
    pub index: usize,
    /// The interval (in seconds) of this gap.
    pub interval: i64,
    /// The threshold that was exceeded (median + 3*MAD of intervals).
    pub threshold: f64,
}

/// Report from temporal analysis of a timestamp sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalReport {
    /// Whether timestamps are strictly increasing.
    pub is_monotonic: bool,
    /// Gaps that exceed median + 3*MAD of the interval distribution.
    pub gaps: Vec<GapInfo>,
    /// Statistics on inter-event intervals (None if fewer than 2 timestamps).
    pub interval_stats: Option<NumericStats>,
    /// Number of identical timestamps in the sequence.
    pub duplicate_count: u64,
}

/// Date format that was used for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFormat {
    /// YYYY-MM-DD
    IsoDate,
    /// YYYY-MM-DDThh:mm:ss or YYYY-MM-DDThh:mm:ssZ
    IsoDatetime,
    /// US format: MM/DD/YYYY
    UsDate,
    /// EU format: DD/MM/YYYY (with ambiguity when day <= 12)
    EuDate,
    /// Numeric string interpreted as epoch seconds
    EpochSeconds,
}

/// Try to detect and parse dates from a slice of string values.
///
/// Returns a vector of `(index, TemporalValue)` for each string that
/// was successfully parsed as a date/datetime. Formats are tried in
/// order: ISO date, ISO datetime, US date, EU date, epoch seconds.
#[must_use]
pub fn detect_dates(values: &[&str]) -> Vec<(usize, TemporalValue)> {
    let mut results = Vec::new();
    for (i, &s) in values.iter().enumerate() {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(tv) = try_parse_temporal(trimmed) {
            results.push((i, tv));
        }
    }
    results
}

/// Perform temporal analysis on a slice of epoch-second timestamps.
///
/// Computes monotonicity, detects anomalous gaps, provides interval
/// statistics, and counts duplicate timestamps.
#[must_use]
pub fn temporal_analysis(timestamps: &[i64]) -> TemporalReport {
    if timestamps.len() < 2 {
        return TemporalReport {
            is_monotonic: true,
            gaps: Vec::new(),
            interval_stats: None,
            duplicate_count: count_duplicates(timestamps),
        };
    }

    // Check monotonicity
    let is_monotonic = timestamps.windows(2).all(|w| w[0] < w[1]);

    // Compute intervals
    let intervals: Vec<i64> = timestamps.windows(2).map(|w| w[1] - w[0]).collect();

    // Compute interval stats
    #[allow(clippy::cast_precision_loss)]
    let mut interval_floats: Vec<f64> = intervals.iter().map(|&i| i as f64).collect();
    let interval_stats = compute_numeric_stats(&mut interval_floats);

    // Detect gaps: intervals exceeding median + 3*MAD
    let gaps = if let Some(ref stats) = interval_stats {
        let threshold = stats.median + 3.0 * stats.mad;
        intervals
            .iter()
            .enumerate()
            .filter_map(|(idx, &interval)| {
                #[allow(clippy::cast_precision_loss)]
                let interval_f = interval as f64;
                if interval_f > threshold && threshold > 0.0 {
                    Some(GapInfo {
                        index: idx,
                        interval,
                        threshold,
                    })
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let duplicate_count = count_duplicates(timestamps);

    TemporalReport {
        is_monotonic,
        gaps,
        interval_stats,
        duplicate_count,
    }
}

/// Count the number of duplicate timestamps (total occurrences beyond first).
fn count_duplicates(timestamps: &[i64]) -> u64 {
    if timestamps.is_empty() {
        return 0;
    }
    let mut sorted = timestamps.to_vec();
    sorted.sort_unstable();
    let mut dup_count = 0_u64;
    for w in sorted.windows(2) {
        if w[0] == w[1] {
            dup_count += 1;
        }
    }
    dup_count
}

/// Try parsing a string as a temporal value, attempting formats in order.
fn try_parse_temporal(s: &str) -> Option<TemporalValue> {
    // Try ISO 8601 datetime first (more specific)
    if let Some(tv) = try_parse_iso_datetime(s) {
        return Some(tv);
    }
    // Then ISO 8601 date
    if let Some(tv) = try_parse_iso_date(s) {
        return Some(tv);
    }
    // US format MM/DD/YYYY
    if let Some(tv) = try_parse_us_date(s) {
        return Some(tv);
    }
    // EU format DD/MM/YYYY
    if let Some(tv) = try_parse_eu_date(s) {
        return Some(tv);
    }
    // Epoch seconds (numeric string in valid range)
    try_parse_epoch_seconds(s)
}

/// Try to parse ISO 8601 date: YYYY-MM-DD
fn try_parse_iso_date(s: &str) -> Option<TemporalValue> {
    if s.len() != 10 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_u32(&s[0..4])?;
    let month = parse_u32(&s[5..7])?;
    let day = parse_u32(&s[8..10])?;
    let epoch = date_to_epoch(year, month, day)?;
    Some(TemporalValue {
        epoch_seconds: epoch,
    })
}

/// Try to parse ISO 8601 datetime: YYYY-MM-DDThh:mm:ss with optional Z or timezone
fn try_parse_iso_datetime(s: &str) -> Option<TemporalValue> {
    // Minimum: YYYY-MM-DDThh:mm:ss (19 chars)
    if s.len() < 19 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' || (bytes[10] != b'T' && bytes[10] != b't') {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }

    let year = parse_u32(&s[0..4])?;
    let month = parse_u32(&s[5..7])?;
    let day = parse_u32(&s[8..10])?;
    let hour = parse_u32(&s[11..13])?;
    let minute = parse_u32(&s[14..16])?;
    let second = parse_u32(&s[17..19])?;

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let date_epoch = date_to_epoch(year, month, day)?;
    let time_offset = i64::from(hour * 3600 + minute * 60 + second);

    Some(TemporalValue {
        epoch_seconds: date_epoch + time_offset,
    })
}

/// Try to parse US date format: MM/DD/YYYY
fn try_parse_us_date(s: &str) -> Option<TemporalValue> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let month = parse_u32(parts[0])?;
    let day = parse_u32(parts[1])?;
    let year = parse_u32(parts[2])?;
    // Require 4-digit year to avoid ambiguity
    if parts[2].len() != 4 {
        return None;
    }
    // US format: month must be 1-12, day can be > 12 (which disambiguates from EU)
    // but we also accept day <= 12 -- the caller specifies US format is tried first
    date_to_epoch(year, month, day).map(|epoch| TemporalValue {
        epoch_seconds: epoch,
    })
}

/// Try to parse EU date format: DD/MM/YYYY
fn try_parse_eu_date(s: &str) -> Option<TemporalValue> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let day = parse_u32(parts[0])?;
    let month = parse_u32(parts[1])?;
    let year = parse_u32(parts[2])?;
    if parts[2].len() != 4 {
        return None;
    }
    date_to_epoch(year, month, day).map(|epoch| TemporalValue {
        epoch_seconds: epoch,
    })
}

/// Try to parse a numeric string as epoch seconds in range 2000-2050.
fn try_parse_epoch_seconds(s: &str) -> Option<TemporalValue> {
    // Must be all digits (possibly with leading minus, but we restrict to valid range)
    let epoch: i64 = s.parse().ok()?;
    // Valid range: 2000-01-01 (946684800) to ~2050-01-01 (2524608000)
    if (946_684_800..=2_524_608_000).contains(&epoch) {
        Some(TemporalValue {
            epoch_seconds: epoch,
        })
    } else {
        None
    }
}

/// Parse a string as a u32. Returns None if not a valid non-negative integer.
fn parse_u32(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    // All characters must be ASCII digits
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Convert a calendar date to Unix epoch seconds (UTC midnight).
///
/// Returns `None` for invalid dates (bad month/day, year out of range).
#[allow(clippy::cast_possible_wrap)]
fn date_to_epoch(year: u32, month: u32, day: u32) -> Option<i64> {
    if month < 1 || month > 12 || day < 1 {
        return None;
    }
    if year < 1970 || year > 2100 {
        return None;
    }

    let max_day = days_in_month(year, month);
    if day > max_day {
        return None;
    }

    // Days from Unix epoch (1970-01-01) to the target date.
    // Compute days from year 0 for both dates and subtract.
    let target_days = days_from_civil(year as i64, month, day);
    let epoch_days = days_from_civil(1970, 1, 1);

    Some((target_days - epoch_days) * 86400)
}

/// Days from a civil date to a reference point (algorithm from Howard Hinnant).
/// This computes the number of days since an epoch using the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    // Adjust so that March is month 0 (avoids leap day complications)
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };

    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32; // year of era [0, 399]
    let doy = (153 * m + 2) / 5 + day - 1; // day of year [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day of era [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

/// Number of days in a month for a given year.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Whether a year is a leap year.
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ISO date parsing ----

    #[test]
    fn parse_valid_iso_date() {
        let results = detect_dates(&["2024-01-15"]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 0);
        // 2024-01-15 00:00:00 UTC
        // = days from 1970-01-01 to 2024-01-15 * 86400
        let expected = date_to_epoch(2024, 1, 15);
        assert!(expected.is_some());
        assert_eq!(results[0].1.epoch_seconds, expected.unwrap_or(0));
    }

    #[test]
    fn parse_invalid_month() {
        let results = detect_dates(&["2024-13-01"]);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_invalid_day() {
        let results = detect_dates(&["2024-02-30"]);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_leap_day_valid() {
        let results = detect_dates(&["2024-02-29"]);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn parse_leap_day_invalid() {
        // 2023 is not a leap year
        let results = detect_dates(&["2023-02-29"]);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_boundary_dates() {
        // First valid date
        let results = detect_dates(&["1970-01-01"]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.epoch_seconds, 0);

        // Dec 31
        let results = detect_dates(&["2024-12-31"]);
        assert_eq!(results.len(), 1);
    }

    // ---- ISO datetime parsing ----

    #[test]
    fn parse_iso_datetime() {
        let results = detect_dates(&["2024-01-15T10:30:00"]);
        assert_eq!(results.len(), 1);
        let date_epoch = date_to_epoch(2024, 1, 15).unwrap_or(0);
        let expected = date_epoch + 10 * 3600 + 30 * 60;
        assert_eq!(results[0].1.epoch_seconds, expected);
    }

    #[test]
    fn parse_iso_datetime_with_z() {
        let results = detect_dates(&["2024-01-15T10:30:00Z"]);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn parse_invalid_hour() {
        let results = detect_dates(&["2024-01-15T25:00:00"]);
        assert!(results.is_empty());
    }

    // ---- US date format ----

    #[test]
    fn parse_us_date() {
        let results = detect_dates(&["01/15/2024"]);
        assert_eq!(results.len(), 1);
        let expected = date_to_epoch(2024, 1, 15).unwrap_or(0);
        assert_eq!(results[0].1.epoch_seconds, expected);
    }

    #[test]
    fn parse_us_date_unambiguous() {
        // Month 12 day 25 -> clearly US if tried first
        let results = detect_dates(&["12/25/2024"]);
        assert_eq!(results.len(), 1);
    }

    // ---- EU date format ----

    #[test]
    fn parse_eu_date_unambiguous() {
        // Day 25 month 12 -> only valid as EU (25 is not a valid month)
        // But US format is tried first, and 25/12/2024 as US would be month=25 (invalid)
        // So it falls through to EU
        let results = detect_dates(&["25/12/2024"]);
        assert_eq!(results.len(), 1);
        let expected = date_to_epoch(2024, 12, 25).unwrap_or(0);
        assert_eq!(results[0].1.epoch_seconds, expected);
    }

    // ---- Epoch seconds ----

    #[test]
    fn parse_epoch_seconds() {
        let results = detect_dates(&["1705334400"]); // 2024-01-15T16:00:00Z
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.epoch_seconds, 1_705_334_400);
    }

    #[test]
    fn parse_epoch_out_of_range() {
        // Before 2000
        let results = detect_dates(&["946684799"]);
        assert!(results.is_empty());

        // After 2050
        let results = detect_dates(&["2524608001"]);
        assert!(results.is_empty());
    }

    // ---- Monotonicity ----

    #[test]
    fn monotonic_sorted_timestamps() {
        let timestamps = vec![100, 200, 300, 400, 500];
        let report = temporal_analysis(&timestamps);
        assert!(report.is_monotonic);
    }

    #[test]
    fn monotonic_unsorted_timestamps() {
        let timestamps = vec![100, 300, 200, 400, 500];
        let report = temporal_analysis(&timestamps);
        assert!(!report.is_monotonic);
    }

    #[test]
    fn monotonic_with_duplicates_is_not_strict() {
        let timestamps = vec![100, 200, 200, 300];
        let report = temporal_analysis(&timestamps);
        assert!(!report.is_monotonic); // strictly increasing required
    }

    #[test]
    fn monotonic_single_timestamp() {
        let report = temporal_analysis(&[100]);
        assert!(report.is_monotonic);
    }

    #[test]
    fn monotonic_empty() {
        let report = temporal_analysis(&[]);
        assert!(report.is_monotonic);
    }

    // ---- Gap detection ----

    #[test]
    fn gap_detection_regular_with_one_big_gap() {
        // Regular 100s intervals with one 10000s gap
        let mut timestamps = Vec::new();
        let mut t = 1000_i64;
        for _ in 0..10 {
            timestamps.push(t);
            t += 100;
        }
        // Insert a big gap
        t += 10000;
        timestamps.push(t);
        // Then regular again
        for _ in 0..5 {
            t += 100;
            timestamps.push(t);
        }

        let report = temporal_analysis(&timestamps);
        assert!(
            !report.gaps.is_empty(),
            "should detect the large gap in otherwise regular intervals"
        );
        // The big gap should be around index 9 (between element 9 and 10)
        assert!(report.gaps.iter().any(|g| g.interval > 5000));
    }

    #[test]
    fn gap_detection_no_gaps_when_uniform() {
        // Perfectly uniform intervals -> MAD is 0, threshold is median + 0 = median
        // No interval exceeds its own value (threshold > 0 check prevents false positives)
        let timestamps: Vec<i64> = (0..20).map(|i| 1000 + i * 100).collect();
        let report = temporal_analysis(&timestamps);
        assert!(
            report.gaps.is_empty(),
            "uniform intervals should have no gaps"
        );
    }

    // ---- Duplicate detection ----

    #[test]
    fn duplicate_count_none() {
        let timestamps = vec![100, 200, 300];
        let report = temporal_analysis(&timestamps);
        assert_eq!(report.duplicate_count, 0);
    }

    #[test]
    fn duplicate_count_some() {
        let timestamps = vec![100, 200, 200, 300, 300, 300];
        let report = temporal_analysis(&timestamps);
        // 200 appears twice (1 duplicate), 300 appears three times (2 duplicates) = 3
        assert_eq!(report.duplicate_count, 3);
    }

    #[test]
    fn duplicate_count_empty() {
        let report = temporal_analysis(&[]);
        assert_eq!(report.duplicate_count, 0);
    }

    // ---- Interval stats ----

    #[test]
    fn interval_stats_present_when_enough_data() {
        let timestamps = vec![100, 200, 400, 500];
        let report = temporal_analysis(&timestamps);
        assert!(report.interval_stats.is_some());
    }

    #[test]
    fn interval_stats_none_for_single() {
        let report = temporal_analysis(&[100]);
        assert!(report.interval_stats.is_none());
    }

    // ---- Date validation helpers ----

    #[test]
    fn leap_year_checks() {
        assert!(is_leap_year(2000)); // divisible by 400
        assert!(!is_leap_year(1900)); // divisible by 100 but not 400
        assert!(is_leap_year(2024)); // divisible by 4 but not 100
        assert!(!is_leap_year(2023)); // not divisible by 4
    }

    #[test]
    fn days_in_month_checks() {
        assert_eq!(days_in_month(2024, 1), 31);
        assert_eq!(days_in_month(2024, 2), 29); // leap
        assert_eq!(days_in_month(2023, 2), 28); // non-leap
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 12), 31);
    }

    #[test]
    fn epoch_computation_known_dates() {
        // 1970-01-01 should be 0
        assert_eq!(date_to_epoch(1970, 1, 1), Some(0));
        // 1970-01-02 should be 86400
        assert_eq!(date_to_epoch(1970, 1, 2), Some(86400));
        // 2000-01-01 should be 946684800
        assert_eq!(date_to_epoch(2000, 1, 1), Some(946_684_800));
    }

    #[test]
    fn detect_dates_mixed_formats() {
        let values = &[
            "2024-01-15",
            "not a date",
            "01/20/2024",
            "1705334400",
            "",
        ];
        let results = detect_dates(values);
        // Should detect index 0 (ISO), 2 (US), 3 (epoch)
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 2);
        assert_eq!(results[2].0, 3);
    }
}
