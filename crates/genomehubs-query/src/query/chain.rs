//! Named sub-query chain resolution (cross-query / `queryA.field` substitution).
//!
//! Implements the v3 equivalent of v2's `chainQueries()` function.
//!
//! # How it works
//!
//! 1. A [`SearchQuery`] may carry a `named_queries` map, where each entry
//!    defines a sub-query to execute before the main query.
//! 2. Attribute values in the main query may contain **chain references** of
//!    the form `queryA.field` or `queryA.summary(field)`, pointing to field
//!    values from the corresponding named sub-query.
//! 3. [`collect_chain_refs`] walks the main query and finds all such
//!    references.
//! 4. The API layer executes the referenced sub-queries, then calls
//!    [`resolve_chain_refs`] to substitute the fetched values into the main
//!    query in place.
//!
//! # YAML example
//!
//! ```yaml
//! index: taxon
//! taxa: [Eukaryota]
//! taxon_filter_type: tree
//! attributes:
//!   - name: taxon_id
//!     operator: eq
//!     value: queryA.taxon_id   # chain reference
//! named_queries:
//!   queryA:
//!     index: assembly
//!     filter_expr: "assembly_span>1000000000 AND assembly_level=Chromosome"
//!     limit: 200
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{AttributeValue, SearchIndex};

// ── NamedQuerySpec ────────────────────────────────────────────────────────────

/// A named sub-query used for chain substitution in a [`SearchQuery`].
///
/// Defined under `named_queries` in the YAML block. The sub-query executes
/// before the main query; its results supply values that are substituted
/// into main-query attribute values via dot-notation references such as
/// `value: queryA.taxon_id` or `value: queryA.mean(genome_size)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedQuerySpec {
    /// Target index. `None` → inherit the parent query's `index` value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<SearchIndex>,

    /// Filter expression string (same syntax as arc/tree `filter_expr` params).
    ///
    /// Example: `"assembly_span>1000000000 AND assembly_level=Chromosome"`
    #[serde(default)]
    pub filter_expr: String,

    /// Whether to scope the sub-query inside the parent query's taxon tree.
    ///
    /// Default (when `None`): `true` if `index` matches the parent's index,
    /// `false` for cross-index sub-queries.  Set explicitly to override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit_scope: Option<bool>,

    /// Maximum number of results to fetch (default 500; hard ceiling 10,000).
    ///
    /// Results above 10,000 return a [`ChainError::TooManyHits`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl NamedQuerySpec {
    /// The default result limit when none is specified.
    pub const DEFAULT_LIMIT: usize = 500;

    /// Hard ceiling for result fetching.
    pub const MAX_LIMIT: usize = 10_000;

    /// Effective limit: the configured value clamped to [`MAX_LIMIT`].
    pub fn effective_limit(&self) -> usize {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .min(Self::MAX_LIMIT)
    }

    /// Construct from the v2 URL-parameter string format.
    ///
    /// Format: `[index--]filter_expr`
    ///
    /// | Input string                          | Result                                    |
    /// |---------------------------------------|-------------------------------------------|
    /// | `"assembly--assembly_span>1e9"`       | `index: Assembly, filter_expr: "span>1G"` |
    /// | `"genome_size>0 AND gc_percent<60"`   | `index: None, filter_expr: "…"`           |
    ///
    /// Returns `None` when the index prefix (if present) is not recognised.
    pub fn from_legacy_string(s: &str) -> Option<Self> {
        let (index, filter_expr) = match s.split_once("--") {
            Some((idx_str, rest)) => {
                let index = match idx_str.trim() {
                    "assembly" => SearchIndex::Assembly,
                    "sample" => SearchIndex::Sample,
                    "taxon" => SearchIndex::Taxon,
                    _ => return None,
                };
                (Some(index), rest.trim().to_string())
            }
            None => (None, s.trim().to_string()),
        };
        Some(Self {
            index,
            filter_expr,
            inherit_scope: None,
            limit: None,
        })
    }
}

