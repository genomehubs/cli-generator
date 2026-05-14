//! Server-side region computation for the positional report family.
//!
//! Regions collapse adjacent features sharing the same categorical attribute
//! value into contiguous intervals.  They are computed after all ES filters
//! are applied — no additional ES query is needed.
//!
//! Two boundary placement modes are supported (see [`RegionBounds`]):
//!
//! - `feature_ends` (default) — each region spans exactly from the `start` of
//!   its first feature to the `end` of its last feature.
//! - `midpoints` — boundaries are placed at the midpoint between the last
//!   feature of one run and the first feature of the next, creating a
//!   tile-like partitioning of the sequence space.
//!
//! An optional `max_expansion` cap prevents midpoint boundaries from expanding
//! more than N bp beyond the nearest feature edge.

use std::collections::HashMap;

use genomehubs_query::report::{RegionBounds, RegionsSpec};

use crate::report::positional::feature_query::FeatureRecord;

/// A single computed region interval.
///
/// `x_offset` is not included here; the caller injects it from the sequence
/// layout when serialising to JSON.
#[derive(Debug, Clone)]
pub struct RegionRecord {
    /// Assembly this region belongs to.
    pub assembly_id: String,
    /// Sequence (chromosome / scaffold) this region belongs to.
    pub sequence_id: String,
    /// Start position (bp, inclusive, 0-based).
    pub start: u64,
    /// End position (bp, exclusive).
    pub end: u64,
    /// The resolved category value for this run of features.
    pub cat_value: String,
    /// Number of features that contributed to this region.
    pub feature_count: usize,
}

/// Compute per-assembly, per-sequence region intervals from a feature set.
///
/// Features are grouped by `(assembly_id, sequence_id)`, sorted by `start`,
/// and collapsed into runs of equal `cat_value`.  The `cat_value` for each
/// feature is resolved from [`RegionsSpec::name_to_cat`] when that map is
/// present, otherwise from `feat.cat_value` (falls back to `"other"`).
///
/// `min_features`: regions with fewer features than this threshold are kept
/// as-is in this implementation (the merge pass is reserved for a future
/// phase; the default of 1 means no filtering occurs).
pub fn compute_regions(features: &[FeatureRecord], spec: &RegionsSpec) -> Vec<RegionRecord> {
    let mut by_seq: HashMap<(&str, &str), Vec<&FeatureRecord>> = HashMap::new();
    for f in features {
        by_seq
            .entry((f.assembly_id.as_str(), f.sequence_id.as_str()))
            .or_default()
            .push(f);
    }

    // Sort keys for deterministic output
    let mut keys: Vec<(&str, &str)> = by_seq.keys().copied().collect();
    keys.sort();

    let mut regions = Vec::new();

    for (assembly_id, sequence_id) in keys {
        let mut feats: Vec<&FeatureRecord> = by_seq[&(assembly_id, sequence_id)].clone();
        feats.sort_by_key(|f| f.start);

        regions.extend(regions_for_sequence(feats, assembly_id, sequence_id, spec));
    }

    regions
}

/// Compute region intervals for a single sorted feature list on one sequence.
fn regions_for_sequence(
    feats: Vec<&FeatureRecord>,
    assembly_id: &str,
    sequence_id: &str,
    spec: &RegionsSpec,
) -> Vec<RegionRecord> {
    let mut regions: Vec<RegionRecord> = Vec::new();

    // Accumulator for the active run
    let mut active_start: u64 = 0;
    let mut active_cat: String = String::new();
    let mut active_count: usize = 0;
    let mut last_end: u64 = 0;
    let mut in_run = false;

    for feat in feats.iter() {
        let cat = resolve_cat(feat, spec);

        if !in_run {
            active_start = feat.start;
            active_cat = cat;
            active_count = 1;
            last_end = feat.end;
            in_run = true;
            continue;
        }

        if cat == active_cat {
            last_end = last_end.max(feat.end);
            active_count += 1;
        } else {
            // Boundary between active run and new cat
            let next_start = feat.start;
            let boundary_end = boundary_end(last_end, next_start, spec);

            regions.push(RegionRecord {
                assembly_id: assembly_id.to_string(),
                sequence_id: sequence_id.to_string(),
                start: active_start,
                end: boundary_end,
                cat_value: std::mem::take(&mut active_cat),
                feature_count: active_count,
            });

            active_start = boundary_start(boundary_end, next_start, spec);
            active_cat = cat;
            active_count = 1;
            last_end = feat.end;
        }
    }

    // Flush the final run
    if in_run {
        regions.push(RegionRecord {
            assembly_id: assembly_id.to_string(),
            sequence_id: sequence_id.to_string(),
            start: active_start,
            end: last_end,
            cat_value: active_cat,
            feature_count: active_count,
        });
    }

    regions
}

