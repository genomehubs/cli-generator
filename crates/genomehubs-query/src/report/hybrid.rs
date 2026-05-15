//! Hybrid positional report computation for local and mixed local+remote assemblies.
//!
//! Two public entry points:
//! - [`positional_from_features`] — all-local Oxford / ribbon / painting (no API call required)
//! - [`hybrid_positional`] — combines a rendered remote positional report with one or more
//!   local [`LocalFeatureSet`] instances

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    parse_local::feature_set::{LocalFeature, LocalFeatureSet},
    report::layout::{compute_offsets, order_sequences_by_median, orient_sequence, SequenceLayout},
};

// ── Region computation types ──────────────────────────────────────────────────

/// Controls how region boundaries are placed between adjacent features of
/// different categories.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegionBounds {
    /// Boundaries are placed exactly at feature ends / starts (no expansion).
    #[default]
    FeatureEnds,
    /// Boundaries are placed at the midpoint between adjacent feature ends and
    /// starts; this creates gapless contiguous regions across each sequence.
    Midpoints,
}

/// Configuration for region computation passed to [`positional_from_features`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegionsSpec {
    /// How to compute region boundaries (default: `feature_ends`).
    #[serde(default)]
    pub bounds: RegionBounds,
    /// Minimum number of consecutive same-category features required to *start*
    /// a region.  Runs shorter than this are treated as interruptions within an
    /// adjacent region (when within `tolerance`) or discarded.  Default: 1.
    #[serde(default = "default_min_run", alias = "min_features")]
    pub min_run: usize,
    /// Maximum number of consecutive non-matching (different assigned-category)
    /// features that can be absorbed into a region as labelled interruptions
    /// before the region is considered to have ended.  Unassigned features
    /// (`cat = null`) are always skipped and do not count against the tolerance.
    /// Default: 0 (any non-matching feature breaks the region).
    #[serde(default)]
    pub tolerance: usize,
    /// Optional cap on how far a midpoint boundary can extend beyond the
    /// nearest feature edge (base-pairs).
    pub max_expansion: Option<u64>,
}

fn default_min_run() -> usize {
    1
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors returned by the hybrid layout computation.
#[derive(Debug)]
pub enum LayoutError {
    /// No feature sets were provided.
    NoAssemblies,
    /// Wrong number of assemblies for the requested report type.
    IncompatibleAssemblyCounts {
        report_type: &'static str,
        required: &'static str,
        got: usize,
    },
    /// The remote report JSON could not be parsed.
    RemoteReportParse(String),
    /// No shared group values exist between the reference and any comparison assembly.
    NoSharedGroups,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAssemblies => write!(f, "no feature sets provided"),
            Self::IncompatibleAssemblyCounts {
                report_type,
                required,
                got,
            } => {
                write!(
                    f,
                    "{report_type} report requires {required} assemblies, got {got}"
                )
            }
            Self::RemoteReportParse(msg) => write!(f, "failed to parse remote report: {msg}"),
            Self::NoSharedGroups => {
                write!(
                    f,
                    "no shared group values between reference and comparison assemblies"
                )
            }
        }
    }
}

// ── Internal types ────────────────────────────────────────────────────────────

/// Per-assembly computed layout — mirrors the server-side `AssemblyLayout`.
struct AssemblyLayout {
    sequences: Vec<SequenceLayout>,
    total_span: u64,
    lengths_derived: bool,
}

// ── Layout helpers ────────────────────────────────────────────────────────────

/// Build per-assembly layouts from an ordered list of [`LocalFeatureSet`] slices.
///
/// Mutates each set that lacks `sequence_lengths` by calling
/// [`LocalFeatureSet::derive_lengths`], setting `lengths_derived = true`.
fn build_local_layouts(
    assembly_ids: &[&str],
    feature_sets: &mut [LocalFeatureSet],
    reorient: bool,
) -> HashMap<String, AssemblyLayout> {
    // Ensure lengths are populated
    for set in feature_sets.iter_mut() {
        if set.sequence_lengths.is_empty() {
            set.derive_lengths();
        }
    }

    // Index feature sets by assembly_id
    let set_by_id: HashMap<&str, &LocalFeatureSet> = feature_sets
        .iter()
        .map(|s| (s.assembly_id.as_str(), s))
        .collect();

    let ref_id = match assembly_ids.first() {
        Some(id) => *id,
        None => return HashMap::new(),
    };

    // Build reference layout (sequences sorted by length desc)
    let ref_layout = {
        let ref_set = match set_by_id.get(ref_id) {
            Some(s) => s,
            None => return HashMap::new(),
        };
        let mut seqs: Vec<SequenceLayout> = ref_set
            .sequence_lengths
            .iter()
            .map(|(id, &len)| SequenceLayout {
                sequence_id: id.clone(),
                length: len,
                offset: 0,
                orientation: 1,
            })
            .collect();
        seqs.sort_by(|a, b| b.length.cmp(&a.length));
        compute_offsets(&mut seqs);
        let span = seqs.last().map(|s| s.offset + s.length).unwrap_or(0);
        AssemblyLayout {
            sequences: seqs,
            total_span: span,
            lengths_derived: feature_sets
                .iter()
                .find(|s| s.assembly_id == ref_id)
                .map(|s| s.lengths_derived)
                .unwrap_or(false),
        }
    };

    // Build group → genome-wide reference position (offset + midpoint of feature)
    let ref_offset_map: HashMap<String, u64> = ref_layout
        .sequences
        .iter()
        .map(|s| (s.sequence_id.clone(), s.offset))
        .collect();

    let group_to_ref_pos: HashMap<String, u64> = match set_by_id.get(ref_id) {
        Some(ref_set) => ref_set
            .features
            .iter()
            .filter_map(|f| {
                ref_offset_map
                    .get(&f.sequence_id)
                    .map(|&off| (f.group.clone(), off + (f.start + f.end) / 2))
            })
            .collect(),
        None => HashMap::new(),
    };

    let mut result: HashMap<String, AssemblyLayout> = HashMap::new();
    result.insert(ref_id.to_string(), ref_layout);

    // Build layouts for comparison assemblies
    for &cmp_id in &assembly_ids[1..] {
        let cmp_set = match set_by_id.get(cmp_id) {
            Some(s) => s,
            None => continue,
        };

        let cmp_seqs_init: Vec<SequenceLayout> = cmp_set
            .sequence_lengths
            .iter()
            .map(|(id, &len)| SequenceLayout {
                sequence_id: id.clone(),
                length: len,
                offset: 0,
                orientation: 1,
            })
            .collect();

        let seq_to_groups: HashMap<String, Vec<String>> = {
            let mut m: HashMap<String, Vec<String>> = HashMap::new();
            for f in &cmp_set.features {
                m.entry(f.sequence_id.clone())
                    .or_default()
                    .push(f.group.clone());
            }
            m
        };

        let mut sorted = order_sequences_by_median(
            &cmp_seqs_init,
            &ref_offset_map,
            &group_to_ref_pos,
            &seq_to_groups,
        );

        if reorient {
            for seq in &mut sorted {
                let pairs: Vec<(f64, f64)> = cmp_set
                    .features
                    .iter()
                    .filter(|f| f.sequence_id == seq.sequence_id)
                    .filter_map(|f| {
                        group_to_ref_pos
                            .get(&f.group)
                            .map(|&ref_pos| (ref_pos as f64, f.start as f64))
                    })
                    .collect();
                seq.orientation = orient_sequence(&pairs);
            }
        }

        compute_offsets(&mut sorted);
        let span = sorted.last().map(|s| s.offset + s.length).unwrap_or(0);
        result.insert(
            cmp_id.to_string(),
            AssemblyLayout {
                sequences: sorted,
                total_span: span,
                lengths_derived: feature_sets
                    .iter()
                    .find(|s| s.assembly_id == cmp_id)
                    .map(|s| s.lengths_derived)
                    .unwrap_or(false),
            },
        );
    }

    result
}

