//! Arc report — count document overlap between two or three filter expressions.
//!
//! # Semantics
//!
//! `arc` and `arc2` are **fractions**:
//!
//! - `feature_count` = count(feature AND reference within base query) — the intersection
//! - `reference_count` = count(reference within base query)
//! - `context_count` = count(context within base query) (when provided)
//! - `arc`  = feature_count / reference_count — proportion of reference that satisfies feature
//! - `arc2` = reference_count / context_count  — how reference relates to the broader context
//!
//! # Config keys
//!
//! ```yaml
//! report: arc
//! feature: "genome_size>3000000000"    # required — the specific filter being investigated
//! reference: "genome_size>1000000000" # required — the denominator group
//! context: "assembly_level=Chromosome" # optional — broader backdrop; enables arc2
//! ```
//!
//! # Response shape (two terms)
//!
//! ```json
//! { "type": "arc",
//!   "arc": 0.127,
//!   "feature_count": 16, "reference_count": 126,
//!   "featureTerm": "genome_size>3000000000",
//!   "referenceTerm": "genome_size>1000000000",
//!   "queryString": "genome_size>3000000000 AND genome_size>1000000000" }
//! ```
//!
//! # Response shape (three terms)
//!
//! ```json
//! { "type": "arc",
//!   "arc": 0.127, "arc2": 18.0,
//!   "feature_count": 16, "reference_count": 126, "context_count": 7,
//!   "featureTerm": "...", "referenceTerm": "...", "contextTerm": "...",
//!   "queryString": "..." }
//! ```
//!
//! # Multi-ring arc
//!
//! When `rings` is provided in config, each ring defines its own `feature` filter.
//! All count queries are batched via a single `_msearch` request.  The response
//! uses an array for `arc`:
//!
//! ```json
//! { "type": "arc",
//!   "arc": [
//!     { "ring": 0, "label": "has genome size",  "arc": 0.82, "feature_count": 8200, "reference_count": 10000 },
//!     { "ring": 1, "label": "has C-value",       "arc": 0.41, "feature_count": 4100, "reference_count": 10000 }
//!   ]
//! }
//! ```
//!
//! # Per-rank arc (`arcPerRank` shorthand)
//!
//! When `ranks` is provided, the same `feature`/`reference` is run once per
//! rank.  Each ring's base query is scoped to that rank by ANDing a
//! `term: taxon_rank` filter before the attribute filters.
//!
//! ```yaml
//! report: arc
//! feature: "genome_size>3000000000"
//! reference: "genome_size>1000000000"
//! ranks: [genus, family, order]
//! ```
//!
//! Response shape is the same multi-ring array, with `label` set to the rank name.

use reqwest::Client;
use serde_json::{json, Value};

use crate::report::filter_expr::filter_expr_to_es_query;

// ============================================================================
// Public API
// ============================================================================

/// A single ring definition for a multi-ring arc report.
///
/// Each ring measures `count(feature AND reference) / count(reference)`.
/// When `reference_term` is `None`, the ring inherits the outer
/// [`ArcConfig::reference_term`].
pub struct RingSpec {
    /// The feature filter for this ring (numerator).
    pub feature_term: String,
    /// Override for the reference (denominator) query.  `None` → use outer reference.
    pub reference_term: Option<String>,
    /// Human-readable label for this ring.
    pub label: Option<String>,
}

/// Arc report config parsed from `report_yaml`.
pub struct ArcConfig {
    pub feature_term: String,
    pub reference_term: String,
    pub context_term: Option<String>,
    /// When present, replaces the single arc with a concentric multi-ring arc
    /// where each ring has its own `feature` (and optionally `reference`) filter.
    pub rings: Option<Vec<RingSpec>>,
    /// When present, runs the same `feature`/`reference` once per rank.
    ///
    /// Each ring's base query is scoped to that rank via a `term: taxon_rank`
    /// filter.  Shorthand for v2's `arcPerRank`.
    /// Cannot be combined with `rings`; `ranks` takes priority.
    pub ranks: Option<Vec<String>>,
}

