//! Shared filter-expression parser for nested-attributes ES queries.
//!
//! Compiles compact query-string fragments such as
//! `genome_size>3e9 AND assembly_level=Chromosome` into the nested `attributes[]`
//! ES clause structure used by both the tree `status_filter` and the arc report
//! `x`/`y`/`z` params.
//!
//! # Grammar
//!
//! ```text
//! expr        := term (AND term)*
//! term        := agg_term | simple_term
//! agg_term    := AGG_FN '(' field ')' OP value
//! simple_term := field OP value | field '=' value
//! AGG_FN      := min | max | mean | median
//! OP          := >= | <= | > | <
//! ```
//!
//! `AND` is case-insensitive. `OR` and parentheses are deferred.

use serde_json::{json, Value};

// ─── Public types ────────────────────────────────────────────────────────────

/// A single parsed filter term.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterTerm {
    /// `field OP value` — matched against `long_value`, `float_value`, and
    /// `half_float_value` sub-fields with `minimum_should_match: 1`.
    NumericRange {
        field: String,
        op: &'static str,
        value: f64,
    },
    /// `agg(field) OP value` — matched against the named summary sub-field
    /// (`min`, `max`, `median`, or `mean`).
    AggRange {
        field: String,
        agg: String,
        op: &'static str,
        value: f64,
    },
    /// `field=value` — matched against `keyword_value`.
    KeywordMatch { field: String, value: String },
    /// `field` — bare field name, matches any document that has this attribute.
    /// Mirrors V2's implicit `excludeMissing` behaviour when `x` is a bare field.
    Exists { field: String },
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parse a filter expression string into an ordered list of [`FilterTerm`]s.
///
/// Terms are separated by `AND` (case-insensitive). Returns an error string
/// when an individual term cannot be parsed.
pub fn parse_filter_string(expr: &str) -> Result<Vec<FilterTerm>, String> {
    let terms: Vec<&str> = split_and_terms(expr);
    terms.iter().map(|t| parse_single_term(t.trim())).collect()
}

/// Compile a slice of [`FilterTerm`]s into an ES nested-attributes boolean clause.
///
/// Each term becomes a `nested` query over `attributes[]`. Multiple terms are
/// wrapped in `bool.must`. An empty slice returns `{"match_all": {}}`.
pub fn build_nested_attribute_query(terms: &[FilterTerm]) -> Value {
    let nested_clauses: Vec<Value> = terms.iter().map(term_to_nested_query).collect();
    match nested_clauses.len() {
        0 => json!({ "match_all": {} }),
        1 => nested_clauses.into_iter().next().unwrap(),
        _ => json!({ "bool": { "must": nested_clauses } }),
    }
}

/// Parse, compile, and AND with `base_query` in one step.
///
/// Returns the combined ES clause, or an error if the expression is invalid.
/// When the expression is empty the original `base_query` is returned unchanged.
pub fn filter_expr_to_es_query(expr: &str, base_query: &Value) -> Result<Value, String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Ok(base_query.clone());
    }
    let terms = parse_filter_string(trimmed)?;
    let filter_clause = build_nested_attribute_query(&terms);
    Ok(json!({
        "bool": {
            "must": [ base_query.clone(), filter_clause ]
        }
    }))
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Split `expr` on case-insensitive literal ` AND ` boundaries.
fn split_and_terms(expr: &str) -> Vec<&str> {
    let bytes = expr.as_bytes();
    let mut splits: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 5 <= bytes.len() {
        let matches = bytes[i..i + 5]
            .iter()
            .zip(b" AND ")
            .all(|(a, b)| a.to_ascii_uppercase() == *b);
        if matches {
            splits.push(i);
            i += 5;
        } else {
            i += 1;
        }
    }

    if splits.is_empty() {
        return vec![expr];
    }

    let mut result = Vec::with_capacity(splits.len() + 1);
    let mut prev = 0usize;
    for &pos in &splits {
        result.push(&expr[prev..pos]);
        prev = pos + 5;
    }
    result.push(&expr[prev..]);
    result
}

/// Try to parse a numeric value, accepting scientific notation such as `3e9`.
fn parse_value(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

/// Convert a `>=` / `<=` / `>` / `<` operator string to its ES equivalent.
fn op_to_es(op: &str) -> Option<&'static str> {
    match op {
        ">=" => Some("gte"),
        "<=" => Some("lte"),
        ">" => Some("gt"),
        "<" => Some("lt"),
        _ => None,
    }
}

/// Parse a single term token.
fn parse_single_term(term: &str) -> Result<FilterTerm, String> {
    // Try agg_term first: `agg_fn(field) op value`
    if let Some(result) = try_parse_agg_term(term) {
        return result;
    }
    // Then: `field op value` or `field=value`
    parse_simple_term(term)
}