/// Resolve the category value for a feature.
///
/// Uses `name_to_cat` when present (keyed on `group_value` / feature name),
/// otherwise falls back to `feat.cat_value`.  Returns `"other"` when neither
/// source has a value.
fn resolve_cat(feat: &FeatureRecord, spec: &RegionsSpec) -> String {
    if let Some(map) = &spec.name_to_cat {
        return map
            .get(&feat.group_value)
            .cloned()
            .unwrap_or_else(|| "other".to_string());
    }
    feat.cat_value
        .clone()
        .unwrap_or_else(|| "other".to_string())
}

/// Compute the end boundary of an active region given the mode.
///
/// - `feature_ends`: the region ends exactly at `last_end`.
/// - `midpoints`: the region ends at the midpoint between `last_end` and
///   `next_start`, capped by `max_expansion` when set.
fn boundary_end(last_end: u64, next_start: u64, spec: &RegionsSpec) -> u64 {
    match spec.bounds {
        RegionBounds::FeatureEnds => last_end,
        RegionBounds::Midpoints => {
            let midpoint = (last_end + next_start) / 2;
            if let Some(max_exp) = spec.max_expansion {
                midpoint.min(last_end.saturating_add(max_exp))
            } else {
                midpoint
            }
        }
    }
}

/// Compute the start of the next region given the mode.
///
/// In `feature_ends` mode the next region simply starts at the first
/// feature's start.  In `midpoints` mode it starts at the same boundary
/// computed for the end of the previous region (tile-like partitioning).
fn boundary_start(prev_boundary_end: u64, next_feat_start: u64, spec: &RegionsSpec) -> u64 {
    match spec.bounds {
        RegionBounds::FeatureEnds => next_feat_start,
        RegionBounds::Midpoints => {
            if let Some(max_exp) = spec.max_expansion {
                // The start is capped symmetrically: cannot be more than max_exp before
                // the next feature's start.
                prev_boundary_end.max(next_feat_start.saturating_sub(max_exp))
            } else {
                prev_boundary_end
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use genomehubs_query::report::RegionBounds;

    fn make_feat(
        assembly_id: &str,
        sequence_id: &str,
        start: u64,
        end: u64,
        group_value: &str,
        cat_value: Option<&str>,
    ) -> FeatureRecord {
        FeatureRecord {
            assembly_id: assembly_id.to_string(),
            feature_id: format!("{assembly_id}:{sequence_id}:{start}"),
            sequence_id: sequence_id.to_string(),
            start,
            end,
            strand: 1,
            group_value: group_value.to_string(),
            cat_value: cat_value.map(|s| s.to_string()),
        }
    }

    fn simple_spec(bounds: RegionBounds) -> RegionsSpec {
        RegionsSpec {
            cat: Some("merian_unit".to_string()),
            name_to_cat: None,
            bounds,
            min_features: 1,
            max_expansion: None,
        }
    }

    // ── feature_ends mode ─────────────────────────────────────────────────────

    #[test]
    fn test_feature_ends_two_cats() {
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", Some("MZ-1")),
            make_feat("GCA_001", "chr1", 2000, 3000, "OG2", Some("MZ-1")),
            make_feat("GCA_001", "chr1", 4000, 5000, "OG3", Some("MZ-2")),
            make_feat("GCA_001", "chr1", 6000, 7000, "OG4", Some("MZ-2")),
        ];
        let spec = simple_spec(RegionBounds::FeatureEnds);
        let regions = compute_regions(&features, &spec);

        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].cat_value, "MZ-1");
        assert_eq!(regions[0].start, 0);
        assert_eq!(regions[0].end, 3000);
        assert_eq!(regions[0].feature_count, 2);

        assert_eq!(regions[1].cat_value, "MZ-2");
        assert_eq!(regions[1].start, 4000);
        assert_eq!(regions[1].end, 7000);
        assert_eq!(regions[1].feature_count, 2);
    }

    #[test]
    fn test_feature_ends_single_feature() {
        let features = vec![make_feat("GCA_001", "chr1", 100, 500, "OG1", Some("MZ-1"))];
        let spec = simple_spec(RegionBounds::FeatureEnds);
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start, 100);
        assert_eq!(regions[0].end, 500);
        assert_eq!(regions[0].feature_count, 1);
    }

    #[test]
    fn test_feature_ends_all_same_cat() {
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", Some("MZ-1")),
            make_feat("GCA_001", "chr1", 2000, 3000, "OG2", Some("MZ-1")),
            make_feat("GCA_001", "chr1", 5000, 6000, "OG3", Some("MZ-1")),
        ];
        let spec = simple_spec(RegionBounds::FeatureEnds);
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].end, 6000);
        assert_eq!(regions[0].feature_count, 3);
    }

    #[test]
    fn test_feature_ends_no_cat_uses_other() {
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", None),
            make_feat("GCA_001", "chr1", 2000, 3000, "OG2", Some("MZ-1")),
        ];
        let spec = simple_spec(RegionBounds::FeatureEnds);
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].cat_value, "other");
        assert_eq!(regions[1].cat_value, "MZ-1");
    }

    // ── midpoints mode ────────────────────────────────────────────────────────

    #[test]
    fn test_midpoints_boundary_placement() {
        // last_end=3000, next_start=5000 → midpoint=4000
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 3000, "OG1", Some("A")),
            make_feat("GCA_001", "chr1", 5000, 7000, "OG2", Some("B")),
        ];
        let spec = simple_spec(RegionBounds::Midpoints);
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].end, 4000); // midpoint of (3000, 5000)
        assert_eq!(regions[1].start, 4000);
        assert_eq!(regions[1].end, 7000);
    }

    #[test]
    fn test_midpoints_max_expansion_caps_boundary() {
        // last_end=1000, next_start=9000 → midpoint=5000
        // max_expansion=500 → end capped at 1000+500=1500
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", Some("A")),
            make_feat("GCA_001", "chr1", 9000, 10000, "OG2", Some("B")),
        ];
        let spec = RegionsSpec {
            cat: Some("merian_unit".to_string()),
            name_to_cat: None,
            bounds: RegionBounds::Midpoints,
            min_features: 1,
            max_expansion: Some(500),
        };
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].end, 1500); // capped at last_end + max_expansion
    }

    // ── name_to_cat mapping ───────────────────────────────────────────────────

    #[test]
    fn test_name_to_cat_overrides_cat_value() {
        let mut name_to_cat = HashMap::new();
        name_to_cat.insert("OG1".to_string(), "Clade-A".to_string());
        name_to_cat.insert("OG2".to_string(), "Clade-B".to_string());

        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", Some("ignored")),
            make_feat("GCA_001", "chr1", 2000, 3000, "OG2", Some("also-ignored")),
        ];
        let spec = RegionsSpec {
            cat: None,
            name_to_cat: Some(name_to_cat),
            bounds: RegionBounds::FeatureEnds,
            min_features: 1,
            max_expansion: None,
        };
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].cat_value, "Clade-A");
        assert_eq!(regions[1].cat_value, "Clade-B");
    }

    #[test]
    fn test_name_to_cat_missing_name_uses_other() {
        let name_to_cat: HashMap<String, String> = HashMap::new(); // empty
        let features = vec![make_feat("GCA_001", "chr1", 0, 1000, "OG1", None)];
        let spec = RegionsSpec {
            cat: None,
            name_to_cat: Some(name_to_cat),
            bounds: RegionBounds::FeatureEnds,
            min_features: 1,
            max_expansion: None,
        };
        let regions = compute_regions(&features, &spec);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].cat_value, "other");
    }

    // ── multi-assembly ────────────────────────────────────────────────────────

    #[test]
    fn test_multi_assembly_regions_independent() {
        let features = vec![
            make_feat("GCA_001", "chr1", 0, 1000, "OG1", Some("A")),
            make_feat("GCA_001", "chr1", 2000, 3000, "OG2", Some("B")),
            make_feat("GCA_002", "chr1", 0, 1500, "OG1", Some("A")),
            make_feat("GCA_002", "chr1", 2500, 4000, "OG2", Some("A")),
        ];
        let spec = simple_spec(RegionBounds::FeatureEnds);
        let regions = compute_regions(&features, &spec);
        // GCA_001: 2 regions; GCA_002: 1 region (both A)
        let n_001 = regions
            .iter()
            .filter(|r| r.assembly_id == "GCA_001")
            .count();
        let n_002 = regions
            .iter()
            .filter(|r| r.assembly_id == "GCA_002")
            .count();
        assert_eq!(n_001, 2);
        assert_eq!(n_002, 1);
    }

    // ── empty input ───────────────────────────────────────────────────────────

    #[test]
    fn test_empty_features_returns_empty() {
        let spec = simple_spec(RegionBounds::FeatureEnds);
        assert!(compute_regions(&[], &spec).is_empty());
    }
}
