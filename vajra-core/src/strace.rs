//! Parser for `strace -c` summary output.
//!
//! Converts the tabular summary produced by `strace -c` into a JSON array of
//! per-syscall records.

use vajra_types::VajraError;

/// Parse `strace -c` summary text into a JSON array string.
///
/// Each element has the shape:
/// ```json
/// {"syscall": "futex", "time_pct": 24.44, "seconds": 0.01326,
///  "usecs_per_call": 4, "calls": 2856, "errors": 228}
/// ```
///
/// The header and separator lines are skipped. The trailing `"total"` line is
/// excluded from the output array.
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if the input does not contain a valid strace
/// summary table.
pub fn parse_strace(input: &str) -> Result<String, VajraError> {
    let lines: Vec<&str> = input.lines().collect();

    // Find the first separator line (------...)
    let separator_idx = lines
        .iter()
        .position(|line| line.trim_start().starts_with("------"))
        .ok_or_else(|| VajraError::Parse {
            byte_offset: 0,
            message: "strace summary: no separator line (------) found".to_string(),
            source_path: None,
        })?;

    // Data lines start after the first separator, end at the next separator or
    // the "total" line.
    let data_start = separator_idx + 1;

    let mut records: Vec<serde_json::Value> = Vec::new();

    for (line_offset, line) in lines.iter().enumerate().skip(data_start) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Stop at the closing separator
        if trimmed.starts_with("------") {
            break;
        }
        // Skip the "total" line (may appear without a closing separator)
        if trimmed.starts_with("total") {
            break;
        }

        let record = parse_strace_data_line(trimmed, line_offset)?;
        records.push(record);
    }

    if records.is_empty() {
        return Err(VajraError::Parse {
            byte_offset: 0,
            message: "strace summary: no data rows found after separator".to_string(),
            source_path: None,
        });
    }

    serde_json::to_string(&records).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("strace JSON serialization error: {e}"),
        source_path: None,
    })
}

/// Parse a single strace data line.
///
/// Expected columns (whitespace-separated):
///   `% time | seconds | usecs/call | calls | errors | syscall`
///
/// The `errors` column may be absent (when there are no errors for that syscall),
/// which shifts the syscall name one position left.
fn parse_strace_data_line(line: &str, line_number: usize) -> Result<serde_json::Value, VajraError> {
    let fields: Vec<&str> = line.split_whitespace().collect();

    // Minimum 5 fields (no errors column), maximum 6 (with errors column).
    if fields.len() < 5 {
        return Err(VajraError::Parse {
            byte_offset: line_number,
            message: format!(
                "strace data line {}: expected at least 5 fields, got {}",
                line_number + 1,
                fields.len()
            ),
            source_path: None,
        });
    }

    let time_pct: f64 = fields[0].parse().map_err(|_| VajraError::Parse {
        byte_offset: line_number,
        message: format!(
            "strace line {}: invalid time_pct '{}'",
            line_number + 1,
            fields[0]
        ),
        source_path: None,
    })?;

    let seconds: f64 = fields[1].parse().map_err(|_| VajraError::Parse {
        byte_offset: line_number,
        message: format!(
            "strace line {}: invalid seconds '{}'",
            line_number + 1,
            fields[1]
        ),
        source_path: None,
    })?;

    let usecs_per_call: u64 = fields[2].parse().map_err(|_| VajraError::Parse {
        byte_offset: line_number,
        message: format!(
            "strace line {}: invalid usecs/call '{}'",
            line_number + 1,
            fields[2]
        ),
        source_path: None,
    })?;

    let calls: u64 = fields[3].parse().map_err(|_| VajraError::Parse {
        byte_offset: line_number,
        message: format!(
            "strace line {}: invalid calls '{}'",
            line_number + 1,
            fields[3]
        ),
        source_path: None,
    })?;

    // Determine whether field 4 is the errors column or the syscall name.
    let (errors, syscall) = if fields.len() >= 6 {
        if let Ok(err_count) = fields[4].parse::<u64>() {
            (err_count, fields[5])
        } else {
            (0, fields[4])
        }
    } else {
        (0, fields[4])
    };

    let time_pct_rounded = (time_pct * 100.0).round() / 100.0;
    let seconds_rounded = (seconds * 1_000_000.0).round() / 1_000_000.0;

    Ok(serde_json::json!({
        "syscall": syscall,
        "time_pct": time_pct_rounded,
        "seconds": seconds_rounded,
        "usecs_per_call": usecs_per_call,
        "calls": calls,
        "errors": errors,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_strace() -> &'static str {
        "% time     seconds  usecs/call     calls    errors syscall\n\
         ------ ----------- ----------- --------- --------- ------------------\n \
         24.44    0.013260           4      2856       228 futex\n \
         22.23    0.012065          17       675           munmap\n \
         10.50    0.005700           2      3100         0 read\n\
         ------ ----------- ----------- --------- --------- ------------------\n\
         100.00    0.054270                  6631       228 total\n"
    }

    #[test]
    fn parse_basic_strace() {
        let result = parse_strace(sample_strace());
        assert!(result.is_ok(), "parse failed: {result:?}");
        let json_str = result.unwrap_or_default();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["syscall"], "futex");
        assert_eq!(arr[0]["time_pct"], 24.44);
        assert_eq!(arr[0]["calls"], 2856);
        assert_eq!(arr[0]["errors"], 228);
        assert_eq!(arr[1]["syscall"], "munmap");
        assert_eq!(arr[1]["errors"], 0);
    }

    #[test]
    fn no_separator_returns_error() {
        let input = "some random text\nno separator here\n";
        let result = parse_strace(input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("separator"),
            "error should mention separator: {msg}"
        );
    }

    #[test]
    fn no_data_rows_returns_error() {
        let input = "% time     seconds  usecs/call     calls    errors syscall\n\
                      ------ ----------- ----------- --------- --------- ------\n\
                      ------ ----------- ----------- --------- --------- ------\n";
        let result = parse_strace(input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no data rows"),
            "error should mention no data rows: {msg}"
        );
    }

    #[test]
    fn malformed_line_returns_error() {
        let input = "% time     seconds  usecs/call     calls    errors syscall\n\
                      ------ ----------- ----------- --------- --------- ------\n\
                      not enough fields\n\
                      ------ ----------- ----------- --------- --------- ------\n";
        let result = parse_strace(input);
        assert!(result.is_err());
    }

    #[test]
    fn deterministic_output() {
        let first = parse_strace(sample_strace()).unwrap_or_default();
        for _ in 0..10 {
            let again = parse_strace(sample_strace()).unwrap_or_default();
            assert_eq!(first, again);
        }
    }

    #[test]
    fn handles_total_line_without_closing_separator() {
        let input = "% time     seconds  usecs/call     calls    errors syscall\n\
                      ------ ----------- ----------- --------- --------- ------\n \
                      50.00    0.001000           1      1000           write\n\
                      total    0.001000                  1000           total\n";
        let result = parse_strace(input);
        assert!(result.is_ok(), "parse failed: {result:?}");
        let json_str = result.unwrap_or_default();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["syscall"], "write");
    }
}
