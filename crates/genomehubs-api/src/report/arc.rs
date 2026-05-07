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

use reqwest::Client;
use serde_json::{json, Value};

use crate::report::filter_expr::filter_expr_to_es_query;

// ============================================================================
// Public API
// ============================================================================

/// Arc report config parsed from `report_yaml`.
pub struct ArcConfig {
    pub feature_term: String,
    pub reference_term: String,
    pub context_term: Option<String>,
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
            .ok_or("arc report requires 'reference' query string")?
            .to_string();
        let context = config
            .get("context")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Ok(Self {
            feature_term: feature,
            reference_term: reference,
            context_term: context,
        })
    }
}

/// Run an arc report.
///
/// Issues 2 or 3 parallel `_count` queries:
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
    let feature_ref_filter = filter_expr_to_es_query(
        &combine_terms(&config.feature_term, &config.reference_term),
        base_query,
    )?;
    let reference_filter = filter_expr_to_es_query(&config.reference_term, base_query)?;

    if let Some(ref context_term) = config.context_term {
        let context_filter = filter_expr_to_es_query(context_term, base_query)?;

        let (feature_count, reference_count, context_count) = tokio::try_join!(
            count_matching(client, es_base, index, &feature_ref_filter),
            count_matching(client, es_base, index, &reference_filter),
            count_matching(client, es_base, index, &context_filter),
        )?;

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
        let (feature_count, reference_count) = tokio::try_join!(
            count_matching(client, es_base, index, &feature_ref_filter),
            count_matching(client, es_base, index, &reference_filter),
        )?;

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

// ============================================================================
// Private helpers
// ============================================================================

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
}
