//! Parser for BUSCO `full_table.tsv` output files.
//!
//! Handles files produced by BUSCO v4 and v5 with the standard tab-separated
//! header comment format.  Only `Complete` and `Duplicated` entries are
//! included; `Fragmented` and `Missing` entries are skipped.  When a gene
//! appears as `Duplicated` more than once, the instance with the highest
//! score is kept.

use std::collections::HashMap;

use crate::parse_local::feature_set::{LocalFeature, LocalFeatureSet};

/// Errors that can occur when parsing a BUSCO `full_table.tsv`.
#[derive(Debug)]
pub enum ParseError {
    /// A required column was absent from the header line.
    MissingColumn(String),
    /// A data line had too few columns to extract required fields.
    TooFewColumns { line: usize },
    /// A numeric field could not be parsed.
    InvalidNumber {
        line: usize,
        field: String,
        value: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingColumn(col) => write!(f, "BUSCO TSV: missing column '{col}'"),
            Self::TooFewColumns { line } => {
                write!(f, "BUSCO TSV line {line}: too few columns")
            }
            Self::InvalidNumber { line, field, value } => {
                write!(
                    f,
                    "BUSCO TSV line {line}: cannot parse {field} '{value}' as number"
                )
            }
        }
    }
}

/// Parse a BUSCO `full_table.tsv` into a [`LocalFeatureSet`].
///
/// Column order (BUSCO v4/v5 default, 0-indexed):
/// ```text
/// 0: Busco_id   1: Status     2: Sequence    3: Gene Start
/// 4: Gene End   5: Strand     6: Score       7: Length
/// ```
/// The header line (starting with `# Busco id`) is used to locate columns by
/// name when present, falling back to positional defaults when the file has no
/// recognised header.
///
/// `sequence_lengths` is **not** populated by this function; call
/// [`crate::parse_local::fai::parse_fai`] or
/// [`LocalFeatureSet::derive_lengths`] after parsing.
pub fn parse_busco_tsv(assembly_id: &str, content: &str) -> Result<LocalFeatureSet, ParseError> {
    // Column index defaults (BUSCO v4/v5 positional order)
    let mut col_busco_id = 0usize;
    let mut col_status = 1usize;
    let mut col_sequence = 2usize;
    let mut col_start = 3usize;
    let mut col_end = 4usize;
    let mut col_strand = 5usize;
    let mut col_score = 6usize;

    let mut header_found = false;

    // Accumulate best Duplicated entry by score; all Complete entries go straight in.
    // Key = busco_id; value = (LocalFeature, score)
    let mut best: HashMap<String, (LocalFeature, f64)> = HashMap::new();

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line_no = line_idx + 1;

        // Header detection: BUSCO files start with comment lines.
        // The column-header comment begins with "# Busco id" (v4) or "# busco_id" (v5).
        if raw_line.starts_with('#') {
            let trimmed = raw_line.trim_start_matches('#').trim();
            // Normalise: lowercase and replace spaces/hyphens with underscores
            let normalised = trimmed.to_lowercase().replace([' ', '-'], "_");
            if normalised.starts_with("busco_id") || normalised.starts_with("busco id") {
                let headers: Vec<&str> = raw_line
                    .trim_start_matches('#')
                    .split('\t')
                    .map(str::trim)
                    .collect();
                let find = |name: &str| -> Option<usize> {
                    let name_lower = name.to_lowercase().replace([' ', '-'], "_");
                    headers
                        .iter()
                        .position(|h| h.to_lowercase().replace([' ', '-'], "_") == name_lower)
                };
                if let (Some(i), Some(j), Some(k), Some(l), Some(m), Some(n), Some(o)) = (
                    find("busco_id"),
                    find("status"),
                    find("sequence"),
                    find("gene_start"),
                    find("gene_end"),
                    find("strand"),
                    find("score"),
                ) {
                    col_busco_id = i;
                    col_status = j;
                    col_sequence = k;
                    col_start = l;
                    col_end = m;
                    col_strand = n;
                    col_score = o;
                    header_found = true;
                }
            }
            continue; // skip all comment lines
        }

        if raw_line.trim().is_empty() {
            continue;
        }

        let cols: Vec<&str> = raw_line.split('\t').collect();
        let max_col = [
            col_busco_id,
            col_status,
            col_sequence,
            col_start,
            col_end,
            col_score,
        ]
        .iter()
        .copied()
        .max()
        .unwrap_or(6);

        if cols.len() <= max_col {
            if header_found {
                return Err(ParseError::TooFewColumns { line: line_no });
            }
            continue; // tolerate short lines when no header was parsed
        }

        let status = cols[col_status].trim();
        if status != "Complete" && status != "Duplicated" {
            continue;
        }

        let busco_id = cols[col_busco_id].trim().to_string();
        let sequence_id = cols[col_sequence].trim().to_string();

        let start: u64 = cols[col_start]
            .trim()
            .parse()
            .map_err(|_| ParseError::InvalidNumber {
                line: line_no,
                field: "Gene Start".to_string(),
                value: cols[col_start].to_string(),
            })?;

        let end: u64 = cols[col_end]
            .trim()
            .parse()
            .map_err(|_| ParseError::InvalidNumber {
                line: line_no,
                field: "Gene End".to_string(),
                value: cols[col_end].to_string(),
            })?;

        let strand: i8 = match cols.get(col_strand).map(|s| s.trim()) {
            Some("-") => -1,
            _ => 1,
        };

        let score: f64 = if cols.len() > col_score {
            cols[col_score].trim().parse().unwrap_or(0.0)
        } else {
            0.0
        };

        let feat = LocalFeature {
            group: busco_id.clone(),
            sequence_id,
            start,
            end,
            strand,
            cat: Some(status.to_string()),
        };

        match best.entry(busco_id) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert((feat, score));
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if score > e.get().1 {
                    *e.get_mut() = (feat, score);
                }
            }
        }
    }

    let features: Vec<LocalFeature> = best.into_values().map(|(f, _)| f).collect();

    Ok(LocalFeatureSet {
        assembly_id: assembly_id.to_string(),
        features,
        sequence_lengths: HashMap::new(),
        lengths_derived: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Busco id\tStatus\tSequence\tGene Start\tGene End\tStrand\tScore\tLength
EOG001\tComplete\tchr1\t1000\t2000\t+\t500.0\t1000
EOG002\tDuplicated\tchr1\t3000\t4000\t-\t300.0\t1000
EOG002\tDuplicated\tchr2\t5000\t6000\t+\t400.0\t1000
EOG003\tFragmented\tchr1\t7000\t8000\t+\t100.0\t1000
EOG004\tMissing\t-\t0\t0\t+\t0.0\t0
";

    #[test]
    fn test_parse_complete_and_duplicated() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        // Complete + best Duplicated = 2 features
        assert_eq!(fset.features.len(), 2);
        assert_eq!(fset.assembly_id, "asm_A");
    }

    #[test]
    fn test_skips_fragmented_and_missing() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        let groups: Vec<&str> = fset.features.iter().map(|f| f.group.as_str()).collect();
        assert!(!groups.contains(&"EOG003"));
        assert!(!groups.contains(&"EOG004"));
    }

    #[test]
    fn test_duplicated_keeps_highest_score() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        // EOG002: score 300.0 (chr1) vs 400.0 (chr2) — chr2 should be kept
        let dup = fset.features.iter().find(|f| f.group == "EOG002").unwrap();
        assert_eq!(dup.sequence_id, "chr2");
        assert_eq!(dup.strand, 1);
    }

    #[test]
    fn test_strand_parsing() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        let complete = fset.features.iter().find(|f| f.group == "EOG001").unwrap();
        assert_eq!(complete.strand, 1);
    }

    #[test]
    fn test_cat_value_is_status() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        let complete = fset.features.iter().find(|f| f.group == "EOG001").unwrap();
        assert_eq!(complete.cat.as_deref(), Some("Complete"));
    }

    #[test]
    fn test_lengths_not_derived_by_default() {
        let fset = parse_busco_tsv("asm_A", SAMPLE).unwrap();
        assert!(!fset.lengths_derived);
        assert!(fset.sequence_lengths.is_empty());
    }

    #[test]
    fn test_empty_content_returns_empty_set() {
        let fset = parse_busco_tsv("asm_A", "# comment\n").unwrap();
        assert!(fset.features.is_empty());
    }
}
