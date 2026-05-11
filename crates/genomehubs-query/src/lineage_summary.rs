//! Reduction logic for `lineage_summary` aggregation results into flat columns.
//!
//! The `lineage_summary` block in an API response has shape:
//! `rank → ancestor_taxon_id → field → distribution`
//!
//! A *distribution* is one of:
//! - keyword: `{"chromosome": 3, "scaffold": 2, …}`
//! - stats (numeric/date): `{"min": 0.5, "max": 99.1, "avg": 72.3, "count": 40}`
//! - no data: `{}`
//!
//! [`attach_lineage_summary_columns`] is the main entry point: given a flat row
//! and the response's `lineage_summary` block, it writes one or more columns
//! for every `(rank, field, modes)` combination specified in the config.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

// ── SummaryMode ───────────────────────────────────────────────────────────────

/// Controls how a field distribution is reduced to flat column(s).
#[derive(Debug, Clone, PartialEq)]
pub enum SummaryMode {
    /// Most common keyword value, or `null` for numeric/date distributions.
    Top,
    /// The top-N most common keyword values as a JSON array.
    TopN(usize),
    /// Full distribution object as-is.
    All,
    /// Count of distinct keyword values, or the `count` field from stats.
    Count,
    /// `min` from a stats distribution.
    Min,
    /// `max` from a stats distribution.
    Max,
    /// `avg` from a stats distribution.
    Avg,
    /// All four stats fields: min, max, avg, count — each as a separate column.
    Stats,
}

impl SummaryMode {
    /// Parse a mode name string into a [`SummaryMode`].
    ///
    /// Accepts `"top"`, `"all"`, `"count"`, `"min"`, `"max"`, `"avg"`,
    /// `"stats"`, or `"top_n:<N>"` (e.g. `"top_n:3"`).
    fn from_str(s: &str) -> Result<Self, String> {
        if let Some(rest) = s.strip_prefix("top_n:") {
            let n: usize = rest
                .trim()
                .parse()
                .map_err(|_| format!("invalid top_n value in '{s}'"))?;
            return Ok(SummaryMode::TopN(n));
        }
        match s.trim() {
            "top" => Ok(SummaryMode::Top),
            "all" => Ok(SummaryMode::All),
            "count" => Ok(SummaryMode::Count),
            "min" => Ok(SummaryMode::Min),
            "max" => Ok(SummaryMode::Max),
            "avg" => Ok(SummaryMode::Avg),
            "stats" => Ok(SummaryMode::Stats),
            other => Err(format!("unknown summary mode: '{other}'")),
        }
    }

    /// Column suffixes produced by this mode.
    ///
    /// An empty string `""` means the column is named `{rank}_{field}` directly.
    fn column_suffixes(&self) -> &'static [&'static str] {
        match self {
            SummaryMode::Top | SummaryMode::TopN(_) | SummaryMode::All => &[""],
            SummaryMode::Count => &["__count"],
            SummaryMode::Min => &["__min"],
            SummaryMode::Max => &["__max"],
            SummaryMode::Avg => &["__avg"],
            SummaryMode::Stats => &["__min", "__max", "__avg", "__count"],
        }
    }
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Parsed configuration: `rank → field → ordered list of modes`.
pub type SummaryConfig = HashMap<String, HashMap<String, Vec<SummaryMode>>>;

/// Parse a JSON config string into [`SummaryConfig`].
///
/// Config format:
/// ```json
/// {
///   "genus": {
///     "assembly_level": "top",
///     "genome_size": "stats",
///     "assembly_date": ["min", "max"]
///   }
/// }
/// ```
///
/// Each field value may be a single mode string or an array of mode strings.
/// An empty string produces an empty config (no columns attached).
pub fn parse_summary_config(config_json: &str) -> Result<SummaryConfig, String> {
    if config_json.trim().is_empty() {
        return Ok(SummaryConfig::new());
    }
    let raw: Value =
        serde_json::from_str(config_json).map_err(|e| format!("invalid config JSON: {e}"))?;
    let Value::Object(ranks) = raw else {
        return Err("lineage_summary config must be a JSON object".to_string());
    };

    let mut config = SummaryConfig::new();
    for (rank, fields_val) in ranks {
        let Value::Object(fields) = fields_val else {
            return Err(format!("config[{rank:?}] must be a JSON object"));
        };
        let mut field_map: HashMap<String, Vec<SummaryMode>> = HashMap::new();
        for (field, mode_val) in fields {
            let modes = match &mode_val {
                Value::String(s) => vec![SummaryMode::from_str(s)?],
                Value::Array(arr) => arr
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| "each mode must be a string".to_string())
                            .and_then(SummaryMode::from_str)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                other => {
                    return Err(format!(
                        "config[{rank:?}][{field:?}]: expected string or array, got {other}"
                    ))
                }
            };
            field_map.insert(field, modes);
        }
        config.insert(rank, field_map);
    }
    Ok(config)
}

