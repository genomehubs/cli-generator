//! [`LocalFeatureSet`] — shared output type for all local file parsers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single genomic feature position parsed from a local file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFeature {
    /// Shared marker identifier (e.g. BUSCO gene ID, GFF3 `Name`).
    pub group: String,
    /// Sequence / chromosome identifier.
    pub sequence_id: String,
    /// Start position in bp (0-based, inclusive).
    pub start: u64,
    /// End position in bp (exclusive).
    pub end: u64,
    /// `+1` or `-1`. Defaults to `+1` when absent in the source file.
    pub strand: i8,
    /// Optional category label (e.g. BUSCO status, feature type string).
    pub cat: Option<String>,
}

/// A parsed set of local features for one assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFeatureSet {
    /// User-supplied label for this assembly (e.g. `"my_new_assembly"`).
    pub assembly_id: String,
    /// Parsed feature positions.
    pub features: Vec<LocalFeature>,
    /// Sequence lengths keyed by `sequence_id`.
    ///
    /// Must be populated before calling [`crate::report::hybrid::positional_from_features`].
    /// If left empty, [`LocalFeatureSet::derive_lengths`] will be called automatically
    /// and `lengths_derived` will be set to `true`.
    pub sequence_lengths: HashMap<String, u64>,
    /// `true` when `sequence_lengths` were derived from `max(feature.end)` per sequence
    /// rather than supplied by the user.  Axis proportions will be approximate.
    pub lengths_derived: bool,
}

impl LocalFeatureSet {
    /// Derive sequence lengths from `max(feature.end)` per sequence.
    ///
    /// Called automatically when `sequence_lengths` is empty at layout time.
    /// Sets `lengths_derived = true`.
    pub fn derive_lengths(&mut self) {
        self.sequence_lengths.clear();
        for feat in &self.features {
            let entry = self
                .sequence_lengths
                .entry(feat.sequence_id.clone())
                .or_insert(0);
            if feat.end > *entry {
                *entry = feat.end;
            }
        }
        self.lengths_derived = true;
    }
}