// ── ChainRef ──────────────────────────────────────────────────────────────────

/// A parsed chain reference extracted from an [`AttributeValue`].
///
/// Matches attribute value strings of the form `key.field` or
/// `key.summary(field)`.
///
/// The key must start with a lowercase ASCII letter so that ordinary field
/// names (`genome_size`) can never be mistaken for chain references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainRef {
    /// Named query key, e.g. `"queryA"`.
    pub key: String,
    /// Field to extract from sub-query results, e.g. `"taxon_id"`.
    pub field: String,
    /// Aggregation summary to apply. `"value"` means raw field values (default).
    pub summary: String,
}

impl ChainRef {
    /// Try to parse a string as a chain reference.
    ///
    /// Returns `None` when the string does not match the `key.field` or
    /// `key.summary(field)` pattern.
    ///
    /// # Examples
    /// ```
    /// # use genomehubs_query::query::chain::ChainRef;
    /// let r = ChainRef::parse("queryA.taxon_id").unwrap();
    /// assert_eq!(r.key, "queryA");
    /// assert_eq!(r.field, "taxon_id");
    /// assert_eq!(r.summary, "value");
    ///
    /// let r = ChainRef::parse("queryB.mean(genome_size)").unwrap();
    /// assert_eq!(r.summary, "mean");
    /// assert_eq!(r.field, "genome_size");
    ///
    /// assert!(ChainRef::parse("genome_size").is_none());
    /// assert!(ChainRef::parse("QueryA.field").is_none()); // uppercase start → not a chain ref
    /// ```
    pub fn parse(s: &str) -> Option<Self> {
        // Must contain a dot separator.
        let dot = s.find('.')?;
        let key = &s[..dot];
        let rest = &s[dot + 1..];

        // Key must start with a lowercase letter and be purely alphanumeric.
        let mut key_chars = key.chars();
        if !key_chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
            return None;
        }
        if !key_chars.all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        if key.is_empty() || rest.is_empty() {
            return None;
        }

        // Detect `summary(field)` form.
        if let Some(paren) = rest.find('(') {
            let summary = &rest[..paren];
            let field = rest[paren + 1..].trim_end_matches(')');
            if summary.is_empty() || field.is_empty() {
                return None;
            }
            Some(Self {
                key: key.to_string(),
                field: field.to_string(),
                summary: summary.to_string(),
            })
        } else {
            // Plain `key.field` form — summary defaults to "value".
            Some(Self {
                key: key.to_string(),
                field: rest.to_string(),
                summary: "value".to_string(),
            })
        }
    }
}

// ── ChainError ────────────────────────────────────────────────────────────────

/// Errors produced during chain-query processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    /// A reference like `queryA.field` was found but `queryA` is not defined
    /// in `named_queries`.
    UndefinedQuery {
        /// The undefined key that was referenced.
        key: String,
    },

    /// A sub-query returned more results than the configured limit.
    TooManyHits {
        /// The key whose sub-query exceeded the limit.
        key: String,
        /// Actual hit count returned.
        count: usize,
        /// The configured limit.
        limit: usize,
    },

    /// The sub-query itself failed (network error, ES error, etc.).
    SubQueryFailed {
        /// The key whose sub-query failed.
        key: String,
        /// Human-readable error message.
        message: String,
    },
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainError::UndefinedQuery { key } => {
                write!(f, "chain reference uses undefined query key: {key:?}")
            }
            ChainError::TooManyHits { key, count, limit } => {
                write!(
                    f,
                    "sub-query {key:?} returned {count} results, exceeding limit {limit}"
                )
            }
            ChainError::SubQueryFailed { key, message } => {
                write!(f, "sub-query {key:?} failed: {message}")
            }
        }
    }
}

// ── collect_chain_refs ────────────────────────────────────────────────────────

