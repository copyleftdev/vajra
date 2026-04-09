//! Temporal pattern analysis for date/time data.
//!
//! Provides date detection from string values, monotonicity checks,
//! gap detection, and interval statistics for timestamp sequences.

use std::collections::BTreeMap;

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
    if !(1..=12).contains(&month) || day < 1 {
        return None;
    }
    if !(1970..=2100).contains(&year) {
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
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// ---------------------------------------------------------------------------
// Time-series windowing primitives
// ---------------------------------------------------------------------------

/// Granularity at which timestamps are bucketed into windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowGranularity {
    /// Monthly windows
    Month,
    /// ISO week windows
    Week,
    /// Daily windows
    Day,
}

/// Result of a linear regression on (index, value) pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendLine {
    pub slope: f64,
    pub direction: String,
    pub r_squared: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSummary {
    pub window: String,
    pub record_count: usize,
    pub field_stats: BTreeMap<String, FieldWindowStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWindowStats {
    pub entropy: f64,
    pub cardinality: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowedAnalysisResult {
    pub windows: Vec<WindowSummary>,
    pub trends: BTreeMap<String, TrendLine>,
}

#[must_use]
pub fn truncate_to_window(epoch_seconds: i64, granularity: WindowGranularity) -> Option<String> {
    let (year, month, day) = epoch_to_civil(epoch_seconds)?;
    match granularity {
        WindowGranularity::Month => Some(format!("{year:04}-{month:02}")),
        WindowGranularity::Day => Some(format!("{year:04}-{month:02}-{day:02}")),
        WindowGranularity::Week => {
            let (iso_year, iso_week) = iso_week_number(year, month, day);
            Some(format!("{iso_year:04}-W{iso_week:02}"))
        }
    }
}

pub fn bucket_by_window<V>(
    records: impl IntoIterator<Item = (i64, V)>,
    granularity: WindowGranularity,
) -> BTreeMap<String, Vec<V>> {
    let mut buckets: BTreeMap<String, Vec<V>> = BTreeMap::new();
    for (epoch, value) in records {
        if let Some(label) = truncate_to_window(epoch, granularity) {
            buckets.entry(label).or_default().push(value);
        }
    }
    buckets
}

#[must_use]
pub fn linear_regression(values: &[f64]) -> Option<TrendLine> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let n_f = n as f64;
    #[allow(clippy::cast_precision_loss)]
    let x_mean = (n_f - 1.0) / 2.0;
    let y_mean: f64 = values.iter().sum::<f64>() / n_f;
    let mut ss_xy = 0.0_f64;
    let mut ss_xx = 0.0_f64;
    let mut ss_yy = 0.0_f64;
    for (i, &y) in values.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64;
        let dx = x - x_mean;
        let dy = y - y_mean;
        ss_xy += dx * dy;
        ss_xx += dx * dx;
        ss_yy += dy * dy;
    }
    if ss_xx < f64::EPSILON {
        return None;
    }
    let slope = ss_xy / ss_xx;
    let r_squared = if ss_yy < f64::EPSILON {
        1.0
    } else {
        (ss_xy * ss_xy) / (ss_xx * ss_yy)
    };
    let r_squared = r_squared.clamp(0.0, 1.0);
    let direction = if r_squared > 0.3 {
        if slope > 0.0 {
            "increasing"
        } else {
            "decreasing"
        }
    } else {
        "stable"
    };
    Some(TrendLine {
        slope,
        direction: direction.to_owned(),
        r_squared,
    })
}

#[must_use]
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if let Some(tv) = try_parse_iso_datetime_tz(trimmed) {
        return Some(tv);
    }
    if let Some(tv) = try_parse_iso_datetime(trimmed) {
        return Some(tv.epoch_seconds);
    }
    if let Some(tv) = try_parse_iso_date(trimmed) {
        return Some(tv.epoch_seconds);
    }
    None
}

