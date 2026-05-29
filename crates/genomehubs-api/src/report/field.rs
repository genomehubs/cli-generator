//! Field storage resolution: single source of truth for where a field lives in ES.
//!
//! Every ES nested/attribute/lineage path decision in agg builders, bounds
//! computation and extraction is derived from [`FieldStorage`].  No other file
//! should call `is_rank`, `is_attribute`, or `get_attribute_value_field`
//! directly; use [`resolve_field_storage`] instead.

use serde_json::{json, Value};

use crate::es_metadata::MetadataCache;
use genomehubs_query::report::axis::ValueType;

// ── FieldStorage ─────────────────────────────────────────────────────────────

/// Where a field's values are physically stored in the ES document.
///
/// All agg builders and extractors derive their nested path structure from
/// this enum so that the (x_type × cat_type) combinations are handled by
/// composing two `FieldStorage` values rather than hand-writing four cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldStorage {
    /// `attributes[].key == key`, value at `es_value_field`.
    Attribute {
        key: String,
        /// Full dotted ES field path, e.g. `"attributes.keyword_value.raw"`.
        es_value_field: String,
    },
    /// `lineage[].taxon_rank == rank`; canonical bucket key is `lineage.taxon_id`.
    Lineage { rank: String },
    /// Top-level document field; `es_field` includes `.keyword` suffix when needed.
    Root { es_field: String },
}

impl FieldStorage {
    /// ES `nested` path required before filtering this field, if any.
    #[allow(dead_code)]
    pub fn nested_path(&self) -> Option<&str> {
        match self {
            FieldStorage::Attribute { .. } => Some("attributes"),
            FieldStorage::Lineage { .. } => Some("lineage"),
            FieldStorage::Root { .. } => None,
        }
    }

    /// Term filter that restricts to documents/sub-docs containing this field.
    #[allow(dead_code)]
    pub fn key_filter(&self) -> Value {
        match self {
            FieldStorage::Attribute { key, .. } => json!({ "term": { "attributes.key": key } }),
            FieldStorage::Lineage { rank } => json!({ "term": { "lineage.taxon_rank": rank } }),
            FieldStorage::Root { .. } => json!({ "match_all": {} }),
        }
    }

    /// The name of the inner filter-container used inside the nested agg.
    ///
    /// - Attribute in *x* position: `"by_key"`
    /// - Lineage  in *x* position: `"at_rank"`
    /// - Root: `""` (no container needed)
    ///
    /// The same names are used in the per-cat inner x agg so extraction
    /// paths are deterministic.
    pub fn x_container_name(&self) -> &str {
        match self {
            FieldStorage::Attribute { .. } => "by_key",
            FieldStorage::Lineage { .. } => "at_rank",
            FieldStorage::Root { .. } => "",
        }
    }

    /// Names used when this field is in the *cat* position inside
    /// `categoryHistograms`.
    ///
    /// Returns `(outer_wrapper, inner_container)` for the cat-level nesting:
    ///
    /// ```text
    /// outer_wrapper: {
    ///   nested: {path: ...},
    ///   aggs: {
    ///     inner_container: { filter: ..., aggs: { by_value: ... } }
    ///   }
    /// }
    /// ```
    pub fn cat_wrapper_names(&self) -> (&str, &str) {
        match self {
            FieldStorage::Attribute { .. } => ("by_attribute", "by_cat"),
            FieldStorage::Lineage { .. } => ("by_lineage", "at_cat_rank"),
            FieldStorage::Root { .. } => ("", ""),
        }
    }

    /// Build a presence-existence filter at the *document* level.
    ///
    /// Used when anding presence filters into the base query so bounds
    /// reflect only documents that will actually appear in the final plot.
    pub fn presence_filter(&self) -> Value {
        match self {
            FieldStorage::Attribute { key, .. } => json!({
                "nested": {
                    "path": "attributes",
                    "query": { "term": { "attributes.key": key } }
                }
            }),
            FieldStorage::Lineage { rank } => json!({
                "nested": {
                    "path": "lineage",
                    "query": { "term": { "lineage.taxon_rank": rank } }
                }
            }),
            FieldStorage::Root { es_field } => json!({ "exists": { "field": es_field } }),
        }
    }

    /// The canonical ES field to bucket on (passed to `terms`, `histogram`, etc.).
    pub fn bucket_field(&self) -> &str {
        match self {
            FieldStorage::Attribute { es_value_field, .. } => es_value_field.as_str(),
            FieldStorage::Lineage { .. } => "lineage.taxon_id",
            FieldStorage::Root { es_field } => es_field.as_str(),
        }
    }

    // ── Path helpers ─────────────────────────────────────────────────────────

