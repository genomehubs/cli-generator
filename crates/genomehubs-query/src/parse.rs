//! Response parsers for the genomehubs search API.
//!
//! Each parser accepts a raw JSON string, extracts the relevant fields, and
//! returns a well-typed struct or a JSON string suitable for FFI boundaries.
//!
//! All functions are pure — no I/O, no panics.  Error cases return a
//! descriptive string rather than propagating through `anyhow` or `thiserror`
//! so that both WASM (`wasm_bindgen`) and PyO3 callers get a plain string they
//! can surface directly to users.

use serde::Deserialize;

// ── ResponseStatus ────────────────────────────────────────────────────────────

/// The `status` block present in every genomehubs search/count API response.
///
/// ```json
/// { "status": { "hits": 42, "success": true, "error": null } }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseStatus {
    /// Total number of records matching the query.
    pub hits: u64,
    /// Whether the API reported success.
    pub ok: bool,
    /// Error message returned by the API, if any.
    pub error: Option<String>,
}

/// Minimal serde view of just the `status` block we need.
#[derive(Deserialize)]
struct ApiStatus {
    hits: Option<serde_json::Value>,
    success: Option<bool>,
    error: Option<serde_json::Value>,
}

/// Minimal serde view of the outer response envelope.
#[derive(Deserialize)]
struct ApiResponse {
    status: Option<ApiStatus>,
}

/// Parse the `status` block from a raw genomehubs API JSON response.
///
/// The `hits` field accepts both integer and string encodings (the API
/// occasionally returns `"42"` rather than `42`).
///
/// Returns `Ok(ResponseStatus)` on success.  The only failure case is
/// completely unparseable JSON — a missing or null `status` block is treated
/// as `{ hits: 0, ok: false, error: Some("missing status block") }` rather
/// than an error, because partial/error responses still contain useful context.
///
/// # Example
/// ```
/// use genomehubs_query::parse::parse_response_status;
///
/// let json = r#"{"status":{"hits":42,"success":true}}"#;
/// let s = parse_response_status(json).unwrap();
/// assert_eq!(s.hits, 42);
/// assert!(s.ok);
/// assert!(s.error.is_none());
/// ```
pub fn parse_response_status(raw: &str) -> Result<ResponseStatus, String> {
    let envelope: ApiResponse =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let status = match envelope.status {
        Some(s) => s,
        None => {
            return Ok(ResponseStatus {
                hits: 0,
                ok: false,
                error: Some("missing status block in API response".to_string()),
            });
        }
    };

    let hits = parse_hits(status.hits.as_ref());
    let ok = status.success.unwrap_or(false);
    let error = status.error.and_then(|v| match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    });

    Ok(ResponseStatus { hits, ok, error })
}

/// Coerce `hits` from either a JSON number or a JSON string to `u64`.
fn parse_hits(value: Option<&serde_json::Value>) -> u64 {
    match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Serialise a [`ResponseStatus`] to a compact JSON string for FFI boundaries.
///
/// Returns `{"hits":N,"ok":true|false,"error":null|"msg"}`.
pub fn response_status_to_json(status: &ResponseStatus) -> String {
    match &status.error {
        None => format!(
            r#"{{"hits":{},"ok":{},"error":null}}"#,
            status.hits, status.ok
        ),
        Some(msg) => {
            let escaped = msg.replace('\\', r"\\").replace('"', r#"\""#);
            format!(
                r#"{{"hits":{},"ok":{},"error":"{}"}}"#,
                status.hits, status.ok, escaped
            )
        }
    }
}

// ── parse_search_json ─────────────────────────────────────────────────────────

/// A flat row produced by [`parse_search_json`].
///
/// All optional columns use `serde_json::Value` so the output can be
/// serialised directly to a JSON array that Polars / R can read without
/// schema negotiation.  Absent values are `null`.
type FlatRow = serde_json::Map<String, serde_json::Value>;

/// Parse a raw genomehubs `/search` JSON response into a flat record array.
///
/// Each element of the returned array corresponds to one `results[i]` entry.
/// Fixed identity columns (`taxon_id`, `scientific_name`, …) are always
/// emitted.  For each attribute field:
///
/// - `{field}` — the representative value (`null` when the field is a stub
///   with no value, e.g. `{"sp_count": 1}`).
/// - `{field}_source` — normalised aggregation source: `"direct"`,
///   `"ancestor"`, `"descendant"`, or `null` for assembly/sample records.
/// - Stat sub-keys present on the raw field object — `{field}_min`,
///   `{field}_max`, `{field}_median`, `{field}_mode`, `{field}_mean`,
///   `{field}_from`, `{field}_to`, `{field}_count`, `{field}_sp_count`,
///   `{field}_length` — are each emitted only when present on the raw
///   object so that the schema reflects what the API actually returned.
///
/// The presence of `{field}_source` makes the full-information form: users
/// who want a simple table can drop any `*_source` column with one Polars /
/// R expression.  Use [`annotate_source_labels`] or [`split_source_columns`]
/// to reshape the parsed output without re-parsing.
///
/// Returns a compact JSON array string on success.
pub fn parse_search_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let results = match envelope.get("results").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Ok("[]".to_string()),
    };

    let rows: Vec<serde_json::Value> = results
        .iter()
        .map(|entry| {
            let result = entry.get("result").unwrap_or(&serde_json::Value::Null);
            let index = entry
                .get("index")
                .and_then(|v| v.as_str())
                .unwrap_or("taxon");
            serde_json::Value::Object(flatten_result(result, index))
        })
        .collect();

    serde_json::to_string(&rows).map_err(|e| format!("serialisation error: {e}"))
}

