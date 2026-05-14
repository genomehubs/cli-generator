//! Parser for samtools `.fai` FASTA index files.
//!
//! Only the first two columns (`NAME`, `LENGTH`) are used.

use std::collections::HashMap;

/// Errors that can occur when parsing a `.fai` index.
#[derive(Debug)]
pub enum ParseError {
    /// A line had only one column (or was otherwise unparseable).
    TooFewColumns { line: usize },
    /// The `LENGTH` column could not be parsed as a `u64`.
    InvalidLength { line: usize, value: String },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewColumns { line } => write!(f, ".fai line {line}: too few columns"),
            Self::InvalidLength { line, value } => {
                write!(f, ".fai line {line}: cannot parse length '{value}'")
            }
        }
    }
}

/// Parse a samtools `.fai` index and return a `sequence_id → length` map.
///
/// Blank lines and lines starting with `#` are skipped.
pub fn parse_fai(content: &str) -> Result<HashMap<String, u64>, ParseError> {
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

        let rest = match cols.next() {
            Some(r) => r,
            None => return Err(ParseError::TooFewColumns { line: line_no }),
        };

        let length_str = rest.split('\t').next().unwrap_or(rest);
        let length: u64 = length_str
            .trim()
            .parse()
            .map_err(|_| ParseError::InvalidLength {
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
chr1\t248956422\t52\t60\t61
chr2\t242193529\t252513167\t60\t61
chrX\t156040895\t823902947\t60\t61
";

    #[test]
    fn test_parse_three_sequences() {
        let m = parse_fai(SAMPLE).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m["chr1"], 248_956_422);
        assert_eq!(m["chr2"], 242_193_529);
        assert_eq!(m["chrX"], 156_040_895);
    }

    #[test]
    fn test_skips_blank_and_comment_lines() {
        let content = "# header\n\nchr1\t100\t0\t60\t61\n";
        let m = parse_fai(content).unwrap();
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn test_error_on_single_column() {
        let result = parse_fai("chr1\n");
        assert!(result.is_err());
    }
}
