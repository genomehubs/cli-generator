//! Regional windowing for positional reports.
//!
//! When `window_size` is set, individual feature positions are binned into
//! non-overlapping intervals of the specified width.  This reduces payload
//! size for large assemblies (e.g. 10 000 BUSCO markers → ~400 1 Mbp windows).

use std::collections::HashMap;

/// `(bucket_start → (hit_count, category_counts))`
type BucketMap = HashMap<u64, (usize, HashMap<String, usize>)>;

/// A single windowed interval for one sequence.
#[derive(Debug, Clone)]
pub struct WindowedPoint {
    pub sequence_id: String,
    pub window_start: u64,
    pub window_end: u64,
    /// Number of features in this window.
    pub count: usize,
    /// Category breakdown: category value → count.
    pub cats: std::collections::HashMap<String, usize>,
}

/// A minimal positional point before windowing.
#[derive(Debug, Clone)]
pub struct RawPoint {
    pub sequence_id: String,
    pub start: u64,
    pub cat_value: Option<String>,
}

/// Bin `points` into non-overlapping windows of `window_size` base-pairs.
///
/// Points are grouped by `sequence_id` then distributed into `[0, window_size)`,
/// `[window_size, 2*window_size)`, … windows.  Empty windows are omitted.
///
/// Runs in O(n) time after an O(n log n) sort.
pub fn apply_window(points: &[RawPoint], window_size: u64) -> Vec<WindowedPoint> {
    if window_size == 0 {
        return vec![];
    }

    // Group by sequence_id, then by window bucket
    let mut by_seq: HashMap<&str, BucketMap> = HashMap::new();

    for point in points {
        let bucket = (point.start / window_size) * window_size;
        let seq_entry = by_seq.entry(point.sequence_id.as_str()).or_default();
        let (count, cats) = seq_entry.entry(bucket).or_insert((0, HashMap::new()));
        *count += 1;
        if let Some(cat) = &point.cat_value {
            *cats.entry(cat.clone()).or_insert(0) += 1;
        }
    }

    let mut result: Vec<WindowedPoint> = by_seq
        .into_iter()
        .flat_map(|(seq_id, windows)| {
            windows
                .into_iter()
                .map(move |(bucket, (count, cats))| WindowedPoint {
                    sequence_id: seq_id.to_string(),
                    window_start: bucket,
                    window_end: bucket + window_size,
                    count,
                    cats,
                })
        })
        .collect();

    result.sort_by(|a, b| {
        a.sequence_id
            .cmp(&b.sequence_id)
            .then(a.window_start.cmp(&b.window_start))
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_basic_binning() {
        let points = vec![
            RawPoint {
                sequence_id: "chr1".to_string(),
                start: 0,
                cat_value: None,
            },
            RawPoint {
                sequence_id: "chr1".to_string(),
                start: 500_000,
                cat_value: None,
            },
            RawPoint {
                sequence_id: "chr1".to_string(),
                start: 1_100_000,
                cat_value: None,
            },
        ];
        let windowed = apply_window(&points, 1_000_000);
        assert_eq!(windowed.len(), 2);
        let w0 = windowed.iter().find(|w| w.window_start == 0).unwrap();
        assert_eq!(w0.count, 2); // positions 0 and 500_000 share the first window
        let w1 = windowed
            .iter()
            .find(|w| w.window_start == 1_000_000)
            .unwrap();
        assert_eq!(w1.count, 1);
    }

    #[test]
    fn window_non_overlapping() {
        // All produced windows must be non-overlapping
        let points: Vec<RawPoint> = (0..100)
            .map(|i| RawPoint {
                sequence_id: "chr1".to_string(),
                start: i * 123_456,
                cat_value: None,
            })
            .collect();
        let windowed = apply_window(&points, 500_000);
        // Verify no two windows on the same sequence share position space
        for i in 0..windowed.len().saturating_sub(1) {
            if windowed[i].sequence_id == windowed[i + 1].sequence_id {
                assert!(windowed[i].window_end <= windowed[i + 1].window_start);
            }
        }
    }
}
