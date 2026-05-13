//! TSV/CSV reader for local data files.
//!
//! Reads a delimited text file into a `Vec<HashMap<String, serde_json::Value>>`.
//! Column types are auto-detected: a column whose every non-empty value parses
//! as `f64` becomes `Value::Number`; everything else stays `Value::String`.
//!
//! # Auto-detection of delimiter
//!
//! When a file path is available, `detect_delimiter` infers the separator
//! from the extension:
//!
//! | Extension         | Separator |
//! |-------------------|-----------|
//! | `.tsv`, `.tab`    | `\t`      |
//! | `.csv`            | `,`       |
//! | anything else     | `\t` (default) |
//!
//! Pass `delimiter` explicitly to `read_delimited` to override.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;
use thiserror::Error;

/// A parsed delimited file: ordered column headers paired with row maps.
pub type ParsedDelimited = (Vec<String>, Vec<HashMap<String, Value>>);

/// Errors that can occur while reading a local data file.
#[derive(Debug, Error)]
pub enum TsvError {
    /// The file has no header row (first non-comment line is empty or absent).
    #[error("data file is empty or has no header row")]
    EmptyFile,

    /// A data row has more columns than the header.
    #[error("row {row} has {got} columns but header has {expected}")]
    ColumnCountMismatch {
        row: usize,
        expected: usize,
        got: usize,
    },
}

/// Infer the field delimiter from a file extension.
///
/// Returns `'\t'` for `.tsv` / `.tab`, `','` for `.csv`, and `'\t'` for
/// everything else (including `None`, i.e. stdin).
pub fn detect_delimiter(path: Option<&Path>) -> char {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "csv" => ',',
        _ => '\t',
    }
}

/// Parse a delimited text file into a list of row maps, also returning column headers.
///
/// Identical to [`read_delimited`] but additionally returns the ordered header
/// list so callers can resolve positional axis defaults correctly.
///
/// # Returns
///
/// `Ok((headers, rows))` where `headers` is the ordered column name list.
pub fn read_delimited_with_headers(
    content: &str,
    delimiter: char,
) -> Result<ParsedDelimited, TsvError> {
    let mut lines = content
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());

    let header_line = lines.next().ok_or(TsvError::EmptyFile)?;
    let raw_headers: Vec<&str> = header_line.split(delimiter).collect();
    let headers: Vec<String> = raw_headers.iter().map(|h| h.trim().to_string()).collect();
    let n_cols = headers.len();

    // Collect raw string rows
    let mut raw_rows: Vec<Vec<Option<&str>>> = Vec::new();
    for (idx, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split(delimiter).collect();
        if cells.len() > n_cols {
            return Err(TsvError::ColumnCountMismatch {
                row: idx + 1,
                expected: n_cols,
                got: cells.len(),
            });
        }
        let padded: Vec<Option<&str>> = (0..n_cols)
            .map(|i| {
                cells.get(i).and_then(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                })
            })
            .collect();
        raw_rows.push(padded);
    }

    // Auto-detect column types: numeric iff every non-null value parses as f64
    let numeric: Vec<bool> = (0..n_cols)
        .map(|col_idx| {
            raw_rows
                .iter()
                .filter_map(|row| row[col_idx])
                .all(|v| v.parse::<f64>().is_ok())
        })
        .collect();

    let rows = raw_rows
        .into_iter()
        .map(|raw| {
            let mut map = HashMap::with_capacity(n_cols);
            for (col_idx, cell) in raw.into_iter().enumerate() {
                let key = headers[col_idx].clone();
                let val = match cell {
                    None => Value::Null,
                    Some(s) if numeric[col_idx] => {
                        let f: f64 = s.parse().unwrap();
                        serde_json::Number::from_f64(f)
                            .map(Value::Number)
                            .unwrap_or_else(|| Value::String(s.to_string()))
                    }
                    Some(s) => Value::String(s.to_string()),
                };
                map.insert(key, val);
            }
            map
        })
        .collect();

    Ok((headers, rows))
}

/// Parse a delimited text file into a list of row maps.
///
/// - Lines starting with `#` are skipped (comments / BUSCO-style headers).
/// - The first non-comment line is treated as the header.
/// - Empty lines are skipped.
/// - Column types are auto-detected after reading all rows: numeric columns
///   (every non-empty value parses as `f64`) are stored as `Value::Number`;
///   everything else as `Value::String`.
/// - Empty cells are stored as `Value::Null`.
///
/// # Errors
///
/// Returns [`TsvError::EmptyFile`] when no header row is found.
/// Returns [`TsvError::ColumnCountMismatch`] when a data row exceeds the
/// header width (extra trailing columns are silently dropped if there are
/// *fewer* values than headers — those cells become `Null`).
pub fn read_delimited(
    content: &str,
    delimiter: char,
) -> Result<Vec<HashMap<String, Value>>, TsvError> {
    read_delimited_with_headers(content, delimiter).map(|(_, rows)| rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_delimiter_tsv() {
        assert_eq!(detect_delimiter(Some(Path::new("data.tsv"))), '\t');
        assert_eq!(detect_delimiter(Some(Path::new("data.tab"))), '\t');
    }

    #[test]
    fn detect_delimiter_csv() {
        assert_eq!(detect_delimiter(Some(Path::new("data.csv"))), ',');
    }

    #[test]
    fn detect_delimiter_unknown_defaults_to_tab() {
        assert_eq!(detect_delimiter(Some(Path::new("data.txt"))), '\t');
        assert_eq!(detect_delimiter(None), '\t');
    }

    #[test]
    fn read_delimited_basic_tsv() {
        let content = "name\tvalue\nalpha\t1.5\nbeta\t2.0\n";
        let rows = read_delimited(content, '\t').unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], Value::String("alpha".to_string()));
        // "value" column is all-numeric
        assert!(rows[0]["value"].is_number());
        assert_eq!(rows[0]["value"].as_f64().unwrap(), 1.5);
    }

    #[test]
    fn read_delimited_skips_comments() {
        let content = "# comment\nfield\tcount\na\t10\nb\t20\n";
        let rows = read_delimited(content, '\t').unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn read_delimited_mixed_column_stays_string() {
        let content = "x\ta\t2.0\nb\t3.0\n";
        // "x" col has "a" and "b" — not numeric
        let rows = read_delimited(content, '\t').unwrap();
        assert!(rows[0]["x"].is_string());
    }

    #[test]
    fn read_delimited_null_cells() {
        let content = "x\ty\n1\t\n2\t3\n";
        let rows = read_delimited(content, '\t').unwrap();
        assert_eq!(rows[0]["y"], Value::Null);
        assert!(rows[1]["y"].is_number());
    }

    #[test]
    fn read_delimited_empty_file_errors() {
        let result = read_delimited("", '\t');
        assert!(matches!(result, Err(TsvError::EmptyFile)));
    }

    #[test]
    fn read_delimited_too_many_columns_errors() {
        let content = "a\tb\n1\t2\t3\n";
        let result = read_delimited(content, '\t');
        assert!(matches!(result, Err(TsvError::ColumnCountMismatch { .. })));
    }
}
