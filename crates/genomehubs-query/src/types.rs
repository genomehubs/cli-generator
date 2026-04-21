//! Portable types for describe and snippet generation in WASM.
//!
//! These types are extracted from src/core for use in the genomehubs-query
//! subcrate, which must compile to WASM and has no heavy dependencies.

use serde::{Deserialize, Serialize};

// ── Field metadata ────────────────────────────────────────────────────────────

/// Metadata for a single API field, parsed from the `resultFields` endpoint.
///
/// Extracted from src/core/fetch.rs for use in WASM-compiled code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldDef {
    /// Internal field name used in API queries, e.g. `"genome_size"`.
    ///
    /// Not present in the inner JSON object (the name is the map key), so we
    /// default to an empty string and set it manually during parsing.
    #[serde(default)]
    pub name: String,
    /// Display group for grouping related fields, e.g. `"genome_size"`.
    #[serde(default)]
    pub display_group: Option<String>,
    /// Human-readable label, e.g. `"Genome size"`.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Short description of the field.
    #[serde(default)]
    pub description: Option<String>,
    /// Field data type as reported by the API, e.g. `"long"`, `"keyword"`.
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
    /// For keyword fields: the set of allowed enum values.
    #[serde(default)]
    pub constraint: Option<FieldConstraint>,
    /// Display priority level (1 = primary, 2 = secondary).
    #[serde(default)]
    pub display_level: Option<u8>,
    /// Alternative names by which this field is also known.
    #[serde(default)]
    pub synonyms: Vec<String>,
    /// Processed type used for validation, e.g. `"long"`, `"keyword"`, `"date"`.
    #[serde(default)]
    pub processed_type: Option<String>,
    /// Direction of value inheritance across the taxonomy tree.
    #[serde(default)]
    pub traverse_direction: Option<String>,
    /// Valid summary modifiers for this field, e.g. `["min", "max", "median"]`.
    #[serde(default)]
    pub summary: Vec<String>,
}

/// Constraint metadata for a field, used to enumerate keyword values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConstraint {
    /// Allowed values for keyword-type fields.
    #[serde(rename = "enum", default)]
    pub enum_values: Vec<String>,
}

// ── Site configuration (minimal) ──────────────────────────────────────────────

/// Minimal site configuration for snippet rendering in WASM.
///
/// Extracted from src/core/config.rs; only includes fields used by
/// the snippet generator.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SiteConfig {
    /// Short identifier, e.g. `"goat"`.
    pub name: String,
    /// Base URL of the API without trailing slash.
    pub api_base: String,
    /// Python/SDK package name, e.g. `"goat_sdk"`. Defaults to `"{name}_sdk"`.
    #[serde(default)]
    pub sdk_name: Option<String>,
}

impl SiteConfig {
    /// Return the Python package name for the generated SDK.
    pub fn resolved_sdk_name(&self) -> String {
        self.sdk_name
            .clone()
            .unwrap_or_else(|| format!("{}_sdk", self.name.replace('-', "_")))
    }
}

// ── Query snapshot (from snippet.rs) ──────────────────────────────────────────

/// Represents a single query as built by an SDK or UI.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct QuerySnapshot {
    /// Index name, e.g. `"taxon"` or `"assembly"`.
    #[serde(default)]
    pub index: String,
    /// Taxon names to filter by.
    #[serde(default)]
    pub taxa: Vec<String>,
    /// How the taxon filter is applied: `"name"`, `"tree"`, or `"lineage"`.
    #[serde(default)]
    pub taxon_filter: String,
    /// Restrict results to this taxonomic rank, e.g. `"species"`.
    #[serde(default)]
    pub rank: Option<String>,
    /// Filters: (field_name, operator, value)
    #[serde(default)]
    pub filters: Vec<(String, String, String)>,
    /// Sorts: (field_name, direction)
    #[serde(default)]
    pub sorts: Vec<(String, String)>,
    /// CLI flags, e.g., ["genome-size", "assembly"]
    #[serde(default)]
    pub flags: Vec<String>,
    /// Selected output fields
    #[serde(default)]
    pub selections: Vec<String>,
    /// Traversal context: (field_name, direction)
    #[serde(default)]
    pub traversal: Option<(String, String)>,
    /// Summaries: (field_name, modifier)
    #[serde(default)]
    pub summaries: Vec<(String, String)>,
}