impl ArcConfig {
    /// Parse config from a `report_yaml` value.
    pub fn from_yaml(config: &serde_yaml::Value) -> Result<Self, String> {
        let feature = config
            .get("feature")
            .and_then(|v| v.as_str())
            .ok_or("arc report requires 'feature' query string")?
            .to_string();
        let reference = config
            .get("reference")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let context = config
            .get("context")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        // Parse optional `rings` array.
        let rings = match config.get("rings").and_then(|v| v.as_sequence()) {
            None => None,
            Some(seq) => {
                let parsed: Result<Vec<RingSpec>, String> = seq
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        let feature_term = entry
                            .get("feature")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| format!("ring[{i}] requires 'feature'"))?
                            .to_string();
                        let reference_term = entry
                            .get("reference")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        let label = entry
                            .get("label")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        Ok(RingSpec {
                            feature_term,
                            reference_term,
                            label,
                        })
                    })
                    .collect();
                Some(parsed?)
            }
        };

        // Parse optional `ranks` array.
        let ranks = config
            .get("ranks")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());

        Ok(Self {
            feature_term: feature,
            reference_term: reference,
            context_term: context,
            rings,
            ranks,
        })
    }
}

/// Run an arc report.
///
/// When `config.rings` is `Some`, delegates to [`run_rings_report`] which
/// batches all count queries via a single `_msearch` request.
///
/// Otherwise issues 2 or 3 parallel `_count` queries:
///
/// - always: `count(feature AND reference)` → `feature_count`, `count(reference)` → `reference_count`
/// - with context: additionally `count(context)` → `context_count`
///
/// `arc` = feature_count / reference_count;
/// `arc2` = reference_count / context_count (can exceed 1).
///
/// Returns `(total_hits, took_ms, report_data)`.
pub async fn run_arc_report(
    client: &Client,
    es_base: &str,
    index: &str,
    base_query: &Value,
    config: &ArcConfig,
) -> Result<(u64, u64, Value), String> {
    if config.ranks.is_some() {
        return run_per_rank_report(client, es_base, index, base_query, config).await;
    }
    let feature_ref_filter = filter_expr_to_es_query(
        &combine_terms(&config.feature_term, &config.reference_term),
        base_query,
    )?;
    let reference_filter = filter_expr_to_es_query(&config.reference_term, base_query)?;

    let (feature_count, reference_count) = tokio::try_join!(
        count_matching(client, es_base, index, &feature_ref_filter),
        count_matching(client, es_base, index, &reference_filter),
    )?;
    let context_count = if let Some(ref context_term) = config.context_term {
        let context_filter = filter_expr_to_es_query(context_term, base_query)?;

        let (context_count,) =
            tokio::try_join!(count_matching(client, es_base, index, &context_filter))?;
        Some(context_count)
    } else {
        None
    };
    if config.rings.is_some() {
        return run_rings_report(client, es_base, index, base_query, config, context_count).await;
    }

    if let Some(ref context_term) = config.context_term {
        let context_count = context_count.unwrap_or(0);

        let arc = safe_fraction(feature_count, reference_count);
        let arc2 = safe_fraction(reference_count, context_count);

        let report_data = json!({
            "type": "arc",
            "arc": arc,
            "arc2": arc2,
            "feature_count": feature_count,
            "reference_count": reference_count,
            "context_count": context_count,
            "featureTerm": &config.feature_term,
            "referenceTerm": &config.reference_term,
            "contextTerm": context_term,
            "queryString": combine_terms(&config.feature_term, &config.reference_term)
        });
        Ok((feature_count, 0, report_data))
    } else {
        let arc = safe_fraction(feature_count, reference_count);

        let report_data = json!({
            "type": "arc",
            "arc": arc,
            "feature_count": feature_count,
            "reference_count": reference_count,
            "featureTerm": &config.feature_term,
            "referenceTerm": &config.reference_term,
            "queryString": combine_terms(&config.feature_term, &config.reference_term)
        });
        Ok((feature_count, 0, report_data))
    }
}