// ── Distribution reduction ────────────────────────────────────────────────────

/// Reduce a field distribution to `(column_suffix, value)` pairs.
///
/// `column_suffix` is `""` for modes that collapse to a single value
/// (`Top`, `TopN`, `All`), or a `__stat` string for named stat modes.
///
/// When `dist` is an empty object (`{}`), every mode produces `null` for all
/// of its columns — this means the ancestor had matching species but none had
/// a value for this field.
pub fn reduce_distribution(dist: &Value, mode: &SummaryMode) -> Vec<(String, Value)> {
    let is_empty = dist.as_object().map(|o| o.is_empty()).unwrap_or(true);
    if is_empty {
        return mode
            .column_suffixes()
            .iter()
            .map(|s| (s.to_string(), Value::Null))
            .collect();
    }

    match mode {
        SummaryMode::Top => vec![("".to_string(), top_keyword_value(dist))],
        SummaryMode::TopN(n) => vec![("".to_string(), top_n_keyword_values(dist, *n))],
        SummaryMode::All => vec![("".to_string(), dist.clone())],
        SummaryMode::Count => vec![("__count".to_string(), dist_count(dist))],
        SummaryMode::Min => vec![("__min".to_string(), stat_field(dist, "min"))],
        SummaryMode::Max => vec![("__max".to_string(), stat_field(dist, "max"))],
        SummaryMode::Avg => vec![("__avg".to_string(), stat_field(dist, "avg"))],
        SummaryMode::Stats => vec![
            ("__min".to_string(), stat_field(dist, "min")),
            ("__max".to_string(), stat_field(dist, "max")),
            ("__avg".to_string(), stat_field(dist, "avg")),
            ("__count".to_string(), stat_field(dist, "count")),
        ],
    }
}

/// Extract a named stats field, returning `null` when absent.
fn stat_field(dist: &Value, key: &str) -> Value {
    dist.get(key).cloned().unwrap_or(Value::Null)
}

/// Return the keyword value with the highest doc_count, or `null`.
///
/// Returns `null` for stats distributions (which have `"count"` or `"min"` keys)
/// since there is no single "most common" value in a numeric range.
fn top_keyword_value(dist: &Value) -> Value {
    let obj = match dist.as_object() {
        Some(o) if !o.contains_key("count") && !o.contains_key("min") => o,
        _ => return Value::Null,
    };
    obj.iter()
        .filter_map(|(k, v)| Some((k.as_str(), v.as_u64()?)))
        .max_by_key(|&(_, c)| c)
        .map(|(k, _)| Value::String(k.to_string()))
        .unwrap_or(Value::Null)
}

/// Return the top-N keyword values by count as a JSON array, highest first.
///
/// Returns an empty array for stats distributions.
fn top_n_keyword_values(dist: &Value, n: usize) -> Value {
    let obj = match dist.as_object() {
        Some(o) if !o.contains_key("count") && !o.contains_key("min") => o,
        _ => return json!([]),
    };
    let mut pairs: Vec<(&str, u64)> = obj
        .iter()
        .filter_map(|(k, v)| Some((k.as_str(), v.as_u64()?)))
        .collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));
    Value::Array(
        pairs
            .into_iter()
            .take(n)
            .map(|(k, _)| Value::String(k.to_string()))
            .collect(),
    )
}