/// Flatten one `result` object into a single row map.
fn flatten_result(result: &serde_json::Value, index: &str) -> FlatRow {
    let mut row: FlatRow = serde_json::Map::new();

    // ── Identity columns ─────────────────────────────────────────────────────
    // Emit whichever of the three ID fields is present.
    for id_key in &["taxon_id", "assembly_id", "sample_id"] {
        if let Some(v) = result.get(*id_key) {
            row.insert(id_key.to_string(), v.clone());
        }
    }
    for col in &["scientific_name", "taxon_rank"] {
        if let Some(v) = result.get(*col) {
            row.insert(col.to_string(), v.clone());
        }
    }

    // ── Attribute fields ─────────────────────────────────────────────────────
    if let Some(fields) = result.get("fields").and_then(|v| v.as_object()) {
        let is_taxon = index.starts_with("taxon");
        for (name, field_val) in fields {
            // The API returns "field:modifier" keys (e.g. "assembly_span:min") when
            // a specific summary was requested.  Store just the scalar value as
            // "{field}__{modifier}" and skip all sub-key metadata — the sub-keys
            // are already present on the bare "assembly_span" entry.
            if let Some((bare, modifier)) = name.split_once(':') {
                let value = field_val
                    .as_object()
                    .and_then(|o| o.get("value"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                row.insert(format!("{bare}__{modifier}"), value);
                continue;
            }
            flatten_field(name, field_val, is_taxon, &mut row);
        }
    }

    row
}

/// Flatten one attribute field object into `row`, prefixing with `name`.
///
/// Stat sub-keys are only written when present on the raw object.
fn flatten_field(name: &str, field_val: &serde_json::Value, is_taxon: bool, row: &mut FlatRow) {
    let obj = match field_val.as_object() {
        Some(o) => o,
        None => {
            row.insert(name.to_string(), serde_json::Value::Null);
            return;
        }
    };

    // Stub field — only sp_count present, no value.
    let value = obj.get("value").cloned().unwrap_or(serde_json::Value::Null);

    // Normalise list `value`: single string → one-element array when length > 1
    // would be inconsistent, so leave as-is and let callers handle both forms.
    row.insert(name.to_string(), value);

    if !is_taxon {
        // Assembly/sample: only emit length for list fields, nothing else.
        if let Some(len) = obj.get("length") {
            row.insert(format!("{name}__length"), len.clone());
        }
        return;
    }

    // ── Taxon-only metadata ───────────────────────────────────────────────────

    // aggregation_source: normalise string/array to a plain string.
    let source = normalize_source(obj.get("aggregation_source"));
    row.insert(
        format!("{name}__source"),
        source
            .as_deref()
            .map(|s| serde_json::Value::String(s.to_string()))
            .unwrap_or(serde_json::Value::Null),
    );

    // Numeric stat sub-keys — emit only when present.
    for stat in &["min", "max", "median", "mode", "mean", "count", "sp_count"] {
        if let Some(v) = obj.get(*stat) {
            row.insert(format!("{name}__{stat}"), v.clone());
        }
    }

    // Date range keys.
    for key in &["from", "to"] {
        if let Some(v) = obj.get(*key) {
            row.insert(format!("{name}__{key}"), v.clone());
        }
    }

    // List length.
    if let Some(len) = obj.get("length") {
        row.insert(format!("{name}__length"), len.clone());
    }
}

/// Normalise `aggregation_source` to a plain string.
///
/// The API sends `"direct"` (a bare string) for directly measured values and
/// `["ancestor"]` or `["descendant"]` (single-element arrays) for estimated
/// values.  Returns `None` when the key is absent.
fn normalize_source(val: Option<&serde_json::Value>) -> Option<String> {
    match val? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.first().and_then(|v| v.as_str()).map(str::to_string),
        _ => None,
    }
}

// ── annotate_source_labels ────────────────────────────────────────────────────

/// Controls which values receive a source annotation label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelMode {
    /// Annotate all values: `"3.4"`, `"3.4 (Descendant)"`, `"3.4 (Ancestral)"`.
    All,
    /// Annotate only non-direct values: direct stays bare, others are labelled.
    NonDirect,
    /// Annotate only ancestral values; direct and descendant stay bare.
    AncestralOnly,
}

impl LabelMode {
    fn from_str(s: &str) -> Self {
        match s {
            "all" => Self::All,
            "ancestral_only" => Self::AncestralOnly,
            _ => Self::NonDirect,
        }
    }
}

/// Add `{field}__label` columns to already-flat parsed records.
///
/// Accepts the JSON array produced by [`parse_search_json`] and a mode string
/// (`"all"`, `"non_direct"`, or `"ancestral_only"`).  For each `{field}__source`
/// column present in the records, adds a `{field}__label` column whose value is
/// the field value formatted as a string with an optional source annotation:
///
/// | source     | non_direct mode         | all mode                |
/// |------------|-------------------------|-------------------------|
/// | direct     | `"3.4"`                 | `"3.4"`                 |
/// | descendant | `"57 (Descendant)"`     | `"57 (Descendant)"`     |
/// | ancestor   | `"3.4 (Ancestral)"`     | `"3.4 (Ancestral)"`     |
///
/// This function operates on the already-flat JSON string — it does not
/// re-parse the raw HTTP response.
pub fn annotate_source_labels(records_json: &str, mode: &str) -> Result<String, String> {
    let mut records: Vec<serde_json::Value> =
        serde_json::from_str(records_json).map_err(|e| format!("invalid records JSON: {e}"))?;

    let label_mode = LabelMode::from_str(mode);

    // Collect all field names that have a _source companion (scan first record).
    let source_fields: Vec<String> = records
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| {
            obj.keys()
                .filter(|k| k.ends_with("__source"))
                .map(|k| k[..k.len() - 8].to_string())
                .collect()
        })
        .unwrap_or_default();

    for record in &mut records {
        let obj = match record.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        for field in &source_fields {
            let source_key = format!("{field}__source");
            let source = obj
                .get(&source_key)
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let should_annotate = match (label_mode, source.as_deref()) {
                (LabelMode::All, _) => true,
                (LabelMode::NonDirect, Some("direct") | None) => false,
                (LabelMode::NonDirect, _) => true,
                (LabelMode::AncestralOnly, Some("ancestor")) => true,
                (LabelMode::AncestralOnly, _) => false,
            };

            if !should_annotate {
                continue;
            }

            let raw_val = obj.get(field).cloned().unwrap_or(serde_json::Value::Null);
            let label = build_label(&raw_val, source.as_deref());
            obj.insert(format!("{field}__label"), serde_json::Value::String(label));
        }
    }

    serde_json::to_string(&records).map_err(|e| format!("serialisation error: {e}"))
}

/// Format a value and source string into a display label.
fn build_label(value: &serde_json::Value, source: Option<&str>) -> String {
    let val_str = match value {
        serde_json::Value::Null => return String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .map(|v| v.as_str().unwrap_or("?").to_string())
                .collect();
            parts.join(", ")
        }
        other => other.to_string(),
    };

    match source {
        Some("ancestor") => format!("{val_str} (Ancestral)"),
        Some("descendant") => format!("{val_str} (Descendant)"),
        _ => val_str,
    }
}

// ── split_source_columns ──────────────────────────────────────────────────────

/// Reshape already-flat parsed records into split-source columns.
///
/// For every `{field}` / `{field}__source` pair in the records, produces:
/// - `{field}__direct`     — value when `{field}__source == "direct"`, else null
/// - `{field}__descendant` — value when `{field}__source == "descendant"`, else null
/// - `{field}__ancestral`  — value when `{field}__source == "ancestor"`, else null
///
/// The original `{field}` and `{field}__source` columns are removed.  All other
/// columns (stat sub-keys, `__length`, identity fields) are kept unchanged.
///
/// Useful for analyses where direct and derived values must be treated
/// separately — load the output two-frame join on `taxon_id`.
pub fn split_source_columns(records_json: &str) -> Result<String, String> {
    let mut records: Vec<serde_json::Value> =
        serde_json::from_str(records_json).map_err(|e| format!("invalid records JSON: {e}"))?;

    let source_fields: Vec<String> = records
        .first()
        .and_then(|r| r.as_object())
        .map(|obj| {
            obj.keys()
                .filter(|k| k.ends_with("__source"))
                .map(|k| k[..k.len() - 8].to_string())
                .collect()
        })
        .unwrap_or_default();

    for record in &mut records {
        let obj = match record.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        for field in &source_fields {
            let source_key = format!("{field}__source");
            let source = obj
                .get(&source_key)
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let value = obj
                .remove(field.as_str())
                .unwrap_or(serde_json::Value::Null);
            obj.remove(&source_key);

            let null = serde_json::Value::Null;
            obj.insert(
                format!("{field}__direct"),
                if source.as_deref() == Some("direct") {
                    value.clone()
                } else {
                    null.clone()
                },
            );
            obj.insert(
                format!("{field}__descendant"),
                if source.as_deref() == Some("descendant") {
                    value.clone()
                } else {
                    null.clone()
                },
            );
            obj.insert(
                format!("{field}__ancestral"),
                if source.as_deref() == Some("ancestor") {
                    value
                } else {
                    null
                },
            );
        }
    }

    serde_json::to_string(&records).map_err(|e| format!("serialisation error: {e}"))
}