    /// JSON pointer to the main bucket list for a top-level agg (`agg_name`).
    ///
    /// ```text
    /// attribute x: /aggregations/{agg_name}/by_key/{bucket_type}/buckets
    /// lineage   x: /aggregations/{agg_name}/at_rank/{bucket_type}/buckets
    /// root      x: /aggregations/{agg_name}/{bucket_type}/buckets
    /// ```
    pub fn main_bucket_path(&self, agg_name: &str, bucket_type: &str) -> String {
        match self {
            FieldStorage::Attribute { .. } => {
                format!("/aggregations/{}/by_key/{}/buckets", agg_name, bucket_type)
            }
            FieldStorage::Lineage { .. } => {
                format!("/aggregations/{}/at_rank/{}/buckets", agg_name, bucket_type)
            }
            FieldStorage::Root { .. } => {
                format!("/aggregations/{}/{}/buckets", agg_name, bucket_type)
            }
        }
    }

    /// JSON pointer to the `by_value` buckets object inside `categoryHistograms`,
    /// given the x-storage (self) and cat-storage.
    ///
    /// ```text
    /// /aggregations/{agg_name}/{x_container}/categoryHistograms/{cat_outer}/{cat_inner}/by_value/buckets
    /// ```
    pub fn cat_histograms_base(
        &self,
        agg_name: &str,
        cat_storage: &FieldStorage,
    ) -> Option<String> {
        let x_container = self.x_container_name();
        let (cat_outer, cat_inner) = cat_storage.cat_wrapper_names();
        if cat_outer.is_empty() {
            // root cat not yet supported in category histogram path
            return None;
        }
        let path = if x_container.is_empty() {
            // root x
            format!(
                "/aggregations/{}/categoryHistograms/{}/{}/by_value/buckets",
                agg_name, cat_outer, cat_inner
            )
        } else {
            format!(
                "/aggregations/{}/{}/categoryHistograms/{}/{}/by_value/buckets",
                agg_name, x_container, cat_outer, cat_inner
            )
        };
        Some(path)
    }

    /// JSON pointer from a per-category bucket root to the inner x histogram
    /// buckets array.
    ///
    /// ```text
    /// attribute x: /histogram/by_attribute/by_key/{bucket_type}/buckets
    /// lineage   x: /histogram/by_lineage/at_rank/{bucket_type}/buckets
    /// root      x: /histogram/{bucket_type}/buckets
    /// ```
    pub fn inner_x_path(&self, bucket_type: &str) -> String {
        match self {
            FieldStorage::Attribute { .. } => {
                format!("/histogram/by_attribute/by_key/{}/buckets", bucket_type)
            }
            FieldStorage::Lineage { .. } => {
                format!("/histogram/by_lineage/at_rank/{}/buckets", bucket_type)
            }
            FieldStorage::Root { .. } => {
                format!("/histogram/{}/buckets", bucket_type)
            }
        }
    }
}

// ── Resolution ───────────────────────────────────────────────────────────────

