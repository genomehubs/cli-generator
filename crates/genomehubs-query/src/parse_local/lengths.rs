//! Parser for explicit `sequence_id<TAB>length` TSV files.
//!
//! Two-column format: first column is the sequence / chromosome identifier,
//! second column is the length in base-pairs.  Comment lines (`#`) and blank
//! lines are skipped.

use std::collections::HashMap;

/// Errors that can occur when parsing a lengths TSV.
#[derive(Debug)]
pub enum ParseError {
    /// A data line had too few columns.
    TooFewColumns { line: usize },
    /// The length value could not be parsed as a `u64`.
    InvalidLength { line: usize, value: String },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewColumns { line } => {
                write!(f, "lengths TSV line {line}: missing length column")
            }
            Self::InvalidLength { line, value } => {
                write!(f, "lengths TSV line {line}: cannot parse length '{value}'")
            }
        }
    }
}

/// Parse a two-column `sequence_id<TAB>length` TSV.
///
/// Returns a `sequence_id → length` map on success.
pub fn parse_lengths_tsv(content: &str) -> Result<HashMap<String, u64>, ParseError> {
    let mut lengths: HashMap<String, u64> = HashMap::new();

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line_no = line_idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut cols = trimmed.splitn(2, '\t');
        let name = match cols.next() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let length_str = match cols.next() {
            Some(s) => s.split('\t').next().unwrap_or(s).trim(),
            None => return Err(ParseError::TooFewColumns { line: line_no }),
        };

        let length: u64 = length_str.parse().map_err(|_| ParseError::InvalidLength {
            line: line_no,
            value: length_str.to_string(),
        })?;

        lengths.insert(name, length);
    }

    Ok(lengths)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# optional header
chr1\t248956422
chr2\t242193529

chrX\t156040895
";

    #[test]
    fn test_parse_three_sequences() {
        let m = parse_lengths_tsv(SAMPLE).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m["chr1"], 248_956_422);
    }

    #[test]
    fn test_skips_blank_and_comment() {
        let content = "# comment\n\nchr1\t100\n";
        let m = parse_lengths_tsv(content).unwrap();
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn test_error_on_missing_length_column() {
        let result = parse_lengths_tsv("chr1\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_on_non_numeric_length() {
        let result = parse_lengths_tsv("chr1\tabc\n");
        assert!(result.is_err());
    }
}