/// Walk all attribute values in a [`SearchQuery`] and collect every chain
/// reference found.
///
/// Returns one entry per occurrence (not per unique key). The caller should
/// deduplicate by key to know which sub-queries to execute.
pub fn collect_chain_refs(
    attributes: &[super::super::query::attributes::Attribute],
) -> Vec<ChainRef> {
    attributes
        .iter()
        .filter_map(|attr| {
            if let Some(AttributeValue::Single(s)) = &attr.value {
                ChainRef::parse(s)
            } else {
                None
            }
        })
        .collect()
}

// ── resolve_chain_refs ────────────────────────────────────────────────────────

/// Substitute resolved values into a list of attributes in place.
///
/// For each attribute whose `value` is a chain reference, replaces it with
/// an [`AttributeValue::List`] of the pre-fetched strings. The operator is
/// set to [`AttributeOperator::Eq`] when currently `None`.
///
/// # Errors
///
/// Returns [`ChainError::UndefinedQuery`] if a referenced key is not present
/// in `resolved`, or [`ChainError::TooManyHits`] if the resolved value list
/// for a key exceeds the spec's configured limit.
pub fn resolve_chain_refs(
    attributes: &mut [super::super::query::attributes::Attribute],
    resolved: &HashMap<String, Vec<String>>,
    specs: &HashMap<String, NamedQuerySpec>,
) -> Result<(), ChainError> {
    use super::AttributeOperator;

    for attr in attributes.iter_mut() {
        let chain_ref = match &attr.value {
            Some(AttributeValue::Single(s)) => match ChainRef::parse(s) {
                Some(r) => r,
                None => continue,
            },
            _ => continue,
        };

        let values = resolved
            .get(&chain_ref.key)
            .ok_or_else(|| ChainError::UndefinedQuery {
                key: chain_ref.key.clone(),
            })?;

        // Enforce the per-spec limit.
        if let Some(spec) = specs.get(&chain_ref.key) {
            let limit = spec.effective_limit();
            if values.len() > limit {
                return Err(ChainError::TooManyHits {
                    key: chain_ref.key.clone(),
                    count: values.len(),
                    limit,
                });
            }
        }

        attr.value = Some(AttributeValue::List(values.clone()));
        if attr.operator.is_none() {
            attr.operator = Some(AttributeOperator::Eq);
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::attributes::AttributeOperator;

    // ── ChainRef::parse ───────────────────────────────────────────────────────

    #[test]
    fn chain_ref_parses_plain_field() {
        let r = ChainRef::parse("queryA.taxon_id").unwrap();
        assert_eq!(r.key, "queryA");
        assert_eq!(r.field, "taxon_id");
        assert_eq!(r.summary, "value");
    }

    #[test]
    fn chain_ref_parses_summary_field() {
        let r = ChainRef::parse("queryB.mean(genome_size)").unwrap();
        assert_eq!(r.key, "queryB");
        assert_eq!(r.summary, "mean");
        assert_eq!(r.field, "genome_size");
    }

    #[test]
    fn chain_ref_rejects_plain_field_name() {
        assert!(ChainRef::parse("genome_size").is_none());
    }

    #[test]
    fn chain_ref_rejects_uppercase_key() {
        // Uppercase start → ordinary field name, not a chain ref.
        assert!(ChainRef::parse("QueryA.field").is_none());
    }

    #[test]
    fn chain_ref_rejects_empty_key() {
        assert!(ChainRef::parse(".field").is_none());
    }

    #[test]
    fn chain_ref_rejects_empty_field() {
        assert!(ChainRef::parse("queryA.").is_none());
    }

    // ── NamedQuerySpec::from_legacy_string ────────────────────────────────────

    #[test]
    fn named_query_spec_parses_cross_index() {
        let spec =
            NamedQuerySpec::from_legacy_string("assembly--assembly_span>1000000000").unwrap();
        assert_eq!(spec.index, Some(SearchIndex::Assembly));
        assert_eq!(spec.filter_expr, "assembly_span>1000000000");
    }

    #[test]
    fn named_query_spec_parses_same_index() {
        let spec = NamedQuerySpec::from_legacy_string("genome_size>0 AND gc_percent<60").unwrap();
        assert!(spec.index.is_none());
        assert_eq!(spec.filter_expr, "genome_size>0 AND gc_percent<60");
    }

    #[test]
    fn named_query_spec_rejects_unknown_index() {
        assert!(NamedQuerySpec::from_legacy_string("unknown_index--field>1").is_none());
    }

    #[test]
    fn named_query_spec_taxon_index() {
        let spec = NamedQuerySpec::from_legacy_string("taxon--tax_tree(Rodentia)").unwrap();
        assert_eq!(spec.index, Some(SearchIndex::Taxon));
        assert_eq!(spec.filter_expr, "tax_tree(Rodentia)");
    }

    // ── collect_chain_refs ────────────────────────────────────────────────────

    #[test]
    fn collect_finds_chain_refs() {
        use crate::query::attributes::Attribute;

        let attrs = vec![
            Attribute {
                name: "taxon_id".to_string(),
                operator: Some(AttributeOperator::Eq),
                value: Some(AttributeValue::Single("queryA.taxon_id".to_string())),
                modifier: vec![],
            },
            Attribute {
                name: "genome_size".to_string(),
                operator: Some(AttributeOperator::Gt),
                value: Some(AttributeValue::Single("1000000000".to_string())),
                modifier: vec![],
            },
        ];

        let refs = collect_chain_refs(&attrs);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].key, "queryA");
        assert_eq!(refs[0].field, "taxon_id");
    }

    // ── resolve_chain_refs ────────────────────────────────────────────────────

    #[test]
    fn resolve_substitutes_values() {
        use crate::query::attributes::Attribute;

        let mut attrs = vec![Attribute {
            name: "taxon_id".to_string(),
            operator: None,
            value: Some(AttributeValue::Single("queryA.taxon_id".to_string())),
            modifier: vec![],
        }];

        let mut resolved = HashMap::new();
        resolved.insert(
            "queryA".to_string(),
            vec!["9606".to_string(), "10090".to_string()],
        );
        let specs = HashMap::new();

        resolve_chain_refs(&mut attrs, &resolved, &specs).unwrap();

        assert_eq!(
            attrs[0].value,
            Some(AttributeValue::List(vec![
                "9606".to_string(),
                "10090".to_string()
            ]))
        );
        // operator defaults to Eq when it was None
        assert_eq!(attrs[0].operator, Some(AttributeOperator::Eq));
    }

    #[test]
    fn resolve_errors_on_undefined_key() {
        use crate::query::attributes::Attribute;

        let mut attrs = vec![Attribute {
            name: "taxon_id".to_string(),
            operator: None,
            value: Some(AttributeValue::Single("queryZ.taxon_id".to_string())),
            modifier: vec![],
        }];

        let resolved = HashMap::new();
        let specs = HashMap::new();

        let err = resolve_chain_refs(&mut attrs, &resolved, &specs).unwrap_err();
        assert_eq!(
            err,
            ChainError::UndefinedQuery {
                key: "queryZ".to_string()
            }
        );
    }

    #[test]
    fn resolve_errors_on_too_many_hits() {
        use crate::query::attributes::Attribute;

        let mut attrs = vec![Attribute {
            name: "taxon_id".to_string(),
            operator: None,
            value: Some(AttributeValue::Single("queryA.taxon_id".to_string())),
            modifier: vec![],
        }];

        let mut resolved = HashMap::new();
        // Inject 3 values
        resolved.insert(
            "queryA".to_string(),
            vec!["1".to_string(), "2".to_string(), "3".to_string()],
        );

        // Spec with limit = 2
        let mut specs = HashMap::new();
        specs.insert(
            "queryA".to_string(),
            NamedQuerySpec {
                index: None,
                filter_expr: String::new(),
                inherit_scope: None,
                limit: Some(2),
            },
        );

        let err = resolve_chain_refs(&mut attrs, &resolved, &specs).unwrap_err();
        assert_eq!(
            err,
            ChainError::TooManyHits {
                key: "queryA".to_string(),
                count: 3,
                limit: 2
            }
        );
    }
}