/// Count distinct keyword values, or return the `count` field for stats distributions.
fn dist_count(dist: &Value) -> Value {
    if let Some(c) = dist.get("count") {
        return c.clone();
    }
    let len = dist.as_object().map(|o| o.len()).unwrap_or(0) as u64;
    json!(len)
}

// ── Join ──────────────────────────────────────────────────────────────────────

/// Look up the ancestor taxon_id string at a given rank.
///
/// Checks `result.ranks` first (shape `{rank → {"taxon_id": [N], …}}`).
/// Falls back to scanning `result.lineage` (an array of `{taxon_rank, taxon_id, …}`
/// objects as returned by the API when `include_lineage` is true).
pub fn ancestor_taxon_id_at_rank(result: &Value, rank: &str) -> Option<String> {
    // Primary path: result.ranks object
    if let Some(ranks) = result.get("ranks").and_then(|v| v.as_object()) {
        if let Some(rank_obj) = ranks.get(rank).and_then(|v| v.as_object()) {
            if let Some(id_val) = rank_obj.get("taxon_id") {
                let id_elem = match id_val {
                    Value::Array(arr) => arr.first()?,
                    other => other,
                };
                return match id_elem {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                };
            }
        }
    }
    // Fallback: scan result.lineage array
    if let Some(lineage) = result.get("lineage").and_then(|v| v.as_array()) {
        for entry in lineage {
            let entry_rank = entry.get("taxon_rank").and_then(|v| v.as_str());
            if entry_rank == Some(rank) {
                let id_val = entry.get("taxon_id")?;
                return match id_val {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                };
            }
        }
    }
    None
}

