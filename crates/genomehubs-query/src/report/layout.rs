//! Sequence layout algorithms for positional reports.
//!
//! Implements the three-step layout pipeline extracted from the v2 `oxford.js`:
//!
//! 1. [`order_sequences_by_median`] — sort comparison sequences by the median
//!    position of their shared markers in the reference assembly.
//! 2. [`orient_sequence`] — decide whether to flip a sequence (negative strand)
//!    using a linear regression on (ref_pos, cmp_pos) pairs.
//! 3. [`compute_offsets`] — accumulate per-sequence lengths into a global
//!    offset table used to convert local positions to genome-wide coordinates.

use std::collections::HashMap;

/// A sequence entry with length and computed layout metadata.
#[derive(Debug, Clone)]
pub struct SequenceLayout {
    pub sequence_id: String,
    pub length: u64,
    /// Genome-wide offset at which this sequence starts.
    pub offset: u64,
    /// +1 if the sequence is shown in its natural orientation, -1 if flipped.
    pub orientation: i8,
}

/// Compute genome-wide offsets for an ordered list of sequences.
///
/// Sequences are placed end-to-end in order.  When `orientation == -1` the
/// sequence is displayed in reverse; the offset still marks the *start* of
/// the visual region in genome-wide coordinates (i.e. the offset is the
/// *higher* absolute position when the sequence is flipped).
///
/// Returns the total genome span as a convenience.
pub fn compute_offsets(sequences: &mut [SequenceLayout]) -> u64 {
    let mut cursor: u64 = 0;
    for seq in sequences.iter_mut() {
        seq.offset = cursor;
        cursor += seq.length;
    }
    cursor
}

/// Sort comparison sequences by the median reference position of their shared
/// markers.
///
/// `ref_positions` maps `(assembly_id, sequence_id)` to a sorted list of
/// genome-wide positions in the reference.  `cmp_sequences` is the list of
/// sequences in the comparison assembly to order.
///
/// The *score* for each comparison sequence is the median of the reference
/// positions of all features that appear in both the comparison sequence and
/// the reference assembly.
pub fn order_sequences_by_median(
    cmp_sequences: &[SequenceLayout],
    ref_cumulative: &HashMap<String, u64>,
    group_to_ref_pos: &HashMap<String, u64>,
    seq_to_groups: &HashMap<String, Vec<String>>,
) -> Vec<SequenceLayout> {
    let mut scored: Vec<(f64, SequenceLayout)> = cmp_sequences
        .iter()
        .map(|seq| {
            let groups = seq_to_groups.get(&seq.sequence_id);
            let score = median_ref_score(groups, group_to_ref_pos, ref_cumulative);
            (score, seq.clone())
        })
        .collect();

    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, s)| s).collect()
}

/// Return the median reference position score for a set of group IDs.
fn median_ref_score(
    groups: Option<&Vec<String>>,
    group_to_ref_pos: &HashMap<String, u64>,
    _ref_cumulative: &HashMap<String, u64>,
) -> f64 {
    let groups = match groups {
        Some(g) if !g.is_empty() => g,
        _ => return f64::MAX,
    };

    let mut positions: Vec<f64> = groups
        .iter()
        .filter_map(|g| group_to_ref_pos.get(g))
        .map(|&p| p as f64)
        .collect();

    if positions.is_empty() {
        return f64::MAX;
    }

    positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = positions.len() / 2;
    if positions.len().is_multiple_of(2) {
        (positions[mid - 1] + positions[mid]) / 2.0
    } else {
        positions[mid]
    }
}

/// Determine the orientation of a comparison sequence relative to the reference.
///
/// Fits a linear regression through the scatter of `(ref_pos, cmp_pos)` pairs.
/// Returns `+1` if the slope is non-negative, `-1` if negative.
/// Falls back to `+1` when there are fewer than 2 points (no regression possible).
pub fn orient_sequence(pairs: &[(f64, f64)]) -> i8 {
    if pairs.len() < 2 {
        return 1;
    }

    let n = pairs.len() as f64;
    let sum_x: f64 = pairs.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = pairs.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = pairs.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = pairs.iter().map(|(x, _)| x * x).sum();

    let denominator = n * sum_xx - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return 1; // degenerate (all ref positions identical)
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denominator;
    if slope >= 0.0 {
        1
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orient_positive_slope() {
        let pairs = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)];
        assert_eq!(orient_sequence(&pairs), 1);
    }

    #[test]
    fn orient_negative_slope() {
        let pairs = vec![(0.0, 3.0), (1.0, 2.0), (2.0, 1.0), (3.0, 0.0)];
        assert_eq!(orient_sequence(&pairs), -1);
    }

    #[test]
    fn orient_single_point_fallback() {
        assert_eq!(orient_sequence(&[(5.0, 10.0)]), 1);
    }

    #[test]
    fn orient_empty_fallback() {
        assert_eq!(orient_sequence(&[]), 1);
    }

    #[test]
    fn compute_offsets_basic() {
        let mut seqs = vec![
            SequenceLayout {
                sequence_id: "chr1".to_string(),
                length: 1000,
                offset: 0,
                orientation: 1,
            },
            SequenceLayout {
                sequence_id: "chr2".to_string(),
                length: 500,
                offset: 0,
                orientation: 1,
            },
        ];
        let total = compute_offsets(&mut seqs);
        assert_eq!(seqs[0].offset, 0);
        assert_eq!(seqs[1].offset, 1000);
        assert_eq!(total, 1500);
    }
}