/// Attempt to parse `agg_fn(field) OP value`.
///
/// Returns `None` when the term does not start with a known agg prefix.
fn try_parse_agg_term(term: &str) -> Option<Result<FilterTerm, String>> {
    let agg_fns = ["min", "max", "mean", "median"];
    let term_lc = term.to_lowercase();

    let agg = agg_fns.iter().find(|&&a| term_lc.starts_with(a))?;
    let after_agg = &term[agg.len()..].trim_start();
    if !after_agg.starts_with('(') {
        return None;
    }
    let close = after_agg.find(')')?;
    let field = after_agg[1..close].trim().to_string();
    if field.is_empty() {
        return Some(Err(format!("empty field name in agg term: {term}")));
    }

    let rest = after_agg[close + 1..].trim();
    let (op_str, val_str) = split_op(rest)?;
    let es_op = match op_to_es(op_str) {
        Some(o) => o,
        None => return Some(Err(format!("unknown operator '{op_str}' in: {term}"))),
    };
    let value = match parse_value(val_str) {
        Some(v) => v,
        None => return Some(Err(format!("invalid numeric value '{val_str}' in: {term}"))),
    };

    Some(Ok(FilterTerm::AggRange {
        field,
        agg: agg.to_string(),
        op: es_op,
        value,
    }))
}

/// Parse `field OP value` or `field=value`.
fn parse_simple_term(term: &str) -> Result<FilterTerm, String> {
    // Ordered so `>=` and `<=` are tried before `>` / `<`.
    for op_str in &[">=", "<=", ">", "<"] {
        if let Some(pos) = term.find(op_str) {
            let field = term[..pos].trim().to_string();
            let val_str = term[pos + op_str.len()..].trim();
            if field.is_empty() {
                return Err(format!("empty field name in: {term}"));
            }
            let es_op = op_to_es(op_str).unwrap();
            let value = parse_value(val_str)
                .ok_or_else(|| format!("invalid numeric value '{val_str}' in: {term}"))?;
            return Ok(FilterTerm::NumericRange {
                field,
                op: es_op,
                value,
            });
        }
    }

    // Keyword equality: `field=value`
    if let Some(pos) = term.find('=') {
        let field = term[..pos].trim().to_string();
        let value = term[pos + 1..].trim().to_string();
        if field.is_empty() {
            return Err(format!("empty field name in: {term}"));
        }
        return Ok(FilterTerm::KeywordMatch { field, value });
    }

    // Bare field name (no operator): match any document that has this attribute.
    // Mirrors V2's auto-excludeMissing when x is a bare field like `assembly_span`.
    let is_bare_identifier = !term.is_empty()
        && term
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    if is_bare_identifier {
        return Ok(FilterTerm::Exists {
            field: term.to_string(),
        });
    }

    Err(format!("could not parse filter term: '{term}'"))
}

/// Split a `OP value` suffix into `(op_str, val_str)`.
///
/// Tries multi-char operators before single-char ones.
fn split_op(s: &str) -> Option<(&'static str, &str)> {
    for &op in &[">=", "<=", ">", "<"] {
        if let Some(rest) = s.strip_prefix(op) {
            return Some((op, rest));
        }
    }
    None
}

/// Build `{"range": {"<field>": {"<op>": <value>}}}` without using the `json!` key-variable
/// limitation (which cannot accept a `&&str`).
fn make_range(field: &str, op: &str, value: f64) -> Value {
    let mut op_map = serde_json::Map::new();
    op_map.insert(op.to_string(), Value::from(value));
    let mut field_map = serde_json::Map::new();
    field_map.insert(field.to_string(), Value::Object(op_map));
    json!({ "range": Value::Object(field_map) })
}

/// Compile a single [`FilterTerm`] into a nested ES clause.
fn term_to_nested_query(term: &FilterTerm) -> Value {
    match term {
        FilterTerm::NumericRange { field, op, value } => {
            let inner = json!({
                "bool": {
                    "should": [
                        make_range("attributes.long_value", op, *value),
                        make_range("attributes.float_value", op, *value),
                        make_range("attributes.half_float_value", op, *value)
                    ],
                    "minimum_should_match": 1
                }
            });
            nested_attr_query(field, inner)
        }
        FilterTerm::AggRange {
            field,
            agg,
            op,
            value,
        } => {
            let sub_field = format!("attributes.{agg}");
            let inner = make_range(&sub_field, op, *value);
            nested_attr_query(field, inner)
        }
        FilterTerm::KeywordMatch { field, value } => {
            let inner = json!({ "term": { "attributes.keyword_value": value } });
            nested_attr_query(field, inner)
        }
        FilterTerm::Exists { field } => {
            // Just match on the key — any document that has this attribute.
            json!({
                "nested": {
                    "path": "attributes",
                    "query": {
                        "term": { "attributes.key": field }
                    }
                }
            })
        }
    }
}