/// Run a multi-ring arc report using a single `_msearch` batch.
///
/// For N rings sharing the same outer reference:
/// - Builds 1 query for `count(reference)` (shared denominator)
/// - Builds N queries for `count(ring.feature AND ring_reference)` (one per ring)
/// - Total: N+1 count queries in one `_msearch` request
///
/// Each ring may override the reference with its own `reference_term`.
async fn run_rings_report(
    client: &Client,
    es_base: &str,
    index: &str,
    base_query: &Value,
    config: &ArcConfig,
    context_count: Option<u64>,
) -> Result<(u64, u64, Value), String> {
    let rings = config.rings.as_deref().unwrap_or(&[]);

    // Build the list of ES queries to batch.
    // Layout: [reference_query, ring_0_query, ring_1_query, ...]
    let reference_filter = filter_expr_to_es_query(&config.reference_term, base_query)?;

    let mut queries: Vec<Value> = Vec::with_capacity(rings.len() + 1);
    queries.push(reference_filter.clone());

    for ring in rings {
        let ring_ref = ring
            .reference_term
            .as_deref()
            .unwrap_or(&config.reference_term);
        let combined = combine_terms(&ring.feature_term, ring_ref);
        let ring_filter = filter_expr_to_es_query(&combined, base_query)?;
        queries.push(ring_filter);
    }

    let counts = msearch_counts(client, es_base, index, &queries).await?;
    let reference_count = counts[0];

    let ring_entries: Vec<Value> = rings
        .iter()
        .enumerate()
        .map(|(i, ring)| {
            let feature_count = counts[i + 1];
            let ring_ref = ring
                .reference_term
                .as_deref()
                .unwrap_or(&config.reference_term);
            let arc = safe_fraction(feature_count, reference_count);
            let mut entry = serde_json::Map::new();
            entry.insert("ring".to_string(), json!(i));
            if let Some(label) = &ring.label {
                entry.insert("label".to_string(), json!(label));
            }
            entry.insert("arc".to_string(), json!(arc));
            entry.insert("feature_count".to_string(), json!(feature_count));
            entry.insert("reference_count".to_string(), json!(reference_count));
            entry.insert("featureTerm".to_string(), json!(ring.feature_term));
            entry.insert("referenceTerm".to_string(), json!(ring_ref));

            if let Some(context_count) = context_count {
                let arc2 = safe_fraction(reference_count, context_count);
                entry.insert("arc2".to_string(), json!(arc2));
                entry.insert("context_count".to_string(), json!(context_count));
                entry.insert(
                    "contextTerm".to_string(),
                    json!(config.context_term.as_deref().unwrap_or("")),
                );
            }
            Value::Object(entry)
        })
        .collect();

    let total_hits = ring_entries
        .first()
        .and_then(|e| e["feature_count"].as_u64())
        .unwrap_or(0);

    let report_data = json!({
        "type": "arc",
        "arc": ring_entries,
        "referenceTerm": &config.reference_term,
        "reference_count": reference_count,
    });
    Ok((total_hits, 0, report_data))
}