/// Determine where `field` is stored, given its declared `value_type` and the
/// metadata cache.
///
/// Taxon ranks take priority over same-named attributes.  Unknown fields fall
/// back to a root-level field.
pub fn resolve_field_storage(
    field: &str,
    value_type: ValueType,
    cache: &Option<std::sync::Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<FieldStorage, String> {
    // Rank interpretation takes priority
    if matches!(value_type, ValueType::TaxonRank) || is_rank(field, cache) {
        return Ok(FieldStorage::Lineage {
            rank: field.to_string(),
        });
    }

    if is_attribute(field, cache) {
        let es_value_field = get_attribute_value_field(field, cache)?;
        return Ok(FieldStorage::Attribute {
            key: field.to_string(),
            es_value_field,
        });
    }

    // Root-level field — add .keyword suffix for keyword types
    let es_field = if matches!(value_type, ValueType::Keyword) {
        format!("{}.keyword", field)
    } else {
        field.to_string()
    };
    Ok(FieldStorage::Root { es_field })
}

// ── Low-level helpers (used internally and by bounds.rs / agg.rs) ─────────

/// Return `true` if `field` is a known taxonomic rank.
pub fn is_rank(
    field: &str,
    cache: &Option<std::sync::Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> bool {
    if let Some(lock) = cache {
        if let Ok(c) = lock.try_read() {
            return c.taxonomic_ranks.contains(&field.to_string());
        }
    }
    false
}

/// Return `true` if `field` is a nested attribute (and not a taxonomic rank).
pub fn is_attribute(
    field: &str,
    cache: &Option<std::sync::Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> bool {
    if let Some(lock) = cache {
        if let Ok(c) = lock.try_read() {
            if c.taxonomic_ranks.contains(&field.to_string()) {
                return false;
            }
            if let Value::Object(groups) = &c.attr_types {
                for (_, group) in groups {
                    if let Value::Object(fields) = group {
                        if fields.contains_key(field) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Return the fully-qualified ES field path for a nested attribute value.
///
/// Reads `processed_summary` from the metadata cache, e.g.
/// `"keyword_value.raw"` → `"attributes.keyword_value.raw"`.
pub fn get_attribute_value_field(
    field: &str,
    cache: &Option<std::sync::Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<String, String> {
    if let Some(lock) = cache {
        if let Ok(c) = lock.try_read() {
            if let Value::Object(groups) = &c.attr_types {
                for (_, group) in groups {
                    if let Value::Object(fields) = group {
                        if let Some(Value::Object(meta)) = fields.get(field) {
                            if let Some(ps) = meta.get("processed_summary").and_then(|v| v.as_str())
                            {
                                return Ok(format!("attributes.{}", ps));
                            }
                        }
                    }
                }
            }
        }
    }
    Err(format!("field '{}' not found in metadata", field))
}

// ── Agg block builders ───────────────────────────────────────────────────────

/// Wrap `inner_aggs` in the nested + filter envelope appropriate for this storage.
///
/// For `Attribute`/`Lineage` this emits:
/// ```json
/// { "nested": {"path":"..."}, "aggs": { "{container}": { "filter": {...}, "aggs": inner_aggs } } }
/// ```
/// For `Root` it returns `inner_aggs` unchanged (no nesting needed).
///
/// `container_name` is the name of the inner filter-agg key
/// (use [`FieldStorage::x_container_name`] or the cat container names).
pub fn wrap_in_nested(storage: &FieldStorage, container_name: &str, inner_aggs: Value) -> Value {
    match storage {
        FieldStorage::Root { .. } => inner_aggs,
        FieldStorage::Attribute { key, .. } => json!({
            "nested": { "path": "attributes" },
            "aggs": {
                container_name: {
                    "filter": { "term": { "attributes.key": key } },
                    "aggs": inner_aggs
                }
            }
        }),
        FieldStorage::Lineage { rank } => json!({
            "nested": { "path": "lineage" },
            "aggs": {
                container_name: {
                    "filter": { "term": { "lineage.taxon_rank": rank } },
                    "aggs": inner_aggs
                }
            }
        }),
    }
}

/// Wrap `inner_aggs` in the cat-level nested envelope (used inside
/// `categoryHistograms` reverse-nested context).
///
/// Uses `cat_wrapper_names()` to determine the outer wrapper and inner
/// container keys, keeping extraction paths deterministic.
pub fn wrap_cat_in_nested(storage: &FieldStorage, inner_aggs: Value) -> Value {
    let (outer, container) = storage.cat_wrapper_names();
    if outer.is_empty() {
        return inner_aggs;
    }
    match storage {
        FieldStorage::Attribute { key, .. } => json!({
            outer: {
                "nested": { "path": "attributes" },
                "aggs": {
                    container: {
                        "filter": { "term": { "attributes.key": key } },
                        "aggs": inner_aggs
                    }
                }
            }
        }),
        FieldStorage::Lineage { rank } => json!({
            outer: {
                "nested": { "path": "lineage" },
                "aggs": {
                    container: {
                        "filter": { "term": { "lineage.taxon_rank": rank } },
                        "aggs": inner_aggs
                    }
                }
            }
        }),
        FieldStorage::Root { .. } => inner_aggs,
    }
}

/// Build the inner x bucket agg block used inside each category bucket
/// (within `categoryHistograms`).
///
/// `bucket_params` is the **raw** aggregation params object (e.g. the
/// `{"field": …, "interval": …}` body for a histogram agg).  This function
/// wraps it in the required ES `{name: {type: params}}` nesting and then
/// adds the `reverse_nested` envelope with the correct nested path.
///
/// `sub_aggs` is an optional inner `"aggs"` object to attach to the x bucket
/// agg (used by scatter to add `yHistograms` inside each x-bucket).
///
/// Uses `"by_key"` / `"at_rank"` container names consistently so
/// [`FieldStorage::inner_x_path`] can compute the extraction path
/// deterministically.
pub fn build_inner_x_agg_block(
    x_storage: &FieldStorage,
    bucket_type: &str,
    bucket_params: Value,
    sub_aggs: Option<Value>,
) -> Value {
    // Build the named x bucket agg: {name: {type: raw_params[, "aggs": sub_aggs]}}
    let mut agg_body = json!({ bucket_type: bucket_params });
    if let Some(sa) = sub_aggs {
        agg_body["aggs"] = sa;
    }
    let inner_content = json!({ bucket_type: agg_body });
    match x_storage {
        FieldStorage::Root { .. } => json!({
            "histogram": {
                "reverse_nested": {},
                "aggs": inner_content
            }
        }),
        FieldStorage::Attribute { key, .. } => json!({
            "histogram": {
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "attributes" },
                        "aggs": {
                            "by_key": {
                                "filter": { "term": { "attributes.key": key } },
                                "aggs": inner_content
                            }
                        }
                    }
                }
            }
        }),
        FieldStorage::Lineage { rank } => json!({
            "histogram": {
                "reverse_nested": {},
                "aggs": {
                    "by_lineage": {
                        "nested": { "path": "lineage" },
                        "aggs": {
                            "at_rank": {
                                "filter": { "term": { "lineage.taxon_rank": rank } },
                                "aggs": inner_content
                            }
                        }
                    }
                }
            }
        }),
    }
}