/// Build a `sequenceId → offset` map from an [`AssemblyLayout`].
fn offset_map(layout: Option<&AssemblyLayout>) -> HashMap<String, u64> {
    layout
        .map(|l| {
            l.sequences
                .iter()
                .map(|s| (s.sequence_id.clone(), s.offset))
                .collect()
        })
        .unwrap_or_default()
}

/// Compute the genome-wide Y position, accounting for orientation flipping.
fn genome_wide_y(
    feat: &LocalFeature,
    cmp_layout: Option<&AssemblyLayout>,
    cmp_offsets: &HashMap<String, u64>,
    y_orient: i8,
) -> (u64, u64) {
    if let Some(&off) = cmp_offsets.get(&feat.sequence_id) {
        let seq_len = cmp_layout
            .and_then(|l| {
                l.sequences
                    .iter()
                    .find(|s| s.sequence_id == feat.sequence_id)
            })
            .map(|s| s.length)
            .unwrap_or(0);
        if y_orient == -1 {
            let flipped_start = seq_len.saturating_sub(feat.end);
            let flipped_end = seq_len.saturating_sub(feat.start);
            (off + flipped_start, off + flipped_end)
        } else {
            (off + feat.start, off + feat.end)
        }
    } else {
        (feat.start, feat.end)
    }
}

/// Serialise assembly metadata (sequences + domain) as a JSON object keyed by assembly ID.
fn serialise_assembly_metadata(
    assembly_ids: &[&str],
    layouts: &HashMap<String, AssemblyLayout>,
) -> Value {
    let mut map = serde_json::Map::new();
    for &id in assembly_ids {
        let layout = match layouts.get(id) {
            Some(l) => l,
            None => continue,
        };
        let sequences: Vec<Value> = layout
            .sequences
            .iter()
            .map(|s| {
                json!({
                    "id": s.sequence_id,
                    "length": s.length,
                    "offset": s.offset,
                    "orientation": s.orientation
                })
            })
            .collect();
        let buckets: Vec<u64> = layout.sequences.iter().map(|s| s.offset).collect();
        map.insert(
            id.to_string(),
            json!({
                "sequences":     sequences,
                "domain":        [0, layout.total_span],
                "buckets":       buckets,
                "lengthsDerived": layout.lengths_derived
            }),
        );
    }
    Value::Object(map)
}

// ── Window helpers (inline — no API crate dependency) ────────────────────────

/// A windowed interval for painting segments.
struct WindowedPaint {
    sequence_id: String,
    window_start: u64,
    window_end: u64,
    count: usize,
    cats: HashMap<String, usize>,
}