/// Run an `arcPerRank` report: same `feature`/`reference`, one ring per rank.
///
/// Each ring's base query is scoped to the given `taxon_rank` value before
/// the attribute filters are applied.  All 2N count queries are batched in a
/// single `_msearch` request.
async fn run_per_rank_report(
    client: &Client,
    es_base: &str,
    index: &str,
    base_query: &Value,
    config: &ArcConfig,
) -> Result<(u64, u64, Value), String> {
    let ranks = config.ranks.as_deref().unwrap_or(&[]);

    // Build query pairs: [ref_rank0, feat_rank0, ref_rank1, feat_rank1, ...]
    let mut queries: Vec<Value> = Vec::with_capacity(ranks.len() * 2);
    for rank in ranks {
        let rank_base = rank_scoped_base_query(base_query, rank);
        let reference_filter = filter_expr_to_es_query(&config.reference_term, &rank_base)?;
        let feature_ref_filter = filter_expr_to_es_query(
            &combine_terms(&config.feature_term, &config.reference_term),
            &rank_base,
        )?;
        queries.push(reference_filter);
        queries.push(feature_ref_filter);
    }

    let counts = msearch_counts(client, es_base, index, &queries).await?;

    let ring_entries: Vec<Value> = ranks
        .iter()
        .enumerate()
        .map(|(i, rank)| {
            let reference_count = counts[i * 2];
            let feature_count = counts[i * 2 + 1];
            let arc = safe_fraction(feature_count, reference_count);
            json!({
                "ring": i,
                "label": rank,
                "arc": arc,
                "feature_count": feature_count,
                "reference_count": reference_count,
                "featureTerm": &config.feature_term,
                "referenceTerm": &config.reference_term,
                "rank": rank,
            })
        })
        .collect();

    let total_hits = ring_entries
        .first()
        .and_then(|e| e["reference_count"].as_u64())
        .unwrap_or(0);

    let report_data = json!({
        "type": "arc",
        "arc": ring_entries,
        "featureTerm": &config.feature_term,
        "referenceTerm": &config.reference_term,
    });
    Ok((total_hits, 0, report_data))
}

// ============================================================================
// Private helpers
// ============================================================================

/// Execute multiple count queries in a single `_msearch` request.
///
/// Each entry in `queries` is an ES `query` object (the value inside
/// `"query": ...`).  Returns one hit count per input query in the same order.
async fn msearch_counts(
    client: &Client,
    es_base: &str,
    index: &str,
    queries: &[Value],
) -> Result<Vec<u64>, String> {
    let base = es_base.trim_end_matches('/');
    let url = format!("{base}/{index}/_msearch");

    // Build ndjson body: alternating header + body lines.
    let header = json!({ "index": index });
    let mut body = String::new();
    for query in queries {
        body.push_str(&serde_json::to_string(&header).unwrap());
        body.push('\n');
        body.push_str(
            &serde_json::to_string(&json!({ "query": query, "size": 0, "track_total_hits": true }))
                .unwrap(),
        );
        body.push('\n');
    }

    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("arc msearch request failed: {e}"))?;

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("arc msearch parse error: {e}"))?;

    let responses = data["responses"]
        .as_array()
        .ok_or("arc msearch: missing 'responses' array")?;

    responses
        .iter()
        .enumerate()
        .map(|(i, r)| {
            r["hits"]["total"]["value"]
                .as_u64()
                .ok_or_else(|| format!("arc msearch: missing hit count at index {i}"))
        })
        .collect()
}

/// Count documents in `index` matching `query`.
async fn count_matching(
    client: &Client,
    es_base: &str,
    index: &str,
    query: &Value,
) -> Result<u64, String> {
    let url = format!("{}/{index}/_count", es_base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&json!({ "query": query }))
        .send()
        .await
        .map_err(|e| format!("arc count request failed: {e}"))?;
    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("arc count parse error: {e}"))?;
    Ok(data.get("count").and_then(|v| v.as_u64()).unwrap_or(0))
}

/// Build a rank-scoped base query by ANDing a `term: taxon_rank` filter into
/// an existing base query.
///
/// The result is used as the scope for per-rank count queries in
/// [`run_per_rank_report`].
fn rank_scoped_base_query(base_query: &Value, rank: &str) -> Value {
    let rank_clause = json!({ "term": { "taxon_rank": rank } });
    json!({
        "bool": {
            "must": [base_query, rank_clause]
        }
    })
}