// ── values_only ────────────────────────────────────────────────────────────────

/// Parse a JSON array of `__*` column names that must be preserved by [`values_only`]
/// and [`annotated_values`].
///
/// Returns an empty set when the input is blank, `"[]"`, or unparseable so
/// that the functions degrade gracefully to stripping all `__*` columns.
fn parse_keep_columns(keep_columns_json: &str) -> std::collections::HashSet<String> {
    if keep_columns_json.is_empty() || keep_columns_json == "[]" {
        return std::collections::HashSet::new();
    }
    serde_json::from_str::<Vec<String>>(keep_columns_json)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Strip `__*` sub-key columns from `records`, keeping any listed in `keep`.
fn apply_values_filter(
    records: Vec<serde_json::Value>,
    keep: &std::collections::HashSet<String>,
) -> Vec<serde_json::Value> {
    records
        .into_iter()
        .map(|record| {
            let serde_json::Value::Object(obj) = record else {
                return serde_json::Value::Null;
            };
            let filtered: serde_json::Map<String, serde_json::Value> = obj
                .into_iter()
                .filter(|(k, _)| !k.contains("__") || keep.contains(k.as_str()))
                .collect();
            serde_json::Value::Object(filtered)
        })
        .collect()
}

/// Strip all metadata columns from already-flat records, keeping only identity
/// columns and bare field values.
///
/// Any column whose name contains `__` is removed (i.e. `{field}__source`,
/// `{field}__min`, `{field}__label`, `{field}__direct`, etc.) **unless** it is
/// listed in `keep_columns_json`.
///
/// `keep_columns_json` is a JSON array of column names that should be preserved
/// despite containing `__`, e.g. `'["assembly_span__min"]'`.  Pass `""` or
/// `"[]"` to strip all `__*` columns (the original behaviour).
///
/// This is useful when the caller requested a specific summary statistic via
/// `field:modifier` syntax (e.g. `addField("assembly_span:min")`) and wants
/// the corresponding `assembly_span__min` column in the output.
pub fn values_only(records_json: &str, keep_columns_json: &str) -> Result<String, String> {
    let records: Vec<serde_json::Value> =
        serde_json::from_str(records_json).map_err(|e| format!("invalid records JSON: {e}"))?;

    let keep = parse_keep_columns(keep_columns_json);
    let stripped = apply_values_filter(records, &keep);

    serde_json::to_string(&stripped).map_err(|e| format!("serialisation error: {e}"))
}

/// Return records with each field value replaced by its display label.
///
/// Combines [`annotate_source_labels`] and [`values_only`] in one step.
/// For each `{field}__label` produced by annotation, the label value is moved
/// into the `{field}` column.  All other `__*` metadata columns are then
/// removed, unless listed in `keep_columns_json`.
///
/// `keep_columns_json` works identically to [`values_only`] — pass `""` or
/// `"[]"` to strip all remaining `__*` columns after label promotion, or pass
/// a JSON array of column names to preserve specific stat sub-keys.
///
/// Fields that have no label (e.g. because they had source `"direct"` in
/// `"non_direct"` mode) keep their original numeric/string value.
///
/// # Example
/// ```
/// // genome_size = 8215200000, source = "ancestor"
/// // → genome_size = "8215200000 (Ancestral)"
/// ```
pub fn annotated_values(
    records_json: &str,
    mode: &str,
    keep_columns_json: &str,
) -> Result<String, String> {
    let labelled_json = annotate_source_labels(records_json, mode)?;
    let mut records: Vec<serde_json::Value> =
        serde_json::from_str(&labelled_json).map_err(|e| format!("invalid JSON: {e}"))?;

    for record in &mut records {
        let obj = match record.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        // Collect which fields have a __label to promote.
        let label_fields: Vec<String> = obj
            .keys()
            .filter(|k| k.ends_with("__label"))
            .map(|k| k[..k.len() - 7].to_string())
            .collect();

        for field in label_fields {
            let label_key = format!("{field}__label");
            if let Some(label) = obj.remove(&label_key) {
                obj.insert(field, label);
            }
        }
    }

    let keep = parse_keep_columns(keep_columns_json);
    let stripped = apply_values_filter(records, &keep);

    serde_json::to_string(&stripped).map_err(|e| format!("serialisation error: {e}"))
}

// ── to_tidy_records ───────────────────────────────────────────────────────────

/// The fixed identity columns that are carried through unchanged into every tidy row.
const IDENTITY_COLUMNS: &[&str] = &[
    "taxon_id",
    "assembly_id",
    "sample_id",
    "scientific_name",
    "taxon_rank",
];

/// Reshape already-flat records into long/tidy format.
///
/// Accepts the JSON array produced by [`parse_search_json`] and emits one row
/// per *field* per *original record*.  Each output row contains:
///
/// - All identity columns present in the source record (`taxon_id`, `scientific_name`, …).
/// - `"field"` — the bare field name (e.g. `"genome_size"`).
/// - `"value"` — the representative value for that field.
/// - `"source"` — the aggregation source (`"direct"`, `"ancestor"`, `"descendant"`, or `null`).
///
/// Columns with `__` in their name (stat sub-keys, labels, split columns) are
/// consumed when they belong to a field row but are **not** emitted as separate
/// rows — only the bare field entries are pivoted.  Explicitly-requested
/// modifier columns (e.g. `genome_size__direct`, `assembly_span__min` from
/// `field:modifier` requests) are emitted as their own tidy rows with
/// `"source": null`.
///
/// This matches the shape of the GoaT API's tidy TSV format and is the natural
/// input for R's `tidyverse` / Python's `pandas.melt`.
///
/// # Example
/// ```
/// // Flat input:
/// // {"taxon_id":"9606","genome_size":3100000000,"genome_size__source":"direct"}
/// //
/// // Tidy output:
/// // {"taxon_id":"9606","field":"genome_size","value":3100000000,"source":"direct"}
/// ```
pub fn to_tidy_records(records_json: &str) -> Result<String, String> {
    let records: Vec<serde_json::Value> =
        serde_json::from_str(records_json).map_err(|e| format!("invalid records JSON: {e}"))?;

    let mut tidy_rows: Vec<serde_json::Value> = Vec::new();

    for record in &records {
        let obj = match record.as_object() {
            Some(o) => o,
            None => continue,
        };

        // Build the identity portion shared by every tidy row from this record.
        let mut identity: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for col in IDENTITY_COLUMNS {
            if let Some(v) = obj.get(*col) {
                identity.insert(col.to_string(), v.clone());
            }
        }

        // Collect bare field names: columns that have no `__` and are not identity columns.
        let bare_fields: Vec<&str> = obj
            .keys()
            .filter(|k| !k.contains("__") && !IDENTITY_COLUMNS.contains(&k.as_str()))
            .map(String::as_str)
            .collect();

        // Emit one tidy row per bare field.
        for field_name in bare_fields {
            let value = obj[field_name].clone();
            let source_key = format!("{field_name}__source");
            let source = obj
                .get(&source_key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let mut row = identity.clone();
            row.insert(
                "field".to_string(),
                serde_json::Value::String(field_name.to_string()),
            );
            row.insert("value".to_string(), value);
            row.insert("source".to_string(), source);
            tidy_rows.push(serde_json::Value::Object(row));
        }

        // Emit one tidy row per explicitly-requested modifier column (e.g. genome_size__direct,
        // assembly_span__min).  These are columns that contain `__` but were produced by a
        // `field:modifier` request, not by the automatic sub-key flattening.  We distinguish
        // them from auto sub-keys by checking whether the portion after `__` is a known stat
        // sub-key or automatic metadata key — those are skipped.
        const AUTO_SUBKEYS: &[&str] = &[
            "source",
            "min",
            "max",
            "median",
            "mode",
            "mean",
            "count",
            "sp_count",
            "from",
            "to",
            "length",
            "label",
            "direct",
            "descendant",
            "ancestral",
        ];
        for key in obj.keys() {
            let Some((bare, modifier)) = key.split_once("__") else {
                continue;
            };
            if IDENTITY_COLUMNS.contains(&bare) {
                continue;
            }
            // Skip auto-generated sub-keys; only emit user-requested modifier columns.
            // A user-requested modifier column is one where the `bare` field IS present
            // as a standalone column in the record (i.e. it came from a `field:modifier`
            // API request rather than the standard flattening pipeline).
            if AUTO_SUBKEYS.contains(&modifier) && obj.contains_key(bare) {
                continue;
            }
            let value = obj[key].clone();
            let mut row = identity.clone();
            row.insert(
                "field".to_string(),
                serde_json::Value::String(format!("{bare}:{modifier}")),
            );
            row.insert("value".to_string(), value);
            row.insert("source".to_string(), serde_json::Value::Null);
            tidy_rows.push(serde_json::Value::Object(row));
        }
    }

    serde_json::to_string(&tidy_rows).map_err(|e| format!("serialisation error: {e}"))
}

// ── parse_paginated_json ──────────────────────────────────────────────────────

/// The result of parsing one page from `/searchPaginated`.
///
/// Callers use this to drive a fetch loop:
/// - append [`records`] to the accumulator
/// - if [`has_more`] is `true`, set the next request's `search_after` to [`next_cursor`]
/// - stop when [`has_more`] is `false`
pub struct PaginatedPage {
    /// Flat records parsed by the same rules as [`parse_search_json`].
    pub records: Vec<serde_json::Value>,
    /// Whether the API has more pages after this one.
    pub has_more: bool,
    /// Cursor to pass as `searchAfter` on the next request.
    /// `None` when [`has_more`] is `false`.
    pub next_cursor: Option<Vec<serde_json::Value>>,
    /// Total hits reported by the `status` block of this response.
    pub total_hits: u64,
}

/// Minimal serde view of the `/searchPaginated` envelope.
///
/// ```json
/// {
///   "status": {"hits": 5000, "success": true},
///   "hits": [...],
///   "pagination": {"limit": 1000, "count": 1000, "hasMore": true, "searchAfter": [...]}
/// }
/// ```
#[derive(Deserialize)]
struct PaginatedApiResponse {
    status: Option<ApiStatus>,
    hits: Option<Vec<serde_json::Value>>,
    pagination: Option<PaginationBlock>,
}

#[derive(Deserialize)]
struct PaginationBlock {
    #[serde(rename = "hasMore")]
    has_more: bool,
    #[serde(rename = "searchAfter")]
    search_after: Option<Vec<serde_json::Value>>,
}

/// Parse one page of a `/searchPaginated` response.
///
/// Accepts the raw JSON string returned by the API and returns a
/// [`PaginatedPage`] containing the flat records (parsed with the same rules as
/// [`parse_search_json`]), the `hasMore` flag, the next cursor, and the total
/// hit count.
///
/// Use this to drive a pagination loop:
///
/// ```rust,ignore
/// let mut all_records = Vec::new();
/// let mut cursor: Option<Vec<serde_json::Value>> = None;
/// loop {
///     params.search_after = cursor.clone();
///     let raw = http_get(build_query_url(&query, &params, base, version, "searchPaginated"));
///     let page = parse_paginated_json(&raw)?;
///     all_records.extend(page.records);
///     if !page.has_more { break; }
///     cursor = page.next_cursor;
/// }
/// ```
pub fn parse_paginated_json(raw: &str) -> Result<PaginatedPage, String> {
    let envelope: PaginatedApiResponse =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let total_hits = envelope
        .status
        .as_ref()
        .map(|s| parse_hits(s.hits.as_ref()))
        .unwrap_or(0);

    // Re-use the same per-result flattening logic as parse_search_json.
    // The /searchPaginated envelope uses "hits" (not "results") for the records,
    // but each element has the same {index, id, score, result} shape.
    let raw_hits = envelope.hits.unwrap_or_default();
    let mut records: Vec<serde_json::Value> = Vec::with_capacity(raw_hits.len());
    for hit in &raw_hits {
        let result = hit.get("result").unwrap_or(&serde_json::Value::Null);
        let index = hit.get("index").and_then(|v| v.as_str()).unwrap_or("taxon");
        let row = flatten_result(result, index);
        if !row.is_empty() {
            records.push(serde_json::Value::Object(row));
        }
    }

    let (has_more, next_cursor) = match envelope.pagination {
        Some(p) => (p.has_more, p.search_after),
        None => (false, None),
    };

    Ok(PaginatedPage {
        records,
        has_more,
        next_cursor,
        total_hits,
    })
}

/// Serialise a [`PaginatedPage`] into a JSON object for FFI callers.
///
/// ```json
/// {
///   "records": [...],
///   "hasMore": true,
///   "searchAfter": [...],
///   "totalHits": 5000
/// }
/// ```
pub fn paginated_page_to_json(page: &PaginatedPage) -> String {
    let search_after_val = page
        .next_cursor
        .as_ref()
        .map(|c| serde_json::Value::Array(c.clone()))
        .unwrap_or(serde_json::Value::Null);

    let obj = serde_json::json!({
        "records": page.records,
        "hasMore": page.has_more,
        "searchAfter": search_after_val,
        "totalHits": page.total_hits,
    });
    obj.to_string()
}

// ── parse_msearch_json ────────────────────────────────────────────────────────

/// Flat records + metadata for one query within an msearch response.
pub struct MsearchQueryResult {
    /// Flat records parsed by the same rules as [`parse_search_json`].
    pub records: Vec<serde_json::Value>,
    /// Total hits reported by the API for this query (may exceed `records.len()`).
    pub total: u64,
    /// API error message for this query, if any.
    pub error: Option<String>,
}

/// The result of parsing a full `/msearch` response.
pub struct MsearchResult {
    /// Per-query results, in the same order as the request's `searches` array.
    pub results: Vec<MsearchQueryResult>,
    /// Sum of all per-query totals from the outer `status.hits` field.
    pub total_hits: u64,
}

#[derive(Deserialize)]
struct MsearchApiResponse {
    status: Option<ApiStatus>,
    results: Option<Vec<MsearchApiQueryResult>>,
}

#[derive(Deserialize)]
struct MsearchApiQueryResult {
    total: Option<serde_json::Value>,
    hits: Option<Vec<serde_json::Value>>,
    error: Option<String>,
}

/// Parse a raw `/msearch` response into per-query flat record lists.
///
/// Each element of [`MsearchResult::results`] corresponds to one entry in the
/// `searches` array of the request.  Records are flattened with the same rules
/// as [`parse_search_json`].
///
/// Returns `Err` only on completely unparseable JSON.
pub fn parse_msearch_json(raw: &str) -> Result<MsearchResult, String> {
    let envelope: MsearchApiResponse =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let total_hits = envelope
        .status
        .as_ref()
        .map(|s| parse_hits(s.hits.as_ref()))
        .unwrap_or(0);

    let raw_results = envelope.results.unwrap_or_default();
    let mut results = Vec::with_capacity(raw_results.len());

    for query_result in &raw_results {
        let raw_hits = query_result.hits.as_deref().unwrap_or(&[]);
        let mut records = Vec::with_capacity(raw_hits.len());
        for hit in raw_hits {
            let hit_result = hit.get("result").unwrap_or(&serde_json::Value::Null);
            let index = hit.get("index").and_then(|v| v.as_str()).unwrap_or("taxon");
            let flat = flatten_result(hit_result, index);
            records.push(serde_json::Value::Object(flat));
        }
        let total = parse_hits(query_result.total.as_ref());
        results.push(MsearchQueryResult {
            records,
            total,
            error: query_result.error.clone(),
        });
    }

    Ok(MsearchResult {
        results,
        total_hits,
    })
}

/// Serialise an [`MsearchResult`] into a JSON object for FFI callers.
///
/// ```json
/// {
///   "results": [
///     {"records": [...], "total": 5200, "error": null},
///     {"records": [...], "total": 7300, "error": null}
///   ],
///   "totalHits": 12500
/// }
/// ```
pub fn msearch_result_to_json(result: &MsearchResult) -> String {
    let results_json: Vec<serde_json::Value> = result
        .results
        .iter()
        .map(|r| {
            serde_json::json!({
                "records": r.records,
                "total": r.total,
                "error": r.error,
            })
        })
        .collect();

    serde_json::json!({
        "results": results_json,
        "totalHits": result.total_hits,
    })
    .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer_hits() {
        let json = r#"{"status":{"hits":42,"success":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 42);
        assert!(s.ok);
        assert!(s.error.is_none());
    }

    #[test]
    fn parses_string_hits() {
        let json = r#"{"status":{"hits":"123","success":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 123);
    }

    #[test]
    fn zero_hits_on_null_hits() {
        let json = r#"{"status":{"hits":null,"success":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
    }

    #[test]
    fn missing_status_block() {
        let json = r#"{"results":[]}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
        assert!(!s.ok);
        assert!(s.error.is_some());
    }

    #[test]
    fn captures_api_error() {
        let json = r#"{"status":{"hits":0,"success":false,"error":"query parse error"}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
        assert!(!s.ok);
        assert_eq!(s.error.as_deref(), Some("query parse error"));
    }

    #[test]
    fn invalid_json_returns_err() {
        assert!(parse_response_status("not json").is_err());
    }

    #[test]
    fn to_json_round_trips() {
        let status = ResponseStatus {
            hits: 5,
            ok: true,
            error: None,
        };
        let json = response_status_to_json(&status);
        // response_status_to_json produces the inner status object (for FFI).
        // Verify the serialised form has the correct fields.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["hits"], 5);
        assert_eq!(v["ok"], true);
        assert!(v["error"].is_null());
    }

    #[test]
    fn to_json_round_trips_with_error() {
        let status = ResponseStatus {
            hits: 0,
            ok: false,
            error: Some("bad request".to_string()),
        };
        let json = response_status_to_json(&status);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["error"], "bad request");
    }

    // ── parse_search_json ─────────────────────────────────────────────────────

    // Minimal response fixture shared across several tests.
    fn taxon_response(fields_json: &str) -> String {
        format!(
            r#"{{"status":{{"hits":1,"success":true}},
              "results":[{{"index":"taxon--ncbi--goat--2026.04.16","id":"9606","score":1.0,
                "result":{{"taxon_id":"9606","scientific_name":"Homo sapiens",
                  "taxon_rank":"species","fields":{fields_json}}}}}]}}"#
        )
    }

    fn assembly_response(fields_json: &str) -> String {
        format!(
            r#"{{"status":{{"hits":1,"success":true}},
              "results":[{{"index":"assembly","id":"GCA_1","score":1.0,
                "result":{{"assembly_id":"GCA_1","scientific_name":"Homo sapiens",
                  "taxon_id":"9606","taxon_rank":"species","fields":{fields_json}}}}}]}}"#
        )
    }

    #[test]
    fn empty_results_returns_empty_array() {
        let raw = r#"{"status":{"hits":0,"success":true},"results":[]}"#;
        let out = parse_search_json(raw).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn missing_results_key_returns_empty_array() {
        let raw = r#"{"status":{"hits":0,"success":true}}"#;
        let out = parse_search_json(raw).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn parses_taxon_identity_columns() {
        let raw = taxon_response("{}");
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        assert_eq!(rows[0]["taxon_id"], "9606");
        assert_eq!(rows[0]["scientific_name"], "Homo sapiens");
        assert_eq!(rows[0]["taxon_rank"], "species");
    }

    #[test]
    fn parses_taxon_numeric_direct_field() {
        let raw = taxon_response(
            r#"{"genome_size":{"value":3423000000,"count":1,"min":3423000000,"max":3423000000,"median":3423000000,"aggregation_method":"primary","aggregation_source":"direct","sp_count":0}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];
        assert_eq!(row["genome_size"], 3423000000_u64);
        assert_eq!(row["genome_size__source"], "direct");
        assert_eq!(row["genome_size__min"], 3423000000_u64);
        assert_eq!(row["genome_size__max"], 3423000000_u64);
        assert_eq!(row["genome_size__median"], 3423000000_u64);
        assert_eq!(row["genome_size__count"], 1);
        assert_eq!(row["genome_size__sp_count"], 0);
    }

    #[test]
    fn parses_taxon_ancestor_source_as_string() {
        let raw = taxon_response(
            r#"{"ploidy":{"value":2,"count":1,"min":2,"max":2,"median":2,"mode":2,"aggregation_method":"mode","aggregation_source":["ancestor"],"aggregation_rank":"clade","aggregation_taxon_id":"9347"}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];
        assert_eq!(row["ploidy"], 2);
        assert_eq!(row["ploidy__source"], "ancestor");
        // sp_count absent on ancestor-derived fields
        assert!(row.get("ploidy__sp_count").is_none());
    }

    #[test]
    fn parses_taxon_descendant_source_as_string() {
        let raw = taxon_response(
            r#"{"mitochondrion_gc_percent":{"value":45.5,"count":2,"median":45.5,"aggregation_method":"median","aggregation_source":["descendant"],"sp_count":2}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        assert_eq!(rows[0]["mitochondrion_gc_percent__source"], "descendant");
        assert_eq!(rows[0]["mitochondrion_gc_percent__sp_count"], 2);
    }

    #[test]
    fn parses_taxon_date_field_with_from_to() {
        let raw = taxon_response(
            r#"{"ebp_standard_date":{"value":"2004-09-01","from":"2004-09-01T00:00:00.000Z","to":"2026-03-25T00:00:00.000Z","count":548,"aggregation_method":"min","aggregation_source":"direct","sp_count":0}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];
        assert_eq!(row["ebp_standard_date"], "2004-09-01");
        assert_eq!(row["ebp_standard_date__from"], "2004-09-01T00:00:00.000Z");
        assert_eq!(row["ebp_standard_date__to"], "2026-03-25T00:00:00.000Z");
        assert!(row.get("ebp_standard_date__min").is_none());
    }

    #[test]
    fn colon_modifier_field_stored_as_double_underscore_key() {
        // The API returns "assembly_span:min" as a separate field when the caller
        // requested that specific summary.  We must store it as assembly_span__min
        // (the value only) and NOT emit sub-keys like assembly_span:min__source.
        let raw = taxon_response(
            r#"{"assembly_span":{"value":3843982861,"count":5,"min":3843982861,"max":4288896762,"aggregation_source":["ancestor"]},
               "assembly_span:min":{"value":3843982861,"count":1,"aggregation_source":["ancestor"]}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];

        // Bare field still fully parsed.
        assert_eq!(row["assembly_span"], 3843982861_u64);
        assert_eq!(row["assembly_span__source"], "ancestor");
        assert_eq!(row["assembly_span__min"], 3843982861_u64);

        // The literal "assembly_span:min" key must NOT appear.
        assert!(row.get("assembly_span:min").is_none());
        // No sub-keys for the :min entry itself.
        assert!(row.get("assembly_span:min__source").is_none());
        assert!(row.get("assembly_span:min__count").is_none());
    }

    #[test]
    fn parses_taxon_list_field_with_length() {
        let raw = taxon_response(
            r#"{"bioproject":{"value":["PRJDB1","PRJDB2"],"count":2,"length":2176,"aggregation_method":"list","aggregation_source":"direct","sp_count":1,"has_descendants":true}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];
        assert_eq!(row["bioproject"][0], "PRJDB1");
        assert_eq!(row["bioproject__length"], 2176);
        assert_eq!(row["bioproject__source"], "direct");
    }

    #[test]
    fn stub_field_emits_null_value() {
        // Only sp_count present — no value key.
        let raw = taxon_response(r#"{"sequencing_status_ebp":{"sp_count":1}}"#);
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        assert!(rows[0]["sequencing_status_ebp"].is_null());
        assert_eq!(rows[0]["sequencing_status_ebp__sp_count"], 1);
    }

    #[test]
    fn assembly_fields_have_no_source_column() {
        let raw = assembly_response(
            r#"{"assembly_span":{"value":3088210890,"count":1},"bioproject":{"value":"PRJNA323611","count":1,"length":1}}"#,
        );
        let rows: serde_json::Value =
            serde_json::from_str(&parse_search_json(&raw).unwrap()).unwrap();
        let row = &rows[0];
        assert_eq!(row["assembly_span"], 3088210890_u64);
        // No source column for assembly index.
        assert!(row.get("assembly_span__source").is_none());
        // length still emitted for list fields.
        assert_eq!(row["bioproject__length"], 1);
    }

    // ── annotate_source_labels ────────────────────────────────────────────────

    fn flat_records(field: &str, value: &str, source: &str) -> String {
        format!(r#"[{{"{field}":{value},"{field}__source":"{source}"}}]"#)
    }

    #[test]
    fn annotate_direct_non_direct_mode_no_label() {
        let records = flat_records("genome_size", "3423000000", "direct");
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(&records, "non_direct").unwrap()).unwrap();
        assert!(out[0].get("genome_size__label").is_none());
    }

    #[test]
    fn annotate_ancestor_non_direct_mode_adds_label() {
        let records = flat_records("ploidy", "2", "ancestor");
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(&records, "non_direct").unwrap()).unwrap();
        assert_eq!(out[0]["ploidy__label"], "2 (Ancestral)");
    }

    #[test]
    fn annotate_descendant_adds_label() {
        let records = flat_records("mitochondrion_gc_percent", "45.5", "descendant");
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(&records, "non_direct").unwrap()).unwrap();
        assert_eq!(
            out[0]["mitochondrion_gc_percent__label"],
            "45.5 (Descendant)"
        );
    }

    #[test]
    fn annotate_all_mode_labels_direct_too() {
        let records = flat_records("genome_size", "3423000000", "direct");
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(&records, "all").unwrap()).unwrap();
        assert_eq!(out[0]["genome_size__label"], "3423000000");
    }

    #[test]
    fn annotate_ancestral_only_mode_skips_descendant() {
        let records = flat_records("mitochondrion_gc_percent", "45.5", "descendant");
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(&records, "ancestral_only").unwrap())
                .unwrap();
        assert!(out[0].get("mitochondrion_gc_percent__label").is_none());
    }

    #[test]
    fn annotate_list_value_joins_with_comma() {
        let records = r#"[{"bioproject":["PRJNA1","PRJNA2"],"bioproject__source":"direct"}]"#;
        let out: serde_json::Value =
            serde_json::from_str(&annotate_source_labels(records, "all").unwrap()).unwrap();
        assert_eq!(out[0]["bioproject__label"], "PRJNA1, PRJNA2");
    }

    // ── split_source_columns ──────────────────────────────────────────────────

    #[test]
    fn split_direct_value_goes_to_direct_column() {
        let records = flat_records("genome_size", "3423000000", "direct");
        let out: serde_json::Value =
            serde_json::from_str(&split_source_columns(&records).unwrap()).unwrap();
        let row = &out[0];
        assert_eq!(row["genome_size__direct"], 3423000000_u64);
        assert!(row["genome_size__descendant"].is_null());
        assert!(row["genome_size__ancestral"].is_null());
        // Original columns removed.
        assert!(row.get("genome_size").is_none());
        assert!(row.get("genome_size__source").is_none());
    }

    #[test]
    fn split_ancestor_value_goes_to_ancestral_column() {
        let records = flat_records("ploidy", "2", "ancestor");
        let out: serde_json::Value =
            serde_json::from_str(&split_source_columns(&records).unwrap()).unwrap();
        let row = &out[0];
        assert!(row["ploidy__direct"].is_null());
        assert!(row["ploidy__descendant"].is_null());
        assert_eq!(row["ploidy__ancestral"], 2);
    }

    #[test]
    fn split_descendant_value_goes_to_descendant_column() {
        let records = flat_records("mitochondrion_gc_percent", "45.5", "descendant");
        let out: serde_json::Value =
            serde_json::from_str(&split_source_columns(&records).unwrap()).unwrap();
        assert_eq!(out[0]["mitochondrion_gc_percent__descendant"], 45.5);
        assert!(out[0]["mitochondrion_gc_percent__direct"].is_null());
    }

    #[test]
    fn split_preserves_stat_subkeys() {
        let records = r#"[{"genome_size":3423000000,"genome_size__source":"direct","genome_size__min":3423000000}]"#;
        let out: serde_json::Value =
            serde_json::from_str(&split_source_columns(records).unwrap()).unwrap();
        // stat sub-key untouched.
        assert_eq!(out[0]["genome_size__min"], 3423000000_u64);
    }

    // ── values_only ───────────────────────────────────────────────────────────

    #[test]
    fn values_only_strips_subkey_columns() {
        let records = r#"[{"taxon_id":"9606","scientific_name":"Homo sapiens","taxon_rank":"species","genome_size":3423000000,"genome_size__source":"direct","genome_size__min":3423000000,"genome_size__max":3423000000}]"#;
        let out: serde_json::Value =
            serde_json::from_str(&values_only(records, "").unwrap()).unwrap();
        let row = &out[0];
        // Identity columns kept.
        assert_eq!(row["taxon_id"], "9606");
        assert_eq!(row["scientific_name"], "Homo sapiens");
        assert_eq!(row["taxon_rank"], "species");
        // Bare field value kept.
        assert_eq!(row["genome_size"], 3423000000_u64);
        // Sub-key columns removed.
        assert!(row.get("genome_size__source").is_none());
        assert!(row.get("genome_size__min").is_none());
        assert!(row.get("genome_size__max").is_none());
    }

    #[test]
    fn values_only_preserves_requested_stat_column() {
        let records = r#"[{"taxon_id":"9606","genome_size":3423000000,"genome_size__source":"direct","genome_size__min":3000000000,"genome_size__max":3423000000}]"#;
        let keep = r#"["genome_size__min"]"#;
        let out: serde_json::Value =
            serde_json::from_str(&values_only(records, keep).unwrap()).unwrap();
        let row = &out[0];
        assert_eq!(row["genome_size"], 3423000000_u64);
        // Requested stat preserved.
        assert_eq!(row["genome_size__min"], 3000000000_u64);
        // Non-requested stat still stripped.
        assert!(row.get("genome_size__max").is_none());
        assert!(row.get("genome_size__source").is_none());
    }

    #[test]
    fn values_only_on_empty_records_returns_empty_array() {
        let out = values_only("[]", "").unwrap();
        assert_eq!(out, "[]");
    }

    // ── annotated_values ──────────────────────────────────────────────────────

    #[test]
    fn annotated_values_labels_ancestral_field() {
        let records = flat_records("genome_size", "8215200000", "ancestor");
        let out: serde_json::Value =
            serde_json::from_str(&annotated_values(&records, "non_direct", "").unwrap()).unwrap();
        let row = &out[0];
        // Bare value replaced with labelled string.
        assert_eq!(row["genome_size"], "8215200000 (Ancestral)");
        // Sub-key columns stripped.
        assert!(row.get("genome_size__source").is_none());
        assert!(row.get("genome_size__label").is_none());
    }

    #[test]
    fn annotated_values_keeps_direct_value_numeric() {
        let records = flat_records("genome_size", "3423000000", "direct");
        let out: serde_json::Value =
            serde_json::from_str(&annotated_values(&records, "non_direct", "").unwrap()).unwrap();
        let row = &out[0];
        // Direct values are not labelled in non_direct mode — numeric preserved.
        assert_eq!(row["genome_size"], 3423000000_u64);
        assert!(row.get("genome_size__source").is_none());
        assert!(row.get("genome_size__label").is_none());
    }

    #[test]
    fn annotated_values_preserves_keep_column_alongside_label() {
        // Records with both genome_size (ancestral) and assembly_span__min.
        let records = r#"[{"taxon_id":"9606","genome_size":8215200000,"genome_size__source":"ancestor","assembly_span":100000,"assembly_span__source":"direct","assembly_span__min":90000}]"#;
        let keep = r#"["assembly_span__min"]"#;
        let out: serde_json::Value =
            serde_json::from_str(&annotated_values(records, "non_direct", keep).unwrap()).unwrap();
        let row = &out[0];
        // Annotated ancestral value.
        assert_eq!(row["genome_size"], "8215200000 (Ancestral)");
        // Direct assembly_span stays numeric (non_direct mode).
        assert_eq!(row["assembly_span"], 100000_u64);
        // Explicitly requested stat preserved.
        assert_eq!(row["assembly_span__min"], 90000_u64);
        // Source columns stripped.
        assert!(row.get("genome_size__source").is_none());
        assert!(row.get("assembly_span__source").is_none());
    }

    #[test]
    fn annotated_values_on_empty_records_returns_empty_array() {
        let out = annotated_values("[]", "non_direct", "").unwrap();
        assert_eq!(out, "[]");
    }

    // ── to_tidy_records ───────────────────────────────────────────────────────

    #[test]
    fn tidy_records_bare_field_with_source() {
        let records = r#"[{"taxon_id":"9606","scientific_name":"Homo sapiens","taxon_rank":"species","genome_size":3100000000,"genome_size__source":"direct"}]"#;
        let out: Vec<serde_json::Value> =
            serde_json::from_str(&to_tidy_records(records).unwrap()).unwrap();
        assert_eq!(out.len(), 1);
        let row = &out[0];
        assert_eq!(row["field"], "genome_size");
        assert_eq!(row["value"], 3100000000_i64);
        assert_eq!(row["source"], "direct");
        assert_eq!(row["taxon_id"], "9606");
        assert_eq!(row["scientific_name"], "Homo sapiens");
    }

    #[test]
    fn tidy_records_two_fields_become_two_rows() {
        let records = r#"[{"taxon_id":"9606","genome_size":3100000000,"genome_size__source":"direct","assembly_span":2747877777,"assembly_span__source":"ancestor"}]"#;
        let out: Vec<serde_json::Value> =
            serde_json::from_str(&to_tidy_records(records).unwrap()).unwrap();
        // two bare fields → two tidy rows
        assert_eq!(out.len(), 2);
        let field_names: Vec<&str> = out.iter().map(|r| r["field"].as_str().unwrap()).collect();
        assert!(field_names.contains(&"genome_size"));
        assert!(field_names.contains(&"assembly_span"));
    }

    #[test]
    fn tidy_records_modifier_column_emitted_as_own_row() {
        // assembly_span__min is a user-requested modifier column (assembly_span:min).
        // There is NO bare "assembly_span" key in this record.
        let records = r#"[{"taxon_id":"9606","assembly_span__min":2400000000}]"#;
        let out: Vec<serde_json::Value> =
            serde_json::from_str(&to_tidy_records(records).unwrap()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["field"], "assembly_span:min");
        assert_eq!(out[0]["value"], 2400000000_i64);
        assert_eq!(out[0]["source"], serde_json::Value::Null);
    }

    #[test]
    fn tidy_records_auto_subkey_not_emitted_when_bare_field_present() {
        // genome_size__min is an auto stat sub-key (bare "genome_size" present).
        // It should NOT produce its own tidy row.
        let records = r#"[{"taxon_id":"9606","genome_size":3100000000,"genome_size__source":"direct","genome_size__min":2800000000,"genome_size__max":3400000000}]"#;
        let out: Vec<serde_json::Value> =
            serde_json::from_str(&to_tidy_records(records).unwrap()).unwrap();
        // Only 1 tidy row: the bare genome_size field.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["field"], "genome_size");
    }

    #[test]
    fn tidy_records_empty_input_returns_empty_array() {
        let out = to_tidy_records("[]").unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn tidy_records_invalid_json_returns_error() {
        assert!(to_tidy_records("not json").is_err());
    }

    #[test]
    fn tidy_records_source_null_when_absent() {
        let records = r#"[{"taxon_id":"1","genome_size":1000}]"#;
        let out: Vec<serde_json::Value> =
            serde_json::from_str(&to_tidy_records(records).unwrap()).unwrap();
        assert_eq!(out[0]["source"], serde_json::Value::Null);
    }

    // ── parse_paginated_json ──────────────────────────────────────────────────

    fn paginated_response(has_more: bool, search_after: serde_json::Value) -> String {
        format!(
            r#"{{"status":{{"hits":2,"success":true}},
              "hits":[
                {{"index":"taxon","id":"9606","score":1.0,
                  "result":{{"taxon_id":"9606","scientific_name":"Homo sapiens",
                    "taxon_rank":"species","fields":{{}}}}}},
                {{"index":"taxon","id":"10090","score":0.9,
                  "result":{{"taxon_id":"10090","scientific_name":"Mus musculus",
                    "taxon_rank":"species","fields":{{}}}}}}
              ],
              "pagination":{{"hasMore":{has_more},"searchAfter":{search_after}}}}}"#
        )
    }

    #[test]
    fn paginated_parses_records_and_cursor() {
        let raw = paginated_response(true, serde_json::json!([0.9, "10090"]));
        let page = parse_paginated_json(&raw).unwrap();
        assert_eq!(page.total_hits, 2);
        assert!(page.has_more);
        assert_eq!(page.records.len(), 2);
        assert_eq!(page.records[0]["taxon_id"], "9606");
        assert_eq!(page.records[1]["taxon_id"], "10090");
        let cursor = page.next_cursor.unwrap();
        assert_eq!(cursor[0], serde_json::json!(0.9));
        assert_eq!(cursor[1], serde_json::json!("10090"));
    }

    #[test]
    fn paginated_last_page_has_more_false() {
        let raw = paginated_response(false, serde_json::json!(null));
        let page = parse_paginated_json(&raw).unwrap();
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn paginated_empty_hits() {
        let raw = r#"{"status":{"hits":0,"success":true},"hits":[],"pagination":{"hasMore":false,"searchAfter":null}}"#;
        let page = parse_paginated_json(raw).unwrap();
        assert_eq!(page.total_hits, 0);
        assert!(!page.has_more);
        assert!(page.records.is_empty());
    }

    #[test]
    fn paginated_to_json_round_trips() {
        let raw = paginated_response(true, serde_json::json!(["abc"]));
        let page = parse_paginated_json(&raw).unwrap();
        let json_str = paginated_page_to_json(&page);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["hasMore"], true);
        assert_eq!(v["totalHits"], 2);
        assert_eq!(v["records"].as_array().unwrap().len(), 2);
        assert_eq!(v["searchAfter"][0], "abc");
    }

    #[test]
    fn paginated_invalid_json_returns_err() {
        assert!(parse_paginated_json("not json").is_err());
    }

    // ── parse_msearch_json ────────────────────────────────────────────────────

    /// Build a minimal `/msearch` response JSON string.
    ///
    /// Each `(fields_json, total)` pair becomes one entry in `results`.
    fn msearch_response(entries: &[(&str, u64)]) -> String {
        let overall_total: u64 = entries.iter().map(|(_, t)| t).sum();
        let results: Vec<String> = entries
            .iter()
            .map(|(fields_json, total)| {
                format!(
                    r#"{{"status":"ok","count":1,"total":{total},
                    "hits":[{{"index":"taxon--ncbi","id":"9606","score":1.0,
                      "result":{{"taxon_id":"9606","scientific_name":"Homo sapiens",
                        "taxon_rank":"species","fields":{fields_json}}}}}]}}"#
                )
            })
            .collect();
        format!(
            r#"{{"status":{{"hits":{overall_total},"success":true}},
              "results":[{}]}}"#,
            results.join(",")
        )
    }

    #[test]
    fn parse_msearch_json_two_queries_returns_two_result_groups() {
        let raw = msearch_response(&[("{}", 5200), ("{}", 7300)]);
        let result = parse_msearch_json(&raw).unwrap();
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.total_hits, 12500);
    }

    #[test]
    fn parse_msearch_json_each_group_has_one_record() {
        let raw = msearch_response(&[("{}", 5200), ("{}", 7300)]);
        let result = parse_msearch_json(&raw).unwrap();
        assert_eq!(result.results[0].records.len(), 1);
        assert_eq!(result.results[1].records.len(), 1);
    }

    #[test]
    fn parse_msearch_json_per_query_totals_are_preserved() {
        let raw = msearch_response(&[("{}", 5200), ("{}", 7300)]);
        let result = parse_msearch_json(&raw).unwrap();
        assert_eq!(result.results[0].total, 5200);
        assert_eq!(result.results[1].total, 7300);
    }

    #[test]
    fn parse_msearch_json_records_are_flat() {
        let raw = msearch_response(&[("{}", 100)]);
        let result = parse_msearch_json(&raw).unwrap();
        let record = &result.results[0].records[0];
        assert_eq!(record["taxon_id"], "9606");
        assert_eq!(record["scientific_name"], "Homo sapiens");
        assert_eq!(record["taxon_rank"], "species");
    }

    #[test]
    fn parse_msearch_json_empty_results_array() {
        let raw = r#"{"status":{"hits":0,"success":true},"results":[]}"#;
        let result = parse_msearch_json(raw).unwrap();
        assert_eq!(result.results.len(), 0);
        assert_eq!(result.total_hits, 0);
    }

    #[test]
    fn parse_msearch_json_missing_results_key() {
        let raw = r#"{"status":{"hits":0,"success":true}}"#;
        let result = parse_msearch_json(raw).unwrap();
        assert!(result.results.is_empty());
    }

    #[test]
    fn parse_msearch_json_error_field_is_captured() {
        let raw = r#"{"status":{"hits":0,"success":true},"results":[
            {"status":"error","total":0,"hits":[],"error":"taxonomy not found"}
        ]}"#;
        let result = parse_msearch_json(raw).unwrap();
        assert_eq!(
            result.results[0].error.as_deref(),
            Some("taxonomy not found")
        );
        assert_eq!(result.results[0].records.len(), 0);
    }

    #[test]
    fn parse_msearch_json_no_error_is_none() {
        let raw = msearch_response(&[("{}", 10)]);
        let result = parse_msearch_json(&raw).unwrap();
        assert!(result.results[0].error.is_none());
    }

    #[test]
    fn parse_msearch_json_invalid_input_returns_err() {
        assert!(parse_msearch_json("not json at all {{}}").is_err());
    }

    #[test]
    fn msearch_result_to_json_round_trips() {
        let raw = msearch_response(&[("{}", 42)]);
        let result = parse_msearch_json(&raw).unwrap();
        let json_str = msearch_result_to_json(&result);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["totalHits"], 42);
        assert_eq!(v["results"].as_array().unwrap().len(), 1);
        assert_eq!(v["results"][0]["total"], 42);
        assert!(!v["results"][0]["records"].as_array().unwrap().is_empty());
    }

    #[test]
    fn msearch_result_to_json_null_error_in_output() {
        let raw = msearch_response(&[("{}", 5)]);
        let result = parse_msearch_json(&raw).unwrap();
        let json_str = msearch_result_to_json(&result);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(v["results"][0]["error"].is_null());
    }
}