fn try_parse_iso_datetime_tz(s: &str) -> Option<i64> {
    if s.len() < 20 {
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
    let utc_epoch = date_epoch + time_offset;
    let tz_part = &s[19..];
    let tz_offset_seconds = parse_tz_offset(tz_part)?;
    Some(utc_epoch - tz_offset_seconds)
}

fn parse_tz_offset(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[0] == b'Z' || bytes[0] == b'z' {
        return Some(0);
    }
    if s.len() < 6 {
        return None;
    }
    let sign: i64 = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    if bytes[3] != b':' {
        return None;
    }
    let tz_hours = parse_u32(&s[1..3])?;
    let tz_mins = parse_u32(&s[4..6])?;
    if tz_hours > 23 || tz_mins > 59 {
        return None;
    }
    Some(sign * i64::from(tz_hours * 3600 + tz_mins * 60))
}

fn epoch_to_civil(epoch_seconds: i64) -> Option<(u32, u32, u32)> {
    if epoch_seconds < 0 {
        return None;
    }
    let day_count = epoch_seconds / 86400;
    let z = day_count + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    #[allow(clippy::cast_possible_wrap)]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    #[allow(clippy::cast_possible_wrap)]
    let y = if m <= 2 { y + 1 } else { y };
    if !(1970..=2100).contains(&y) {
        return None;
    }
    #[allow(clippy::cast_sign_loss)]
    Some((y as u32, m, d))
}

fn iso_week_number(year: u32, month: u32, day: u32) -> (u32, u32) {
    let mut doy: u32 = day;
    for m in 1..month {
        doy += days_in_month(year, m);
    }
    let epoch = date_to_epoch(year, month, day).unwrap_or(0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dow = ((epoch / 86400) % 7 + 3) % 7;
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    {
        let dow_i = dow as i32;
        let doy_i = doy as i32;
        let thursday = doy_i + (3 - dow_i);
        if thursday < 1 {
            let prev_dec31_epoch = date_to_epoch(year - 1, 12, 31).unwrap_or(0);
            let prev_dow = ((prev_dec31_epoch / 86400) % 7 + 3) % 7;
            let prev_doy: i32 = if is_leap_year(year - 1) { 366 } else { 365 };
            let prev_thursday = prev_doy + (3 - prev_dow as i32);
            return (year - 1, ((prev_thursday - 1) / 7 + 1) as u32);
        }
        let days_in_year: i32 = if is_leap_year(year) { 366 } else { 365 };
        if thursday > days_in_year {
            return (year + 1, 1);
        }
        (year, ((thursday - 1) / 7 + 1) as u32)
    }
}

#[must_use]
pub fn extract_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let path = path
        .strip_prefix("$.")
        .unwrap_or(path.strip_prefix('$').unwrap_or(path));
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        if let Some(bracket_pos) = segment.find('[') {
            let field = &segment[..bracket_pos];
            if !field.is_empty() {
                current = current.get(field)?;
            }
            let end = segment.len().checked_sub(1)?;
            let idx_str = segment.get(bracket_pos + 1..end)?;
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

#[must_use]
pub fn value_to_epoch(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::String(s) => parse_iso8601(s),
        serde_json::Value::Number(n) => {
            #[allow(clippy::cast_possible_truncation)]
            let epoch = n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?;
            if (946_684_800..=4_102_444_800).contains(&epoch) {
                Some(epoch)
            } else {
                None
            }
        }
        _ => None,
    }
}

const TIME_FIELD_HINTS: &[&str] = &[
    "date",
    "time",
    "timestamp",
    "created",
    "updated",
    "created_at",
    "updated_at",
    "datetime",
];

#[must_use]
pub fn auto_detect_time_field(records: &[serde_json::Value]) -> Option<String> {
    let sample = records.first()?;
    let obj = sample.as_object()?;
    for (key, value) in obj {
        let lower = key.to_lowercase();
        let is_hint = TIME_FIELD_HINTS.iter().any(|h| lower.contains(h));
        if is_hint && value_to_epoch(value).is_some() {
            return Some(format!("$.{key}"));
        }
    }
    None
}

pub fn windowed_analysis(
    records: &[serde_json::Value],
    time_field: &str,
    granularity: WindowGranularity,
) -> Result<WindowedAnalysisResult, String> {
    use crate::entropy::shannon_entropy_from_counts;
    use crate::frequency::FrequencyCounter;
    if records.is_empty() {
        return Err("no records provided".to_owned());
    }
    let mut timestamped: Vec<(i64, &serde_json::Value)> = Vec::new();
    let mut skipped = 0_usize;
    for record in records {
        if let Some(tv) = extract_json_path(record, time_field).and_then(value_to_epoch) {
            timestamped.push((tv, record));
        } else {
            skipped += 1;
        }
    }
    if timestamped.is_empty() {
        return Err(format!(
            "no valid timestamps found at path '{time_field}' (skipped {skipped} records)"
        ));
    }
    if skipped > 0 {
        eprintln!("vajra: warning: skipped {skipped} records with invalid/missing timestamps");
    }
    let buckets = bucket_by_window(timestamped, granularity);
    let field_paths: Vec<String> = {
        let mut paths = Vec::new();
        if let Some(first) = records.iter().find_map(|r| r.as_object()) {
            for key in first.keys() {
                let path = format!("$.{key}");
                if path != time_field {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    };
    let mut window_summaries: Vec<WindowSummary> = Vec::new();
    for (label, window_records) in &buckets {
        let mut field_stats_map: BTreeMap<String, FieldWindowStats> = BTreeMap::new();
        for field_path in &field_paths {
            let mut counter = FrequencyCounter::new();
            let mut value_count = 0_u64;
            for record in window_records {
                if let Some(val) = extract_json_path(record, field_path) {
                    let val_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => "null".to_owned(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    counter.observe_raw(field_path, &val_str);
                    value_count += 1;
                }
            }
            if value_count > 0 {
                let counts = counter.count_values_raw(field_path);
                let entropy = shannon_entropy_from_counts(&counts);
                let cardinality = counter.cardinality_raw(field_path);
                field_stats_map.insert(
                    field_path.clone(),
                    FieldWindowStats {
                        entropy,
                        cardinality,
                    },
                );
            }
        }
        window_summaries.push(WindowSummary {
            window: label.clone(),
            record_count: window_records.len(),
            field_stats: field_stats_map,
        });
    }
    let mut trends: BTreeMap<String, TrendLine> = BTreeMap::new();
    #[allow(clippy::cast_precision_loss)]
    let counts: Vec<f64> = window_summaries
        .iter()
        .map(|w| w.record_count as f64)
        .collect();
    if let Some(trend) = linear_regression(&counts) {
        trends.insert("record_count".to_owned(), trend);
    }
    for field_path in &field_paths {
        let entropies: Vec<f64> = window_summaries
            .iter()
            .filter_map(|w| w.field_stats.get(field_path).map(|s| s.entropy))
            .collect();
        if entropies.len() >= 2 {
            if let Some(t) = linear_regression(&entropies) {
                trends.insert(format!("{field_path}.entropy"), t);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let cards: Vec<f64> = window_summaries
            .iter()
            .filter_map(|w| w.field_stats.get(field_path).map(|s| s.cardinality as f64))
            .collect();
        if cards.len() >= 2 {
            if let Some(t) = linear_regression(&cards) {
                trends.insert(format!("{field_path}.cardinality"), t);
            }
        }
    }
    Ok(WindowedAnalysisResult {
        windows: window_summaries,
        trends,
    })
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
        let values = &["2024-01-15", "not a date", "01/20/2024", "1705334400", ""];
        let results = detect_dates(values);
        // Should detect index 0 (ISO), 2 (US), 3 (epoch)
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 2);
        assert_eq!(results[2].0, 3);
    }

    // ---- Windowing tests ----

    #[test]
    fn truncate_month() {
        let e = date_to_epoch(2025, 6, 15).unwrap_or(0);
        assert_eq!(
            truncate_to_window(e, WindowGranularity::Month),
            Some("2025-06".to_owned())
        );
    }

    #[test]
    fn truncate_day() {
        let e = date_to_epoch(2025, 6, 15).unwrap_or(0);
        assert_eq!(
            truncate_to_window(e, WindowGranularity::Day),
            Some("2025-06-15".to_owned())
        );
    }

    #[test]
    fn truncate_week() {
        let e = date_to_epoch(2025, 6, 15).unwrap_or(0);
        let label = truncate_to_window(e, WindowGranularity::Week).unwrap_or_default();
        assert!(label.starts_with("2025-W"));
    }

    #[test]
    fn bucket_groups() {
        let recs = vec![
            (date_to_epoch(2025, 1, 10).unwrap_or(0), "a"),
            (date_to_epoch(2025, 1, 20).unwrap_or(0), "b"),
            (date_to_epoch(2025, 2, 5).unwrap_or(0), "c"),
        ];
        let b = bucket_by_window(recs, WindowGranularity::Month);
        assert_eq!(b.len(), 2);
        assert_eq!(b.get("2025-01").map(|v| v.len()), Some(2));
    }

    #[test]
    fn regression_increasing() {
        let t = linear_regression(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(t.is_some());
        let t = t.unwrap_or_else(|| TrendLine {
            slope: 0.0,
            direction: String::new(),
            r_squared: 0.0,
        });
        assert!((t.slope - 1.0).abs() < 1e-10);
        assert_eq!(t.direction, "increasing");
    }

    #[test]
    fn regression_decreasing() {
        let t = linear_regression(&[10.0, 8.0, 6.0, 4.0, 2.0]);
        assert!(t.is_some());
        let t = t.unwrap_or_else(|| TrendLine {
            slope: 0.0,
            direction: String::new(),
            r_squared: 0.0,
        });
        assert!((t.slope + 2.0).abs() < 1e-10);
        assert_eq!(t.direction, "decreasing");
    }

    #[test]
    fn regression_stable() {
        let t = linear_regression(&[3.0, 7.0, 2.0, 8.0, 1.0, 9.0]);
        assert!(t.is_some());
        assert_eq!(
            t.unwrap_or_else(|| TrendLine {
                slope: 0.0,
                direction: String::new(),
                r_squared: 0.0
            })
            .direction,
            "stable"
        );
    }

    #[test]
    fn regression_too_few() {
        assert!(linear_regression(&[]).is_none());
        assert!(linear_regression(&[1.0]).is_none());
    }

    #[test]
    fn parse_iso8601_z() {
        let e = parse_iso8601("2025-06-15T12:00:00Z");
        assert_eq!(e, Some(date_to_epoch(2025, 6, 15).unwrap_or(0) + 12 * 3600));
    }

    #[test]
    fn parse_iso8601_plus_offset() {
        let e = parse_iso8601("2025-06-15T12:00:00+02:00");
        assert_eq!(e, Some(date_to_epoch(2025, 6, 15).unwrap_or(0) + 10 * 3600));
    }

    #[test]
    fn parse_iso8601_minus_offset() {
        let e = parse_iso8601("2025-06-15T12:00:00-05:00");
        assert_eq!(e, Some(date_to_epoch(2025, 6, 15).unwrap_or(0) + 17 * 3600));
    }

    #[test]
    fn extract_simple() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"date":"2025-01-01"}"#).unwrap_or_default();
        assert_eq!(
            extract_json_path(&v, "$.date").and_then(|v| v.as_str()),
            Some("2025-01-01")
        );
    }

    #[test]
    fn auto_detect_date() {
        let recs = vec![serde_json::json!({"date": "2025-06-15", "author": "Alice"})];
        assert_eq!(auto_detect_time_field(&recs).as_deref(), Some("$.date"));
    }

    #[test]
    fn windowed_three_months() {
        let mut recs = Vec::new();
        for d in [5, 10, 15, 20] {
            recs.push(serde_json::json!({"date": format!("2025-01-{d:02}"), "author": "Alice"}));
        }
        for d in [5, 10, 15, 20] {
            recs.push(serde_json::json!({"date": format!("2025-02-{d:02}"), "author": "Bob"}));
        }
        for d in [5, 10, 15, 20] {
            recs.push(serde_json::json!({"date": format!("2025-03-{d:02}"), "author": "Carol"}));
        }
        let r = windowed_analysis(&recs, "$.date", WindowGranularity::Month);
        assert!(r.is_ok());
        let r = r.unwrap_or_else(|_| WindowedAnalysisResult {
            windows: Vec::new(),
            trends: BTreeMap::new(),
        });
        assert_eq!(r.windows.len(), 3);
        assert_eq!(r.windows[0].window, "2025-01");
        assert_eq!(r.windows[0].record_count, 4);
        assert!(r.trends.contains_key("record_count"));
    }

    #[test]
    fn windowed_missing_field() {
        assert!(windowed_analysis(
            &[serde_json::json!({"name": "Alice"})],
            "$.date",
            WindowGranularity::Month
        )
        .is_err());
    }

    #[test]
    fn windowed_empty() {
        let recs: Vec<serde_json::Value> = Vec::new();
        assert!(windowed_analysis(&recs, "$.date", WindowGranularity::Month).is_err());
    }

    #[test]
    fn epoch_to_civil_roundtrip() {
        for (y, m, d) in [(1970, 1, 1), (2000, 1, 1), (2024, 2, 29), (2025, 6, 15)] {
            let e = date_to_epoch(y, m, d).unwrap_or(-1);
            assert_eq!(epoch_to_civil(e), Some((y, m, d)));
        }
    }
}