/// Concatenate two filter expressions with `AND`.
fn combine_terms(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a} AND {b}"),
    }
}

/// Compute numerator / denominator as f64, returning 0.0 when denominator is 0.
fn safe_fraction(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_terms_both_non_empty() {
        assert_eq!(
            combine_terms("country=BR", "genome_size>1e9"),
            "country=BR AND genome_size>1e9"
        );
    }

    #[test]
    fn combine_terms_first_empty() {
        assert_eq!(combine_terms("", "genome_size>1e9"), "genome_size>1e9");
    }

    #[test]
    fn combine_terms_second_empty() {
        assert_eq!(combine_terms("country=BR", ""), "country=BR");
    }

    #[test]
    fn safe_fraction_normal() {
        assert!((safe_fraction(1, 2) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn safe_fraction_zero_denominator() {
        assert_eq!(safe_fraction(5, 0), 0.0);
    }

    #[test]
    fn arc_config_parses_two_terms() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("feature: country=BR\nreference: genome_size>1e9").unwrap();
        let cfg = ArcConfig::from_yaml(&yaml).unwrap();
        assert_eq!(cfg.feature_term, "country=BR");
        assert_eq!(cfg.reference_term, "genome_size>1e9");
        assert!(cfg.context_term.is_none());
    }

    #[test]
    fn arc_config_parses_three_terms() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            "feature: country=BR\nreference: genome_size>1e9\ncontext: gc_percent>45",
        )
        .unwrap();
        let cfg = ArcConfig::from_yaml(&yaml).unwrap();
        assert_eq!(cfg.context_term.as_deref(), Some("gc_percent>45"));
    }

    #[test]
    fn arc_config_errors_on_missing_feature() {
        let yaml: serde_yaml::Value = serde_yaml::from_str("reference: genome_size>1e9").unwrap();
        assert!(ArcConfig::from_yaml(&yaml).is_err());
    }

    #[test]
    fn arc_config_parses_rings() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            "feature: genome_size>0\nreference: genome_size>0\nrings:\n  - feature: c_value>0\n    label: has C-value\n  - feature: chromosome_count>0\n    label: has chr count",
        )
        .unwrap();
        let cfg = ArcConfig::from_yaml(&yaml).unwrap();
        let rings = cfg.rings.unwrap();
        assert_eq!(rings.len(), 2);
        assert_eq!(rings[0].feature_term, "c_value>0");
        assert_eq!(rings[0].label.as_deref(), Some("has C-value"));
        assert!(rings[0].reference_term.is_none());
        assert_eq!(rings[1].feature_term, "chromosome_count>0");
    }

    #[test]
    fn arc_config_rings_can_override_reference() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            "feature: genome_size>0\nreference: genome_size>0\nrings:\n  - feature: c_value>0\n    reference: assembly_level=Chromosome",
        )
        .unwrap();
        let cfg = ArcConfig::from_yaml(&yaml).unwrap();
        let rings = cfg.rings.unwrap();
        assert_eq!(
            rings[0].reference_term.as_deref(),
            Some("assembly_level=Chromosome")
        );
    }

    #[test]
    fn arc_config_parses_ranks() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            "feature: genome_size>3000000000\nreference: genome_size>1000000000\nranks:\n  - genus\n  - family\n  - order",
        )
        .unwrap();
        let cfg = ArcConfig::from_yaml(&yaml).unwrap();
        let ranks = cfg.ranks.unwrap();
        assert_eq!(ranks, vec!["genus", "family", "order"]);
        assert!(cfg.rings.is_none());
    }

    #[test]
    fn rank_scoped_base_query_adds_taxon_rank_term() {
        let base = json!({ "match_all": {} });
        let scoped = rank_scoped_base_query(&base, "genus");
        assert_eq!(scoped["bool"]["must"][1]["term"]["taxon_rank"], "genus");
    }
}