/// Bin features into non-overlapping windows of `window_size` bp.
fn apply_window(features: &[&LocalFeature], window_size: u64) -> Vec<WindowedPaint> {
    if window_size == 0 {
        return vec![];
    }

    type BucketMap = HashMap<u64, (usize, HashMap<String, usize>)>;
    let mut by_seq: HashMap<&str, BucketMap> = HashMap::new();
    for f in features {
        let bucket = (f.start / window_size) * window_size;
        let seq_entry = by_seq.entry(f.sequence_id.as_str()).or_default();
        let (count, cats) = seq_entry.entry(bucket).or_insert((0, HashMap::new()));
        *count += 1;
        if let Some(cat) = &f.cat {
            *cats.entry(cat.clone()).or_insert(0) += 1;
        }
    }

    let mut result: Vec<WindowedPaint> = by_seq
        .into_iter()
        .flat_map(|(seq_id, windows)| {
            windows
                .into_iter()
                .map(move |(bucket, (count, cats))| WindowedPaint {
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

// ── Oxford / Ribbon report builder ────────────────────────────────────────────

fn build_oxford_ribbon(
    report_type: &str,
    assembly_ids: &[&str],
    layouts: &HashMap<String, AssemblyLayout>,
    feature_sets: &[LocalFeatureSet],
    cat_field: Option<&str>,
    max_connections_per_group: usize,
) -> Value {
    let ref_id = assembly_ids[0];
    let ref_set = match feature_sets.iter().find(|s| s.assembly_id == ref_id) {
        Some(s) => s,
        None => return json!({"type": report_type, "assemblies": {}, "points": []}),
    };
    let ref_layout = layouts.get(ref_id);
    let ref_offsets = offset_map(ref_layout);

    // Group all ref features by group
    let mut group_to_ref_all: HashMap<&str, Vec<&LocalFeature>> = HashMap::new();
    for f in &ref_set.features {
        group_to_ref_all.entry(&f.group).or_default().push(f);
    }

    let assemblies_json = serialise_assembly_metadata(assembly_ids, layouts);

    let mut all_points: Vec<Value> = Vec::new();
    let mut all_connections: Vec<Value> = Vec::new();
    let mut cat_counts: HashMap<String, u64> = HashMap::new();

    for &cmp_id in &assembly_ids[1..] {
        let cmp_set = match feature_sets.iter().find(|s| s.assembly_id == cmp_id) {
            Some(s) => s,
            None => continue,
        };
        let cmp_layout = layouts.get(cmp_id);
        let cmp_offsets = offset_map(cmp_layout);
        let cmp_orientations: HashMap<String, i8> = cmp_layout
            .map(|l| {
                l.sequences
                    .iter()
                    .map(|s| (s.sequence_id.clone(), s.orientation))
                    .collect()
            })
            .unwrap_or_default();

        let mut cmp_by_group: HashMap<&str, Vec<&LocalFeature>> = HashMap::new();
        for f in &cmp_set.features {
            cmp_by_group.entry(&f.group).or_default().push(f);
        }

        for (group, cmp_feats) in &cmp_by_group {
            let ref_feats = match group_to_ref_all.get(group) {
                Some(v) => v,
                None => continue,
            };

            let cat = cmp_feats
                .first()
                .and_then(|f| f.cat.as_deref())
                .unwrap_or("");
            if !cat.is_empty() {
                *cat_counts.entry(cat.to_string()).or_insert(0) += 1;
            }

            let is_mn = ref_feats.len() > 1 || cmp_feats.len() > 1;

            if !is_mn {
                let rf = ref_feats[0];
                let cf = cmp_feats[0];

                let x = ref_offsets
                    .get(&rf.sequence_id)
                    .map(|&off| off + rf.start)
                    .unwrap_or(rf.start);
                let x2 = ref_offsets
                    .get(&rf.sequence_id)
                    .map(|&off| off + rf.end)
                    .unwrap_or(rf.end);

                let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);

                let mut point = json!({
                    "x": x, "x2": x2,
                    "y": y, "y2": y2,
                    "group": group,
                    "strand": rf.strand,
                    "yStrand": y_orient * cf.strand
                });
                if !cat.is_empty() {
                    point["cat"] = json!(cat);
                }
                if assembly_ids.len() > 2 {
                    point["assemblyPair"] = json!([ref_id, cmp_id]);
                }
                all_points.push(point);
            } else {
                let mut x_coords: Vec<u64> = Vec::new();
                let mut x2_coords: Vec<u64> = Vec::new();
                let mut x_seq_ids: Vec<&str> = Vec::new();
                let mut x_strands: Vec<i8> = Vec::new();
                let mut y_coords: Vec<u64> = Vec::new();
                let mut y2_coords: Vec<u64> = Vec::new();
                let mut y_seq_ids: Vec<&str> = Vec::new();
                let mut y_strands: Vec<i8> = Vec::new();

                for rf in ref_feats.iter() {
                    let x = ref_offsets
                        .get(&rf.sequence_id)
                        .map(|&off| off + rf.start)
                        .unwrap_or(rf.start);
                    let x2 = ref_offsets
                        .get(&rf.sequence_id)
                        .map(|&off| off + rf.end)
                        .unwrap_or(rf.end);
                    x_coords.push(x);
                    x2_coords.push(x2);
                    x_seq_ids.push(&rf.sequence_id);
                    x_strands.push(rf.strand);
                }
                for cf in cmp_feats.iter() {
                    let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                    let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);
                    y_coords.push(y);
                    y2_coords.push(y2);
                    y_seq_ids.push(&cf.sequence_id);
                    y_strands.push(y_orient * cf.strand);
                }

                let total = x_coords.len() * y_coords.len();
                let truncated = total > max_connections_per_group;

                let mut conn = json!({
                    "group":     group,
                    "xCoords":   x_coords,
                    "x2Coords":  x2_coords,
                    "xSeqIds":   x_seq_ids,
                    "xStrands":  x_strands,
                    "yCoords":   y_coords,
                    "y2Coords":  y2_coords,
                    "ySeqIds":   y_seq_ids,
                    "yStrands":  y_strands,
                    "truncated": truncated
                });
                if !cat.is_empty() {
                    conn["catValue"] = json!(cat);
                }
                if assembly_ids.len() > 2 {
                    conn["assemblyPair"] = json!([ref_id, cmp_id]);
                }
                all_connections.push(conn);
            }
        }
    }

    let mut cats_vec: Vec<_> = cat_counts.into_iter().collect();
    cats_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let cats_json: Vec<Value> = cats_vec
        .iter()
        .map(|(k, _)| json!({"key": k, "label": k}))
        .collect();

    json!({
        "type": report_type,
        "assemblies": assemblies_json,
        "points": all_points,
        "connections": all_connections,
        "cats": cats_json,
        "cat": cat_field
    })
}

// ── Painting report builder ───────────────────────────────────────────────────

fn build_painting(
    assembly_id: &str,
    layouts: &HashMap<String, AssemblyLayout>,
    features: &[&LocalFeature],
    cat_field: Option<&str>,
    window_size: Option<u64>,
) -> Value {
    let assemblies_json = serialise_assembly_metadata(&[assembly_id], layouts);

    let segments: Vec<Value> = if let Some(ws) = window_size {
        let windowed = apply_window(features, ws);
        windowed
            .iter()
            .flat_map(|w| {
                if w.cats.is_empty() {
                    vec![json!({
                        "sequence_id": w.sequence_id,
                        "start": w.window_start,
                        "end": w.window_end,
                        "count": w.count
                    })]
                } else {
                    w.cats
                        .iter()
                        .map(|(cat_val, count)| {
                            json!({
                                "sequence_id": w.sequence_id,
                                "start": w.window_start,
                                "end": w.window_end,
                                "cat": cat_val,
                                "count": count
                            })
                        })
                        .collect::<Vec<_>>()
                }
            })
            .collect()
    } else {
        features
            .iter()
            .map(|f| {
                // Include end so the client can display feature-width segments.
                let mut seg = json!({
                    "sequence_id": f.sequence_id,
                    "start": f.start,
                    "end": f.end
                });
                if let Some(cat) = &f.cat {
                    seg["cat"] = json!(cat);
                }
                seg
            })
            .collect()
    };

    json!({
        "type": "painting",
        "assemblies": assemblies_json,
        "segments": segments,
        "cat": cat_field
    })
}

/// Compute regions for one assembly by grouping adjacent same-category features.
///
/// Unassigned features (`cat == None`) are skipped — they are transparent and
/// do not break region continuity.  Features with an assigned category that
/// differ from the current region are buffered; if the region's own category
/// resumes before the buffer exceeds `spec.tolerance`, those features are
/// absorbed as labelled interruptions.  Once the buffer size exceeds the
/// tolerance the current region closes and a new one may begin.
///
/// Regions with fewer than `spec.min_run` primary-category features are never
/// started, and therefore act as interruptions (if within tolerance) or
/// discard separators between adjacent regions.
fn compute_regions(
    assembly_id: &str,
    features: &[&LocalFeature],
    layout: &AssemblyLayout,
    spec: &RegionsSpec,
) -> Vec<Value> {
    // A contiguous run of same-category features.
    struct Run {
        cat: String,
        start: u64,
        end: u64,
        count: usize,
    }

    // A region being assembled by the state machine.
    struct Region {
        cat: String,
        start: u64,
        end: u64,
        feature_count: usize,
        interruptions: Vec<Run>,
    }

    let offsets = offset_map(Some(layout));

    // Group *assigned* features by sequence, sorted by start.
    let mut by_seq: HashMap<&str, Vec<&LocalFeature>> = HashMap::new();
    for f in features {
        if f.cat.is_none() {
            continue; // unassigned — transparent for region computation
        }
        by_seq.entry(f.sequence_id.as_str()).or_default().push(f);
    }
    for feats in by_seq.values_mut() {
        feats.sort_unstable_by_key(|f| f.start);
    }

    let mut all_regions: Vec<Value> = Vec::new();

    for (seq_id, seq_feats) in &by_seq {
        let x_off = offsets.get(*seq_id).copied().unwrap_or(0);

        // Compress consecutive same-cat features into raw runs.
        let mut raw_runs: Vec<Run> = Vec::new();
        for f in seq_feats {
            let cat = f.cat.as_deref().unwrap(); // safe: filtered above
            match raw_runs.last_mut() {
                Some(last) if last.cat == cat => {
                    last.end = last.end.max(f.end);
                    last.count += 1;
                }
                _ => raw_runs.push(Run {
                    cat: cat.to_string(),
                    start: f.start,
                    end: f.end,
                    count: 1,
                }),
            }
        }

        // State machine: current region being built + tolerance pending buffer.
        let mut current: Option<Region> = None;
        let mut pending: Vec<Run> = Vec::new();
        let mut pending_total: usize = 0;
        let mut seq_regions: Vec<Region> = Vec::new();

        for run in raw_runs {
            match current.take() {
                None => {
                    // No active region — discard pending, start fresh if run is large enough.
                    pending.clear();
                    pending_total = 0;
                    if run.count >= spec.min_run {
                        current = Some(Region {
                            cat: run.cat,
                            start: run.start,
                            end: run.end,
                            feature_count: run.count,
                            interruptions: Vec::new(),
                        });
                    }
                }
                Some(mut region) => {
                    if run.cat == region.cat {
                        // Same category: absorb pending buffer as interruptions, extend region.
                        region.interruptions.append(&mut pending);
                        pending_total = 0;
                        region.end = region.end.max(run.end);
                        region.feature_count += run.count;
                        current = Some(region);
                    } else if pending_total + run.count <= spec.tolerance {
                        // Within tolerance: buffer as a potential interruption.
                        pending_total += run.count;
                        pending.push(run);
                        current = Some(region);
                    } else {
                        // Tolerance exceeded: close the current region and start fresh.
                        seq_regions.push(region);
                        pending.clear();
                        pending_total = 0;
                        if run.count >= spec.min_run {
                            current = Some(Region {
                                cat: run.cat,
                                start: run.start,
                                end: run.end,
                                feature_count: run.count,
                                interruptions: Vec::new(),
                            });
                        }
                    }
                }
            }
        }
        if let Some(region) = current {
            seq_regions.push(region);
        }

        // Apply boundary adjustments between adjacent regions.
        if matches!(spec.bounds, RegionBounds::Midpoints) && seq_regions.len() >= 2 {
            for i in 0..seq_regions.len() - 1 {
                let gap_end = seq_regions[i].end;
                let gap_start = seq_regions[i + 1].start;
                let raw_mid = (gap_end + gap_start) / 2;
                let mid = if let Some(max_exp) = spec.max_expansion {
                    raw_mid
                        .min(gap_end.saturating_add(max_exp))
                        .max(gap_start.saturating_sub(max_exp))
                } else {
                    raw_mid
                };
                seq_regions[i].end = mid;
                seq_regions[i + 1].start = mid;
            }
        }

        // Convert to JSON, attaching interruptions where present.
        for region in seq_regions {
            let mut obj = json!({
                "sequenceId": seq_id,
                "assemblyId": assembly_id,
                "start": region.start,
                "end": region.end,
                "catValue": region.cat,
                "featureCount": region.feature_count,
                "xOffset": x_off
            });
            if !region.interruptions.is_empty() {
                obj["interruptions"] = json!(region
                    .interruptions
                    .iter()
                    .map(|r| json!({
                        "catValue": r.cat,
                        "start": r.start,
                        "end": r.end,
                        "featureCount": r.count
                    }))
                    .collect::<Vec<_>>());
            }
            all_regions.push(obj);
        }
    }

    all_regions
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Compute an Oxford, ribbon, or painting plot from local feature sets.
///
/// `feature_sets[0]` is always treated as the reference assembly.
/// For Oxford the slice must have exactly 2 elements; for painting exactly 1;
/// for ribbon ≥ 2.
///
/// Sequence lengths are derived automatically when `sequence_lengths` is empty,
/// setting `lengths_derived = true` on the affected set.
///
/// `regions_spec` adds a `regions` array to the output when `Some`.  Regions
/// are computed per-assembly by grouping adjacent same-category features.
pub fn positional_from_features(
    feature_sets: &mut [LocalFeatureSet],
    report_type: &str,
    reorient: bool,
    cat_field: Option<&str>,
    window_size: Option<u64>,
    max_connections_per_group: usize,
    regions_spec: Option<&RegionsSpec>,
) -> Result<Value, LayoutError> {
    if feature_sets.is_empty() {
        return Err(LayoutError::NoAssemblies);
    }

    match report_type {
        "oxford" if feature_sets.len() != 2 => {
            return Err(LayoutError::IncompatibleAssemblyCounts {
                report_type: "oxford",
                required: "exactly 2",
                got: feature_sets.len(),
            })
        }
        "painting" if feature_sets.len() != 1 => {
            return Err(LayoutError::IncompatibleAssemblyCounts {
                report_type: "painting",
                required: "exactly 1",
                got: feature_sets.len(),
            })
        }
        "ribbon" if feature_sets.len() < 2 => {
            return Err(LayoutError::IncompatibleAssemblyCounts {
                report_type: "ribbon",
                required: "at least 2",
                got: feature_sets.len(),
            })
        }
        _ => {}
    }

    let assembly_ids: Vec<String> = feature_sets.iter().map(|s| s.assembly_id.clone()).collect();
    let assembly_id_refs: Vec<&str> = assembly_ids.iter().map(String::as_str).collect();

    let layouts = build_local_layouts(&assembly_id_refs, feature_sets, reorient);

    let mut result = match report_type {
        "painting" => {
            let asm_id = &assembly_ids[0];
            let feats: Vec<&LocalFeature> = feature_sets[0].features.iter().collect();
            build_painting(asm_id, &layouts, &feats, cat_field, window_size)
        }
        _ => build_oxford_ribbon(
            report_type,
            &assembly_id_refs,
            &layouts,
            feature_sets,
            cat_field,
            max_connections_per_group,
        ),
    };

    if let Some(spec) = regions_spec {
        let mut all_regions: Vec<Value> = Vec::new();
        for (asm_id, set) in assembly_ids.iter().zip(feature_sets.iter()) {
            if let Some(layout) = layouts.get(asm_id.as_str()) {
                let feats: Vec<&LocalFeature> = set.features.iter().collect();
                let mut asm_regions = compute_regions(asm_id, &feats, layout, spec);
                all_regions.append(&mut asm_regions);
            }
        }
        result["regions"] = json!(all_regions);
    }

    Ok(result)
}

/// Combine a rendered remote positional report with one or more local feature sets.
///
/// `remote_report_json` must be the `report` field from a `POST /api/v3/positional`
/// response.  The `points` entries must include a `"group"` field (which the API
/// emits for 1:1 oxford/ribbon points) and the `assemblies` object must list the
/// reference assembly first.
///
/// Each entry in `local_sets` is treated as an additional comparison assembly.
/// The reference assembly layout is reconstructed from the `assemblies` metadata
/// in the remote report; local assembly layouts are computed using the standard
/// layout algorithm against the remote reference positions.
///
/// Returns a `serde_json::Value` in the same format as `positional_from_features`.
pub fn hybrid_positional(
    remote_report_json: &str,
    local_sets: &mut [LocalFeatureSet],
    reorient: bool,
    max_connections_per_group: usize,
) -> Result<Value, LayoutError> {
    if local_sets.is_empty() {
        return Err(LayoutError::NoAssemblies);
    }

    // Ensure local sequence lengths are populated before layout
    for set in local_sets.iter_mut() {
        if set.sequence_lengths.is_empty() {
            set.derive_lengths();
        }
    }

    let remote: Value = serde_json::from_str(remote_report_json)
        .map_err(|e| LayoutError::RemoteReportParse(e.to_string()))?;

    let report_type = remote
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("oxford");

    // Extract remote assemblies — ordered as in the remote report
    let remote_assemblies_obj = match remote.get("assemblies").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => {
            return Err(LayoutError::RemoteReportParse(
                "missing 'assemblies' object".to_string(),
            ))
        }
    };

    // The reference assembly is the first key.  Remote reports serialize assemblies
    // as a JSON object keyed by assembly ID; we preserve the order from the original
    // assembly_ids list (present in the outer response), but since we only have the
    // report here, we accept the first key as the reference.
    let ref_id: String = remote_assemblies_obj
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| LayoutError::RemoteReportParse("assemblies map is empty".to_string()))?;

    // Reconstruct reference layout from remote assemblies metadata
    let ref_seqs: Vec<SequenceLayout> = {
        let obj = match remote_assemblies_obj[&ref_id].as_object() {
            Some(o) => o,
            None => {
                return Err(LayoutError::RemoteReportParse(format!(
                    "assembly '{}' has no metadata",
                    ref_id
                )))
            }
        };
        obj.get("sequences")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| {
                        let id = s.get("id")?.as_str()?.to_string();
                        let length = s.get("length")?.as_u64()?;
                        let offset = s.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
                        let orientation =
                            s.get("orientation").and_then(|v| v.as_i64()).unwrap_or(1) as i8;
                        Some(SequenceLayout {
                            sequence_id: id,
                            length,
                            offset,
                            orientation,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let ref_total_span = ref_seqs.last().map(|s| s.offset + s.length).unwrap_or(0);

    // Build group → reference global position from remote points
    // Points have "group" (1:1) or connections have "group" (M:N)
    let mut group_to_ref_pos: HashMap<String, u64> = HashMap::new();

    if let Some(points) = remote.get("points").and_then(|v| v.as_array()) {
        for pt in points {
            if let (Some(group), Some(x)) = (
                pt.get("group").and_then(|v| v.as_str()),
                pt.get("x").and_then(|v| v.as_u64()),
            ) {
                group_to_ref_pos.entry(group.to_string()).or_insert(x);
            }
        }
    }
    // Also extract from connections (M:N groups)
    if let Some(conns) = remote.get("connections").and_then(|v| v.as_array()) {
        for conn in conns {
            if let (Some(group), Some(x_arr)) = (
                conn.get("group").and_then(|v| v.as_str()),
                conn.get("xCoords").and_then(|v| v.as_array()),
            ) {
                if let Some(x) = x_arr.first().and_then(|v| v.as_u64()) {
                    group_to_ref_pos.entry(group.to_string()).or_insert(x);
                }
            }
        }
    }

    if group_to_ref_pos.is_empty() {
        return Err(LayoutError::NoSharedGroups);
    }

    // ref_offset_map: seq_id → offset (used by order_sequences_by_median)
    let ref_offset_map: HashMap<String, u64> = ref_seqs
        .iter()
        .map(|s| (s.sequence_id.clone(), s.offset))
        .collect();

    // Build layouts for local assemblies
    let local_ids: Vec<&str> = local_sets.iter().map(|s| s.assembly_id.as_str()).collect();

    let mut local_layouts: HashMap<String, AssemblyLayout> = HashMap::new();
    for (set_idx, &cmp_id) in local_ids.iter().enumerate() {
        let cmp_set = &local_sets[set_idx];

        let cmp_seqs_init: Vec<SequenceLayout> = cmp_set
            .sequence_lengths
            .iter()
            .map(|(id, &len)| SequenceLayout {
                sequence_id: id.clone(),
                length: len,
                offset: 0,
                orientation: 1,
            })
            .collect();

        let seq_to_groups: HashMap<String, Vec<String>> = {
            let mut m: HashMap<String, Vec<String>> = HashMap::new();
            for f in &cmp_set.features {
                m.entry(f.sequence_id.clone())
                    .or_default()
                    .push(f.group.clone());
            }
            m
        };

        let mut sorted = order_sequences_by_median(
            &cmp_seqs_init,
            &ref_offset_map,
            &group_to_ref_pos,
            &seq_to_groups,
        );

        if reorient {
            for seq in &mut sorted {
                let pairs: Vec<(f64, f64)> = cmp_set
                    .features
                    .iter()
                    .filter(|f| f.sequence_id == seq.sequence_id)
                    .filter_map(|f| {
                        group_to_ref_pos
                            .get(&f.group)
                            .map(|&ref_pos| (ref_pos as f64, f.start as f64))
                    })
                    .collect();
                seq.orientation = orient_sequence(&pairs);
            }
        }

        compute_offsets(&mut sorted);
        let span = sorted.last().map(|s| s.offset + s.length).unwrap_or(0);
        local_layouts.insert(
            cmp_id.to_string(),
            AssemblyLayout {
                sequences: sorted,
                total_span: span,
                lengths_derived: cmp_set.lengths_derived,
            },
        );
    }

    // Reconstruct group_to_ref_all (Vec of virtual ref features from remote points)
    // Each remote point gives us: group → (x, x2, strand)
    // We represent them as lightweight ad-hoc maps.
    let mut group_to_ref_entries: HashMap<&str, Vec<(u64, u64, i8)>> = HashMap::new();
    if let Some(points) = remote.get("points").and_then(|v| v.as_array()) {
        for pt in points {
            let group = match pt.get("group").and_then(|v| v.as_str()) {
                Some(g) => g,
                None => continue,
            };
            let x = pt.get("x").and_then(|v| v.as_u64()).unwrap_or(0);
            let x2 = pt.get("x2").and_then(|v| v.as_u64()).unwrap_or(x);
            let strand = pt.get("strand").and_then(|v| v.as_i64()).unwrap_or(1) as i8;
            group_to_ref_entries
                .entry(group)
                .or_default()
                .push((x, x2, strand));
        }
    }
    if let Some(conns) = remote.get("connections").and_then(|v| v.as_array()) {
        for conn in conns {
            let group = match conn.get("group").and_then(|v| v.as_str()) {
                Some(g) => g,
                None => continue,
            };
            let x_arr = conn
                .get("xCoords")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let x2_arr = conn
                .get("x2Coords")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let strand_arr = conn
                .get("xStrands")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for ((xi, x2i), si) in x_arr.iter().zip(x2_arr.iter()).zip(strand_arr.iter()) {
                let x = xi.as_u64().unwrap_or(0);
                let x2 = x2i.as_u64().unwrap_or(x);
                let strand = si.as_i64().unwrap_or(1) as i8;
                group_to_ref_entries
                    .entry(group)
                    .or_default()
                    .push((x, x2, strand));
            }
        }
    }

    // Build output points / connections for each local assembly vs. the remote reference
    let mut all_points: Vec<Value> = Vec::new();
    let mut all_connections: Vec<Value> = Vec::new();
    let mut cat_counts: HashMap<String, u64> = HashMap::new();

    for (set_idx, &cmp_id) in local_ids.iter().enumerate() {
        let cmp_set = &local_sets[set_idx];
        let cmp_layout = local_layouts.get(cmp_id);
        let cmp_offsets = offset_map(cmp_layout);
        let cmp_orientations: HashMap<String, i8> = cmp_layout
            .map(|l| {
                l.sequences
                    .iter()
                    .map(|s| (s.sequence_id.clone(), s.orientation))
                    .collect()
            })
            .unwrap_or_default();

        let mut cmp_by_group: HashMap<&str, Vec<&LocalFeature>> = HashMap::new();
        for f in &cmp_set.features {
            cmp_by_group.entry(&f.group).or_default().push(f);
        }

        for (group, cmp_feats) in &cmp_by_group {
            let ref_entries = match group_to_ref_entries.get(group) {
                Some(v) => v,
                None => continue,
            };

            let cat = cmp_feats
                .first()
                .and_then(|f| f.cat.as_deref())
                .unwrap_or("");
            if !cat.is_empty() {
                *cat_counts.entry(cat.to_string()).or_insert(0) += 1;
            }

            let is_mn = ref_entries.len() > 1 || cmp_feats.len() > 1;

            if !is_mn {
                let (x, x2, x_strand) = ref_entries[0];
                let cf = cmp_feats[0];
                let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);

                let mut point = json!({
                    "x": x, "x2": x2,
                    "y": y, "y2": y2,
                    "group": group,
                    "strand": x_strand,
                    "yStrand": y_orient * cf.strand
                });
                if !cat.is_empty() {
                    point["cat"] = json!(cat);
                }
                all_points.push(point);
            } else {
                let mut x_coords: Vec<u64> = Vec::new();
                let mut x2_coords: Vec<u64> = Vec::new();
                let mut x_strands: Vec<i8> = Vec::new();
                let mut y_coords: Vec<u64> = Vec::new();
                let mut y2_coords: Vec<u64> = Vec::new();
                let mut y_seq_ids: Vec<&str> = Vec::new();
                let mut y_strands: Vec<i8> = Vec::new();

                for &(x, x2, strand) in ref_entries {
                    x_coords.push(x);
                    x2_coords.push(x2);
                    x_strands.push(strand);
                }
                for cf in cmp_feats {
                    let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                    let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);
                    y_coords.push(y);
                    y2_coords.push(y2);
                    y_seq_ids.push(&cf.sequence_id);
                    y_strands.push(y_orient * cf.strand);
                }

                let total = x_coords.len() * y_coords.len();
                let truncated = total > max_connections_per_group;

                let mut conn = json!({
                    "group":     group,
                    "xCoords":   x_coords,
                    "x2Coords":  x2_coords,
                    "xStrands":  x_strands,
                    "yCoords":   y_coords,
                    "y2Coords":  y2_coords,
                    "ySeqIds":   y_seq_ids,
                    "yStrands":  y_strands,
                    "truncated": truncated
                });
                if !cat.is_empty() {
                    conn["catValue"] = json!(cat);
                }
                all_connections.push(conn);
            }
        }
    }

    // Build remote assembly metadata JSON block (extracted from remote report)
    let remote_asm_json: Value = remote.get("assemblies").cloned().unwrap_or(json!({}));

    // Build local assembly metadata JSON block
    let local_asm_json = serialise_assembly_metadata(&local_ids, &local_layouts);

    // Merge: start with remote assemblies, add local
    let mut merged_assemblies = match remote_asm_json.as_object().cloned() {
        Some(m) => m,
        None => serde_json::Map::new(),
    };
    // But replace the remote reference with a version that has lengthsDerived: false
    if let Some(ref_entry) = merged_assemblies.get_mut(&ref_id) {
        if let Some(obj) = ref_entry.as_object_mut() {
            obj.entry("lengthsDerived".to_string())
                .or_insert(json!(false));
        }
    }
    // Reference assembly sequences are already in merged_assemblies via remote_asm_json.
    // No additional insertion needed here.

    if let Some(local_obj) = local_asm_json.as_object() {
        for (k, v) in local_obj {
            merged_assemblies.insert(k.clone(), v.clone());
        }
    }

    // Add reference assembly layout block if missing (totalSpan)
    if let Some(ref_entry) = merged_assemblies.get_mut(&ref_id) {
        if let Some(obj) = ref_entry.as_object_mut() {
            if !obj.contains_key("domain") {
                obj.insert("domain".to_string(), json!([0, ref_total_span]));
            }
        }
    }

    let mut cats_vec: Vec<_> = cat_counts.into_iter().collect();
    cats_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let cats_json: Vec<Value> = cats_vec
        .iter()
        .map(|(k, _)| json!({"key": k, "label": k}))
        .collect();

    Ok(json!({
        "type": report_type,
        "assemblies": Value::Object(merged_assemblies),
        "points": all_points,
        "connections": all_connections,
        "cats": cats_json
    }))
}

// ── JSON string wrappers for PyO3 / WASM ────────────────────────────────────

/// Compute a positional report from JSON-encoded local feature sets.
///
/// `feature_sets_json` must be a JSON array of serialised [`LocalFeatureSet`] objects.
/// `report_type` is one of `"oxford"`, `"ribbon"`, or `"painting"`.
/// `cat_field` is the category field name; pass `""` for none.
/// `window_size` is 0 for no windowing.
/// `max_connections_per_group` caps M:N connections (0 → default 25).
///
/// Returns a JSON string.  On error, returns `{"error":"<message>"}`.
/// JSON-string wrapper for [`positional_from_features`].
///
/// `regions_json` is either `""` (skip) or a JSON-serialised [`RegionsSpec`].
pub fn positional_from_features_json(
    feature_sets_json: &str,
    report_type: &str,
    reorient: bool,
    cat_field: &str,
    window_size: u64,
    max_connections_per_group: usize,
    regions_json: &str,
) -> String {
    let mut sets: Vec<LocalFeatureSet> = match serde_json::from_str(feature_sets_json) {
        Ok(v) => v,
        Err(e) => return json!({"error": format!("invalid feature_sets_json: {e}")}).to_string(),
    };

    let cat = if cat_field.is_empty() {
        None
    } else {
        Some(cat_field)
    };
    let ws = if window_size == 0 {
        None
    } else {
        Some(window_size)
    };
    let max_conn = if max_connections_per_group == 0 {
        25
    } else {
        max_connections_per_group
    };

    let regions_spec: Option<RegionsSpec> = if regions_json.is_empty() {
        None
    } else {
        match serde_json::from_str(regions_json) {
            Ok(s) => Some(s),
            Err(e) => return json!({"error": format!("invalid regions_json: {e}")}).to_string(),
        }
    };

    match positional_from_features(
        &mut sets,
        report_type,
        reorient,
        cat,
        ws,
        max_conn,
        regions_spec.as_ref(),
    ) {
        Ok(v) => v.to_string(),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

/// Combine a remote positional report with local feature sets.
///
/// `remote_report_json` is the `report` field from a `POST /api/v3/positional` response.
/// `local_feature_sets_json` is a JSON array of serialised [`LocalFeatureSet`] objects.
/// `max_connections_per_group` caps M:N connections (0 → default 25).
///
/// Returns a JSON string.  On error, returns `{"error":"<message>"}`.
pub fn hybrid_positional_json(
    remote_report_json: &str,
    local_feature_sets_json: &str,
    reorient: bool,
    max_connections_per_group: usize,
) -> String {
    let mut sets: Vec<LocalFeatureSet> = match serde_json::from_str(local_feature_sets_json) {
        Ok(v) => v,
        Err(e) => {
            return json!({"error": format!("invalid local_feature_sets_json: {e}")}).to_string()
        }
    };

    let max_conn = if max_connections_per_group == 0 {
        25
    } else {
        max_connections_per_group
    };

    match hybrid_positional(remote_report_json, &mut sets, reorient, max_conn) {
        Ok(v) => v.to_string(),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

// ── Parse helpers exposed for PyO3 / WASM ───────────────────────────────────

/// Parse a two-column name→category file and return a JSON object string.
///
/// Returns `{"name1":"cat1",...}` or `{"error":"..."}`.  The result can be
/// used to override feature `cat` values after parsing a BUSCO or feature TSV.
pub fn parse_cat_file_json(content: &str) -> String {
    crate::parse_local::cat_file::parse_cat_file_json(content)
}

/// Parse a BUSCO `full_table.tsv` and return a JSON-encoded [`LocalFeatureSet`].
///
/// On parse error, returns `{"error":"<message>"}`.
pub fn parse_busco_tsv_json(assembly_id: &str, content: &str) -> String {
    match crate::parse_local::busco::parse_busco_tsv(assembly_id, content) {
        Ok(set) => serde_json::to_string(&set)
            .unwrap_or_else(|e| json!({"error": format!("serialisation failed: {e}")}).to_string()),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

/// Parse a samtools `.fai` index and return a JSON-encoded `sequence_id → length` map.
///
/// On parse error, returns `{"error":"<message>"}`.
pub fn parse_fai_json(content: &str) -> String {
    match crate::parse_local::fai::parse_fai(content) {
        Ok(map) => serde_json::to_string(&map)
            .unwrap_or_else(|e| json!({"error": format!("serialisation failed: {e}")}).to_string()),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

/// Parse a two-column lengths TSV and return a JSON-encoded `sequence_id → length` map.
///
/// On parse error, returns `{"error":"<message>"}`.
pub fn parse_lengths_tsv_json(content: &str) -> String {
    match crate::parse_local::lengths::parse_lengths_tsv(content) {
        Ok(map) => serde_json::to_string(&map)
            .unwrap_or_else(|e| json!({"error": format!("serialisation failed: {e}")}).to_string()),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_set(
        id: &str,
        features: Vec<(&str, &str, u64, u64, i8)>,
        lengths: Vec<(&str, u64)>,
    ) -> LocalFeatureSet {
        LocalFeatureSet {
            assembly_id: id.to_string(),
            features: features
                .into_iter()
                .map(|(group, seq, start, end, strand)| LocalFeature {
                    group: group.to_string(),
                    sequence_id: seq.to_string(),
                    start,
                    end,
                    strand,
                    cat: None,
                })
                .collect(),
            sequence_lengths: lengths
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            lengths_derived: false,
        }
    }

    #[test]
    fn test_oxford_two_assemblies() {
        let mut sets = vec![
            make_set(
                "asm_A",
                vec![
                    ("gene1", "chr1", 1000, 2000, 1),
                    ("gene2", "chr1", 5000, 6000, 1),
                ],
                vec![("chr1", 10_000_000)],
            ),
            make_set(
                "asm_B",
                vec![
                    ("gene1", "chrA", 3000, 4000, 1),
                    ("gene2", "chrA", 7000, 8000, 1),
                ],
                vec![("chrA", 10_000_000)],
            ),
        ];
        let result =
            positional_from_features(&mut sets, "oxford", true, None, None, 25, None).unwrap();
        assert_eq!(result["type"].as_str(), Some("oxford"));
        let points = result["points"].as_array().unwrap();
        assert_eq!(points.len(), 2, "expected 2 points for 2 shared genes");
    }

    #[test]
    fn test_painting_single_assembly_windowed() {
        let mut sets = vec![make_set(
            "asm_A",
            vec![
                ("gene1", "chr1", 100_000, 110_000, 1),
                ("gene2", "chr1", 1_200_000, 1_210_000, 1),
            ],
            vec![("chr1", 3_000_000)],
        )];
        let result = positional_from_features(
            &mut sets,
            "painting",
            false,
            None,
            Some(1_000_000),
            25,
            None,
        )
        .unwrap();
        assert_eq!(result["type"].as_str(), Some("painting"));
        let segs = result["segments"].as_array().unwrap();
        // gene1 → window 0, gene2 → window 1_000_000 → 2 windows
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn test_derive_lengths_fallback() {
        let mut sets = vec![
            make_set(
                "asm_A",
                vec![("gene1", "chr1", 0, 5_000_000, 1)],
                vec![], // no lengths supplied
            ),
            make_set(
                "asm_B",
                vec![("gene1", "chrA", 0, 5_000_000, 1)],
                vec![], // no lengths supplied
            ),
        ];
        let result =
            positional_from_features(&mut sets, "oxford", false, None, None, 25, None).unwrap();
        // Both sets should now have lengths_derived=true
        assert!(sets[0].lengths_derived);
        assert!(sets[1].lengths_derived);
        // Assembly metadata should carry lengthsDerived:true
        let asm = &result["assemblies"]["asm_A"];
        assert_eq!(asm["lengthsDerived"].as_bool(), Some(true));
    }

    #[test]
    fn test_no_assemblies_error() {
        let mut sets: Vec<LocalFeatureSet> = vec![];
        let err = positional_from_features(&mut sets, "oxford", false, None, None, 25, None);
        assert!(matches!(err, Err(LayoutError::NoAssemblies)));
    }

    #[test]
    fn test_oxford_wrong_count_error() {
        let mut sets = vec![make_set("A", vec![], vec![])];
        let err = positional_from_features(&mut sets, "oxford", false, None, None, 25, None);
        assert!(matches!(
            err,
            Err(LayoutError::IncompatibleAssemblyCounts {
                report_type: "oxford",
                ..
            })
        ));
    }

    #[test]
    fn test_non_overlapping_features_returns_empty_points() {
        let mut sets = vec![
            make_set(
                "asm_A",
                vec![("gene1", "chr1", 0, 1000, 1)],
                vec![("chr1", 1_000_000)],
            ),
            make_set(
                "asm_B",
                vec![("geneX", "chrA", 0, 1000, 1)],
                vec![("chrA", 1_000_000)],
            ),
        ];
        let result =
            positional_from_features(&mut sets, "oxford", false, None, None, 25, None).unwrap();
        let points = result["points"].as_array().unwrap();
        assert!(points.is_empty(), "no shared groups → no points");
    }

    #[test]
    fn test_hybrid_from_remote_report() {
        // Build a minimal remote report JSON (as returned by the API)
        let remote_report = json!({
            "type": "oxford",
            "assemblies": {
                "GCA_remote": {
                    "sequences": [
                        {"id": "chr1", "length": 10_000_000u64, "offset": 0, "orientation": 1}
                    ],
                    "domain": [0, 10_000_000u64],
                    "buckets": [0]
                }
            },
            "points": [
                {"x": 1000u64, "x2": 2000u64, "y": 100u64, "y2": 200u64, "group": "gene1", "strand": 1, "yStrand": 1},
                {"x": 5000u64, "x2": 6000u64, "y": 500u64, "y2": 600u64, "group": "gene2", "strand": 1, "yStrand": 1}
            ],
            "connections": []
        });

        let mut local_sets = vec![make_set(
            "my_new_asm",
            vec![
                ("gene1", "chrA", 3000, 4000, 1),
                ("gene2", "chrA", 8000, 9000, 1),
            ],
            vec![("chrA", 12_000_000)],
        )];

        let result =
            hybrid_positional(&remote_report.to_string(), &mut local_sets, true, 25).unwrap();

        assert_eq!(result["type"].as_str(), Some("oxford"));
        let points = result["points"].as_array().unwrap();
        assert_eq!(points.len(), 2, "both shared genes should produce a point");
        // Remote assembly metadata should be preserved
        assert!(result["assemblies"].get("GCA_remote").is_some());
        // Local assembly metadata should be added
        assert!(result["assemblies"].get("my_new_asm").is_some());
    }

    // ── Region computation tests ──────────────────────────────────────────────

    /// Build a minimal AssemblyLayout for region tests.
    fn make_layout(seq_id: &str, length: u64) -> AssemblyLayout {
        AssemblyLayout {
            sequences: vec![SequenceLayout {
                sequence_id: seq_id.to_string(),
                length,
                offset: 0,
                orientation: 1,
            }],
            total_span: length,
            lengths_derived: false,
        }
    }

    /// Build a LocalFeature with an assigned category.
    fn make_feat(seq: &str, start: u64, end: u64, cat: Option<&str>) -> LocalFeature {
        LocalFeature {
            group: "g".to_string(),
            sequence_id: seq.to_string(),
            start,
            end,
            strand: 1,
            cat: cat.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_regions_basic_two_cats() {
        // 3 × A then 3 × B on one sequence.
        let feats: Vec<LocalFeature> = vec![
            make_feat("chr1", 0, 100, Some("A")),
            make_feat("chr1", 101, 200, Some("A")),
            make_feat("chr1", 201, 300, Some("A")),
            make_feat("chr1", 301, 400, Some("B")),
            make_feat("chr1", 401, 500, Some("B")),
            make_feat("chr1", 501, 600, Some("B")),
        ];
        let layout = make_layout("chr1", 700);
        let spec = RegionsSpec {
            min_run: 1,
            tolerance: 0,
            ..Default::default()
        };
        let feat_refs: Vec<&LocalFeature> = feats.iter().collect();
        let regions = compute_regions("asm", &feat_refs, &layout, &spec);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0]["catValue"].as_str(), Some("A"));
        assert_eq!(regions[0]["featureCount"].as_u64(), Some(3));
        assert_eq!(regions[1]["catValue"].as_str(), Some("B"));
        assert_eq!(regions[1]["featureCount"].as_u64(), Some(3));
    }

    #[test]
    fn test_regions_unassigned_features_are_skipped() {
        // A, unassigned, A — the unassigned feature should not break the A region.
        let feats: Vec<LocalFeature> = vec![
            make_feat("chr1", 0, 100, Some("A")),
            make_feat("chr1", 101, 200, None), // unassigned — transparent
            make_feat("chr1", 201, 300, Some("A")),
        ];
        let layout = make_layout("chr1", 400);
        let spec = RegionsSpec {
            min_run: 1,
            tolerance: 0,
            ..Default::default()
        };
        let feat_refs: Vec<&LocalFeature> = feats.iter().collect();
        let regions = compute_regions("asm", &feat_refs, &layout, &spec);
        // Two raw A runs merged into one because unassigned is skipped entirely.
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0]["catValue"].as_str(), Some("A"));
        assert_eq!(regions[0]["featureCount"].as_u64(), Some(2));
    }

    #[test]
    fn test_regions_tolerance_absorbs_interruption() {
        // 5 × A, 1 × B (within tolerance=1), 5 × A → single A region with one B interruption.
        let feats: Vec<LocalFeature> = vec![
            make_feat("chr1", 0, 100, Some("A")),
            make_feat("chr1", 101, 200, Some("A")),
            make_feat("chr1", 201, 300, Some("A")),
            make_feat("chr1", 301, 400, Some("A")),
            make_feat("chr1", 401, 500, Some("A")),
            make_feat("chr1", 501, 600, Some("B")), // tolerated interruption
            make_feat("chr1", 601, 700, Some("A")),
            make_feat("chr1", 701, 800, Some("A")),
            make_feat("chr1", 801, 900, Some("A")),
            make_feat("chr1", 901, 1000, Some("A")),
            make_feat("chr1", 1001, 1100, Some("A")),
        ];
        let layout = make_layout("chr1", 1200);
        let spec = RegionsSpec {
            min_run: 1,
            tolerance: 1,
            ..Default::default()
        };
        let feat_refs: Vec<&LocalFeature> = feats.iter().collect();
        let regions = compute_regions("asm", &feat_refs, &layout, &spec);
        assert_eq!(
            regions.len(),
            1,
            "B is within tolerance so only one A region"
        );
        assert_eq!(regions[0]["catValue"].as_str(), Some("A"));
        assert_eq!(regions[0]["featureCount"].as_u64(), Some(10));
        let interruptions = regions[0]["interruptions"].as_array().unwrap();
        assert_eq!(interruptions.len(), 1);
        assert_eq!(interruptions[0]["catValue"].as_str(), Some("B"));
        assert_eq!(interruptions[0]["featureCount"].as_u64(), Some(1));
    }

    #[test]
    fn test_regions_tolerance_exceeded_breaks_region() {
        // 5 × A, 2 × B (tolerance=1, exceeded), 5 × A → two separate A regions.
        let feats: Vec<LocalFeature> = vec![
            make_feat("chr1", 0, 100, Some("A")),
            make_feat("chr1", 101, 200, Some("A")),
            make_feat("chr1", 201, 300, Some("A")),
            make_feat("chr1", 301, 400, Some("A")),
            make_feat("chr1", 401, 500, Some("A")),
            make_feat("chr1", 501, 600, Some("B")),
            make_feat("chr1", 601, 700, Some("B")), // 2 > tolerance=1, breaks region
            make_feat("chr1", 701, 800, Some("A")),
            make_feat("chr1", 801, 900, Some("A")),
            make_feat("chr1", 901, 1000, Some("A")),
            make_feat("chr1", 1001, 1100, Some("A")),
            make_feat("chr1", 1101, 1200, Some("A")),
        ];
        let layout = make_layout("chr1", 1300);
        let spec = RegionsSpec {
            min_run: 1,
            tolerance: 1,
            ..Default::default()
        };
        let feat_refs: Vec<&LocalFeature> = feats.iter().collect();
        let regions = compute_regions("asm", &feat_refs, &layout, &spec);
        assert_eq!(regions.len(), 3, "A, B, A");
        assert_eq!(regions[0]["catValue"].as_str(), Some("A"));
        assert_eq!(regions[1]["catValue"].as_str(), Some("B"));
        assert_eq!(regions[2]["catValue"].as_str(), Some("A"));
    }

    #[test]
    fn test_regions_min_run_filters_short_runs() {
        // 2 × A (< min_run=3), 5 × B, 1 × A (< min_run=3), 5 × B → only B regions.
        let feats: Vec<LocalFeature> = vec![
            make_feat("chr1", 0, 100, Some("A")),
            make_feat("chr1", 101, 200, Some("A")),
            make_feat("chr1", 201, 300, Some("B")),
            make_feat("chr1", 301, 400, Some("B")),
            make_feat("chr1", 401, 500, Some("B")),
            make_feat("chr1", 501, 600, Some("B")),
            make_feat("chr1", 601, 700, Some("B")),
            make_feat("chr1", 701, 800, Some("A")), // 1 < min_run=3
            make_feat("chr1", 801, 900, Some("B")),
            make_feat("chr1", 901, 1000, Some("B")),
            make_feat("chr1", 1001, 1100, Some("B")),
            make_feat("chr1", 1101, 1200, Some("B")),
            make_feat("chr1", 1201, 1300, Some("B")),
        ];
        let layout = make_layout("chr1", 1400);
        // With min_run=3 and tolerance=1: single A features can't start regions
        // but 1 A can be absorbed as interruption (within tolerance=1).
        let spec = RegionsSpec {
            min_run: 3,
            tolerance: 1,
            ..Default::default()
        };
        let feat_refs: Vec<&LocalFeature> = feats.iter().collect();
        let regions = compute_regions("asm", &feat_refs, &layout, &spec);
        // First A(2) at start: can't start a region (2 < min_run=3), discarded
        // B(5) starts a region
        // A(1) = 1 <= tolerance=1: absorbed as interruption in B region
        // B(5) extends the B region
        // → 1 B region (10 features, 1 A interruption)
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0]["catValue"].as_str(), Some("B"));
        assert_eq!(regions[0]["featureCount"].as_u64(), Some(10));
        let interruptions = regions[0]["interruptions"].as_array().unwrap();
        assert_eq!(interruptions.len(), 1);
        assert_eq!(interruptions[0]["catValue"].as_str(), Some("A"));
    }
}