/// Wrap `inner` in the standard nested-attributes key filter.
fn nested_attr_query(field: &str, inner: Value) -> Value {
    json!({
        "nested": {
            "path": "attributes",
            "query": {
                "bool": {
                    "must": [
                        { "term": { "attributes.key": field } },
                        inner
                    ]
                }
            }
        }
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numeric_gt() {
        let terms = parse_filter_string("genome_size>3000000000").unwrap();
        assert_eq!(terms.len(), 1);
        assert!(matches!(
            &terms[0],
            FilterTerm::NumericRange { field, op, .. }
            if field == "genome_size" && *op == "gt"
        ));
    }

    #[test]
    fn test_parse_numeric_gte() {
        let terms = parse_filter_string("genome_size>=1e9").unwrap();
        assert!(matches!(
            &terms[0],
            FilterTerm::NumericRange { op, .. } if *op == "gte"
        ));
    }

    #[test]
    fn test_parse_keyword() {
        let terms = parse_filter_string("assembly_level=Chromosome").unwrap();
        assert!(matches!(
            &terms[0],
            FilterTerm::KeywordMatch { field, value }
            if field == "assembly_level" && value == "Chromosome"
        ));
    }

    #[test]
    fn test_parse_agg_lt() {
        let terms = parse_filter_string("min(c_value)<3").unwrap();
        assert!(matches!(
            &terms[0],
            FilterTerm::AggRange { field, agg, op, .. }
            if field == "c_value" && agg == "min" && *op == "lt"
        ));
    }

    #[test]
    fn test_parse_agg_median() {
        let terms = parse_filter_string("median(genome_size)<=2000000000").unwrap();
        assert!(matches!(
            &terms[0],
            FilterTerm::AggRange { agg, op, .. }
            if agg == "median" && *op == "lte"
        ));
    }

    #[test]
    fn test_parse_compound_and() {
        let terms = parse_filter_string("genome_size>1e9 AND assembly_level=Chromosome").unwrap();
        assert_eq!(terms.len(), 2);
        assert!(matches!(&terms[0], FilterTerm::NumericRange { .. }));
        assert!(matches!(&terms[1], FilterTerm::KeywordMatch { .. }));
    }

    #[test]
    fn test_parse_and_case_insensitive() {
        let terms = parse_filter_string("genome_size>1e9 and assembly_level=Chromosome").unwrap();
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_parse_scientific_notation() {
        let terms = parse_filter_string("genome_size>3e9").unwrap();
        if let FilterTerm::NumericRange { value, .. } = &terms[0] {
            assert!((value - 3_000_000_000.0).abs() < 1.0);
        } else {
            panic!("expected NumericRange");
        }
    }

    #[test]
    fn test_empty_expr_returns_base_query() {
        let base = json!({ "match_all": {} });
        let result = filter_expr_to_es_query("", &base).unwrap();
        assert_eq!(result, base);
    }

    #[test]
    fn test_build_numeric_nested_structure() {
        let terms = parse_filter_string("genome_size>3e9").unwrap();
        let clause = build_nested_attribute_query(&terms);
        assert_eq!(clause["nested"]["path"], "attributes");
        let must = &clause["nested"]["query"]["bool"]["must"];
        assert_eq!(must[0]["term"]["attributes.key"], "genome_size");
    }

    #[test]
    fn test_build_agg_nested_structure() {
        let terms = parse_filter_string("min(c_value)<3").unwrap();
        let clause = build_nested_attribute_query(&terms);
        assert_eq!(clause["nested"]["path"], "attributes");
        let must = &clause["nested"]["query"]["bool"]["must"];
        assert_eq!(must[0]["term"]["attributes.key"], "c_value");
        assert!(must[1]["range"]["attributes.min"].is_object());
    }

    #[test]
    fn test_build_keyword_nested_structure() {
        let terms = parse_filter_string("assembly_level=Chromosome").unwrap();
        let clause = build_nested_attribute_query(&terms);
        let must = &clause["nested"]["query"]["bool"]["must"];
        assert_eq!(must[1]["term"]["attributes.keyword_value"], "Chromosome");
    }

    #[test]
    fn test_filter_expr_to_es_query_wraps_base() {
        let base = json!({ "term": { "taxon_rank": "species" } });
        let result = filter_expr_to_es_query("genome_size>1e9", &base).unwrap();
        let must = result["bool"]["must"].as_array().unwrap();
        assert_eq!(must.len(), 2);
        assert_eq!(must[0], base);
    }

    #[test]
    fn test_compound_and_produces_bool_must() {
        let terms = parse_filter_string("genome_size>1e9 AND assembly_level=Chromosome").unwrap();
        let clause = build_nested_attribute_query(&terms);
        // Two terms → wrapped in bool.must
        let inner = clause["bool"]["must"].as_array().unwrap();
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn test_parse_bare_field_name() {
        let terms = parse_filter_string("assembly_span").unwrap();
        assert_eq!(terms.len(), 1);
        assert!(matches!(
            &terms[0],
            FilterTerm::Exists { field } if field == "assembly_span"
        ));
    }

    #[test]
    fn test_bare_field_nested_query() {
        let terms = parse_filter_string("assembly_span").unwrap();
        let clause = build_nested_attribute_query(&terms);
        assert_eq!(clause["nested"]["path"], "attributes");
        assert_eq!(
            clause["nested"]["query"]["term"]["attributes.key"],
            "assembly_span"
        );
    }

    #[test]
    fn test_bare_field_with_hyphen() {
        let terms = parse_filter_string("chromosome-number").unwrap();
        assert!(matches!(
            &terms[0],
            FilterTerm::Exists { field } if field == "chromosome-number"
        ));
    }
}