/// Attach lineage summary columns to an already-flattened row.
///
/// For each `(rank, field, modes)` triple derived from `config`:
/// 1. Looks up the ancestor taxon_id from `ranks` (`result.ranks`).
/// 2. Retrieves the field's distribution from `lineage_summary`.
/// 3. Reduces it according to each mode and inserts the column(s) into `row`.
///
/// Column naming:
/// - `top` / `top_n` / `all` → `{rank}_{field}`
/// - `count` → `{rank}_{field}__count`
/// - `min` → `{rank}_{field}__min`
/// - `max` → `{rank}_{field}__max`
/// - `avg` → `{rank}_{field}__avg`
/// - `stats` → `{rank}_{field}__min`, `…__max`, `…__avg`, `…__count`
///
/// A missing ancestor or missing field always produces `null`.
pub fn attach_lineage_summary_columns(
    row: &mut Map<String, Value>,
    result: &Value,
    lineage_summary: &Value,
    config: &SummaryConfig,
) {
    for (rank, field_modes) in config {
        let ancestor_id = ancestor_taxon_id_at_rank(result, rank);
        let ancestor_data = ancestor_id
            .as_deref()
            .and_then(|id| lineage_summary.get(rank).and_then(|r| r.get(id)));

        for (field, modes) in field_modes {
            let dist = ancestor_data
                .and_then(|a| a.get(field.as_str()))
                .cloned()
                .map(|d| if d.is_null() { json!({}) } else { d })
                .unwrap_or_else(|| json!({}));

            for mode in modes {
                for (suffix, value) in reduce_distribution(&dist, mode) {
                    row.insert(format!("{rank}__{field}{suffix}"), value);
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn keyword_dist() -> Value {
        json!({"chromosome": 5, "scaffold": 3, "contig": 1})
    }

    fn stats_dist() -> Value {
        json!({"min": 0.5, "max": 99.1, "avg": 72.3, "count": 40})
    }

    // ── parse_summary_config ─────────────────────────────────────────────────

    #[test]
    fn test_parse_config_single_mode() {
        let cfg = parse_summary_config(r#"{"genus": {"assembly_level": "top"}}"#).unwrap();
        assert_eq!(cfg["genus"]["assembly_level"], vec![SummaryMode::Top]);
    }

    #[test]
    fn test_parse_config_array_modes() {
        let cfg = parse_summary_config(r#"{"genus": {"genome_size": ["min", "max"]}}"#).unwrap();
        assert_eq!(
            cfg["genus"]["genome_size"],
            vec![SummaryMode::Min, SummaryMode::Max]
        );
    }

    #[test]
    fn test_parse_config_stats_shorthand() {
        let cfg = parse_summary_config(r#"{"genus": {"genome_size": "stats"}}"#).unwrap();
        assert_eq!(cfg["genus"]["genome_size"], vec![SummaryMode::Stats]);
    }

    #[test]
    fn test_parse_config_top_n() {
        let cfg = parse_summary_config(r#"{"genus": {"assembly_level": "top_n:3"}}"#).unwrap();
        assert_eq!(cfg["genus"]["assembly_level"], vec![SummaryMode::TopN(3)]);
    }

    #[test]
    fn test_parse_config_empty_string() {
        let cfg = parse_summary_config("").unwrap();
        assert!(cfg.is_empty());
    }

    #[test]
    fn test_parse_config_unknown_mode_errors() {
        let result = parse_summary_config(r#"{"genus": {"assembly_level": "bogus"}}"#);
        assert!(result.is_err());
    }

    // ── reduce_distribution ──────────────────────────────────────────────────

    #[test]
    fn test_top_picks_highest_count() {
        let pairs = reduce_distribution(&keyword_dist(), &SummaryMode::Top);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "");
        assert_eq!(pairs[0].1, json!("chromosome"));
    }

    #[test]
    fn test_top_on_stats_returns_null() {
        let pairs = reduce_distribution(&stats_dist(), &SummaryMode::Top);
        assert_eq!(pairs[0].1, Value::Null);
    }

    #[test]
    fn test_top_n_ordered_by_count() {
        let pairs = reduce_distribution(&keyword_dist(), &SummaryMode::TopN(2));
        let arr = pairs[0].1.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], json!("chromosome"));
        assert_eq!(arr[1], json!("scaffold"));
    }

    #[test]
    fn test_all_mode_returns_full_dist() {
        let pairs = reduce_distribution(&keyword_dist(), &SummaryMode::All);
        assert_eq!(pairs[0].1, keyword_dist());
    }

    #[test]
    fn test_count_mode_keyword() {
        let pairs = reduce_distribution(&keyword_dist(), &SummaryMode::Count);
        assert_eq!(pairs[0].0, "__count");
        assert_eq!(pairs[0].1, json!(3u64));
    }

    #[test]
    fn test_count_mode_stats_uses_count_field() {
        let pairs = reduce_distribution(&stats_dist(), &SummaryMode::Count);
        assert_eq!(pairs[0].1, json!(40));
    }

    #[test]
    fn test_stats_mode_four_columns() {
        let pairs = reduce_distribution(&stats_dist(), &SummaryMode::Stats);
        assert_eq!(pairs.len(), 4);
        let map: HashMap<String, &Value> = pairs.iter().map(|(k, v)| (k.clone(), v)).collect();
        assert_eq!(*map["__min"], json!(0.5));
        assert_eq!(*map["__max"], json!(99.1));
        assert_eq!(*map["__avg"], json!(72.3));
        assert_eq!(*map["__count"], json!(40));
    }

    #[test]
    fn test_empty_dist_produces_nulls_for_all_modes() {
        let empty = json!({});
        for mode in &[
            SummaryMode::Top,
            SummaryMode::All,
            SummaryMode::Count,
            SummaryMode::Min,
            SummaryMode::Stats,
        ] {
            let pairs = reduce_distribution(&empty, mode);
            assert!(
                pairs.iter().all(|(_, v)| v.is_null()),
                "mode {mode:?} should produce nulls for empty dist"
            );
        }
    }

    // ── ancestor_taxon_id_at_rank ────────────────────────────────────────────

    #[test]
    fn test_ancestor_id_array_form() {
        let result =
            json!({"ranks": {"genus": {"taxon_id": [9701], "scientific_name": ["Canis"]}}});
        assert_eq!(
            ancestor_taxon_id_at_rank(&result, "genus").as_deref(),
            Some("9701")
        );
    }

    #[test]
    fn test_ancestor_id_string_form() {
        let result = json!({"ranks": {"genus": {"taxon_id": ["9701"]}}});
        assert_eq!(
            ancestor_taxon_id_at_rank(&result, "genus").as_deref(),
            Some("9701")
        );
    }

    #[test]
    fn test_ancestor_id_missing_rank() {
        let result = json!({"ranks": {"family": {"taxon_id": [9700]}}});
        assert!(ancestor_taxon_id_at_rank(&result, "genus").is_none());
    }

    #[test]
    fn test_ancestor_id_lineage_fallback() {
        // When result.ranks is absent, fall back to scanning result.lineage
        let result = json!({
            "taxon_id": "9612",
            "parent": "9611",
            "lineage": [
                {"taxon_rank": "species", "taxon_id": "9612"},
                {"taxon_rank": "genus",   "taxon_id": "9611"},
                {"taxon_rank": "family",  "taxon_id": "9608"}
            ]
        });
        assert_eq!(
            ancestor_taxon_id_at_rank(&result, "genus").as_deref(),
            Some("9611")
        );
        assert_eq!(
            ancestor_taxon_id_at_rank(&result, "family").as_deref(),
            Some("9608")
        );
        assert!(ancestor_taxon_id_at_rank(&result, "order").is_none());
    }

    // ── attach_lineage_summary_columns ───────────────────────────────────────

    #[test]
    fn test_attach_top_mode() {
        let mut row = serde_json::Map::new();
        let result = json!({"ranks": {"genus": {"taxon_id": ["9701"]}}});
        let lineage_summary = json!({
            "genus": {
                "9701": {"assembly_level": {"chromosome": 5, "scaffold": 2}}
            }
        });
        let config = parse_summary_config(r#"{"genus": {"assembly_level": "top"}}"#).unwrap();
        attach_lineage_summary_columns(&mut row, &result, &lineage_summary, &config);
        assert_eq!(row["genus__assembly_level"], json!("chromosome"));
    }

    #[test]
    fn test_attach_stats_mode() {
        let mut row = serde_json::Map::new();
        let result = json!({"ranks": {"genus": {"taxon_id": ["9701"]}}});
        let lineage_summary = json!({
            "genus": {
                "9701": {
                    "genome_size": {"min": 1.0e9, "max": 3.2e9, "avg": 2.1e9, "count": 5}
                }
            }
        });
        let config = parse_summary_config(r#"{"genus": {"genome_size": "stats"}}"#).unwrap();
        attach_lineage_summary_columns(&mut row, &result, &lineage_summary, &config);
        assert_eq!(row["genus__genome_size__min"], json!(1.0e9));
        assert_eq!(row["genus__genome_size__count"], json!(5));
    }

    #[test]
    fn test_attach_multi_mode_same_field() {
        let mut row = serde_json::Map::new();
        let result = json!({"ranks": {"genus": {"taxon_id": ["9701"]}}});
        let lineage_summary = json!({
            "genus": {"9701": {"assembly_level": {"chromosome": 5, "scaffold": 2}}}
        });
        let config =
            parse_summary_config(r#"{"genus": {"assembly_level": ["top", "count"]}}"#).unwrap();
        attach_lineage_summary_columns(&mut row, &result, &lineage_summary, &config);
        assert_eq!(row["genus__assembly_level"], json!("chromosome"));
        assert_eq!(row["genus__assembly_level__count"], json!(2u64));
    }

    #[test]
    fn test_attach_missing_ancestor_produces_nulls() {
        let mut row = serde_json::Map::new();
        let result = json!({});
        let lineage_summary = json!({"genus": {}});
        let config = parse_summary_config(r#"{"genus": {"assembly_level": "top"}}"#).unwrap();
        attach_lineage_summary_columns(&mut row, &result, &lineage_summary, &config);
        assert_eq!(row["genus__assembly_level"], Value::Null);
    }

    #[test]
    fn test_attach_empty_field_distribution_produces_nulls() {
        let mut row = serde_json::Map::new();
        let result = json!({"ranks": {"genus": {"taxon_id": ["9701"]}}});
        let lineage_summary = json!({
            "genus": {"9701": {"assembly_level": {}}}
        });
        let config = parse_summary_config(r#"{"genus": {"assembly_level": "top"}}"#).unwrap();
        attach_lineage_summary_columns(&mut row, &result, &lineage_summary, &config);
        assert_eq!(row["genus__assembly_level"], Value::Null);
    }
}
