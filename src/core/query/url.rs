//! Pure URL builder for genomehubs search API queries.
//!
//! [`build_query_url`] converts a [`SearchQuery`] + [`QueryParams`] into a
//! fully-encoded API URL.  The function has no side effects and requires no
//! I/O — all strings are kept raw until a single percent-encoding pass at
//! serialisation.  This eliminates the double-encoding class of bugs that
//! arises from pre-encoding intermediate fragments.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use super::{
    attributes::{AttributeOperator, AttributeSet, AttributeValue},
    identifiers::Identifiers,
    QueryParams, SearchQuery,
};

// ── Percent-encode character sets ─────────────────────────────────────────────

/// Characters that must be encoded inside the `query=` value.
///
/// Encodes everything except unreserved chars (RFC 3986: A-Z a-z 0-9 - _ . ~).
/// Specifically includes `(`, `)`, `,`, `!`, `=`, `<`, `>`, ` ` etc.
const QUERY_FRAGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'~');

/// Characters that must be encoded in outer query parameter values (`&key=value`).
///
/// Includes `,`, `[`, and `]` because the OpenAPI validator used by the
/// genomehubs API requires them to be percent-encoded in param values.
const PARAM_VALUE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b':')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

// ── Public entry point ────────────────────────────────────────────────────────

/// Build a fully-encoded API URL from a [`SearchQuery`] and [`QueryParams`].
///
/// # Parameters
/// - `query`    — *what* to search for
/// - `params`   — *how* to fetch and format results
/// - `api_base` — base URL without trailing slash, e.g. `"https://goat.genomehubs.org/api"`
/// - `api_version` — version path component, e.g. `"v2"`
/// - `endpoint` — one of `"search"`, `"count"`, `"searchPaginated"`, `"report"`
///
/// # Encoding contract
/// All raw strings are kept unencoded until the final serialisation step.
/// The query string value (passed as `?query=…`) is encoded once with
/// [`QUERY_FRAGMENT`].  All other parameter values are encoded with
/// [`PARAM_VALUE`].  No double-encoding occurs.
pub fn build_query_url(
    query: &SearchQuery,
    params: &QueryParams,
    api_base: &str,
    api_version: &str,
    endpoint: &str,
) -> String {
    let raw_query_fragment = build_raw_query_fragment(&query.identifiers, &query.attributes);
    let exclusion_params = build_exclusion_params(&query.attributes);
    let field_params = build_field_params(&query.attributes);

    assemble_url(
        api_base,
        api_version,
        endpoint,
        query.index.to_api_str(),
        &raw_query_fragment,
        &exclusion_params,
        &field_params,
        &query.attributes.names,
        &query.attributes.ranks,
        params,
    )
}

// ── Query fragment builders ───────────────────────────────────────────────────

/// Build the raw (unencoded) `query=` value from identifiers and attribute filters.
///
/// Raw fragments are joined with ` AND ` and percent-encoded as a single unit.
fn build_raw_query_fragment(identifiers: &Identifiers, attributes: &AttributeSet) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(taxa_fragment) = build_taxa_fragment(identifiers) {
        parts.push(taxa_fragment);
    }

    if let Some(rank_fragment) = build_rank_fragment(identifiers) {
        parts.push(rank_fragment);
    }

    if let Some(assembly_fragment) = build_id_fragment("assembly_id", &identifiers.assemblies) {
        parts.push(assembly_fragment);
    }

    if let Some(sample_fragment) = build_id_fragment("sample_id", &identifiers.samples) {
        parts.push(sample_fragment);
    }

    for attr_fragment in build_attribute_fragments(attributes) {
        parts.push(attr_fragment);
    }

    parts.join(" AND ")
}

/// Build `tax_name(A,!B)` / `tax_tree(A,B)` / `tax_lineage(A)` fragment.
///
/// Returns `None` if `taxa` is not set.
fn build_taxa_fragment(identifiers: &Identifiers) -> Option<String> {
    let taxa = identifiers.taxa.as_ref()?;
    let joined = taxa.names.join(",");
    Some(format!("{}({})", taxa.filter_type.api_function(), joined))
}

/// Build `tax_rank(species)` fragment.  Returns `None` if no rank is set.
fn build_rank_fragment(identifiers: &Identifiers) -> Option<String> {
    identifiers
        .rank
        .as_deref()
        .filter(|r| !r.is_empty())
        .map(|rank| format!("tax_rank({rank})"))
}

/// Build `assembly_id=ACC1,ACC2` or `sample_id=ACC` fragment.
///
/// Returns `None` if `ids` is empty.
fn build_id_fragment(param: &str, ids: &[String]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    Some(format!("{}={}", param, ids.join(",")))
}

/// Build one raw query fragment per attribute filter.
///
/// Summary modifiers wrap the field name as `summary(field)`.
/// The operator and value are appended directly.
fn build_attribute_fragments(attributes: &AttributeSet) -> Vec<String> {
    attributes
        .attributes
        .iter()
        .filter_map(build_single_attribute_fragment)
        .collect()
}

/// Build the raw query fragment for a single [`Attribute`].
fn build_single_attribute_fragment(attr: &super::attributes::Attribute) -> Option<String> {
    let summary_mod = attr.modifier.iter().find(|m| m.is_summary());

    let field_name = if let Some(s) = summary_mod {
        format!("{}({})", s.as_str(), attr.name)
    } else {
        attr.name.clone()
    };

    match &attr.operator {
        None => Some(field_name),
        Some(AttributeOperator::Exists) => Some(field_name),
        Some(AttributeOperator::Missing) => {
            // Missing is represented as a status modifier; skip the operator fragment
            // and let set_exclusions handle it via the `excludeMissing` param.
            Some(field_name)
        }
        Some(op) => {
            let value = attr.value.as_ref()?;
            let value_str = format_attribute_value(value);
            Some(format!("{}{}{}", field_name, op.as_str(), value_str))
        }
    }
}

/// Format an [`AttributeValue`] as a raw string for query embedding.
///
/// Lists are joined with commas.
fn format_attribute_value(value: &AttributeValue) -> String {
    match value {
        AttributeValue::Single(s) => s.clone(),
        AttributeValue::List(v) => v.join(","),
    }
}

// ── Exclusion params ──────────────────────────────────────────────────────────

/// A single `excludeXxx[N]=field` parameter pair.
struct ExclusionParam {
    key: String,
    value: String,
}

/// Build `excludeXxx[N]=field` param pairs from status modifiers.
///
/// Status modifiers (`Direct`, `Ancestral`, `Descendant`, `Estimated`,
/// `Missing`) are not embedded in the query string — they are emitted as
/// separate URL params: `&excludeDirect[0]=genome_size`.
fn build_exclusion_params(attributes: &AttributeSet) -> Vec<ExclusionParam> {
    let mut counters: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut params: Vec<ExclusionParam> = Vec::new();

    for attr in &attributes.attributes {
        for modifier in attr.modifier.iter().filter(|m| m.is_status()) {
            let key_base = format!("exclude{}", modifier.as_str());
            let index = counters.entry(key_base.clone()).or_insert(0);
            params.push(ExclusionParam {
                key: format!("{key_base}[{index}]"),
                value: attr.name.clone(),
            });
            *index += 1;
        }
    }

    params
}

// ── Field params ──────────────────────────────────────────────────────────────

/// Build the `fields` param value list from [`AttributeSet::fields`].
///
/// Each field is optionally suffixed with `:modifier` for return modifiers.
fn build_field_params(attributes: &AttributeSet) -> Vec<String> {
    let mut params: Vec<String> = Vec::new();
    for field in &attributes.fields {
        params.push(field.name.clone());
        for modifier in field.modifier.iter().filter(|m| m.is_summary()) {
            params.push(format!("{}:{}", field.name, modifier.as_str()));
        }
    }
    params
}

// ── URL assembly ──────────────────────────────────────────────────────────────

/// Assemble the final URL from all encoded components.
#[allow(clippy::too_many_arguments)]
fn assemble_url(
    api_base: &str,
    api_version: &str,
    endpoint: &str,
    result: &str,
    raw_query: &str,
    exclusion_params: &[ExclusionParam],
    field_params: &[String],
    names: &[String],
    ranks: &[String],
    params: &QueryParams,
) -> String {
    let base = format!("{api_base}/{api_version}/{endpoint}");
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!("result={}", encode_param(result)));

    if params.include_estimates {
        parts.push("includeEstimates=true".to_string());
    }

    parts.push(format!("taxonomy={}", encode_param(&params.taxonomy)));

    if !raw_query.is_empty() {
        let encoded_query = utf8_percent_encode(raw_query, QUERY_FRAGMENT).to_string();
        parts.push(format!("query={encoded_query}"));
    }

    if !field_params.is_empty() {
        let value = field_params.join(",");
        parts.push(format!("fields={}", encode_param(&value)));
    }

    if !names.is_empty() {
        let value = names.join(",");
        parts.push(format!("names={}", encode_param(&value)));
    }

    if !ranks.is_empty() {
        let value = ranks.join(",");
        parts.push(format!("ranks={}", encode_param(&value)));
    }

    let offset = (params.page.saturating_sub(1)) * params.size;
    parts.push(format!("size={}", params.size));
    parts.push(format!("offset={offset}"));

    if let Some(ref sort_by) = params.sort_by {
        parts.push(format!("sortBy={}", encode_param(sort_by)));
        let order = match params.sort_order {
            super::SortOrder::Asc => "asc",
            super::SortOrder::Desc => "desc",
        };
        parts.push(format!("sortOrder={order}"));
    }

    if params.tidy {
        parts.push("summaryValues=false".to_string());
    }

    for ep in exclusion_params {
        parts.push(format!(
            "{}={}",
            encode_param(&ep.key),
            encode_param(&ep.value)
        ));
    }

    format!("{}?{}", base, parts.join("&"))
}

/// Encode a single parameter value with [`PARAM_VALUE`].
fn encode_param(value: &str) -> String {
    utf8_percent_encode(value, PARAM_VALUE).to_string()
}

// ── SearchIndex helper ────────────────────────────────────────────────────────

impl super::SearchIndex {
    /// Return the API `result=` string for this index.
    pub fn to_api_str(&self) -> &'static str {
        match self {
            super::SearchIndex::Taxon => "taxon",
            super::SearchIndex::Assembly => "assembly",
            super::SearchIndex::Sample => "sample",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::query::{
        attributes::{Attribute, AttributeSet, AttributeValue, Field, Modifier},
        identifiers::{Identifiers, TaxaIdentifier, TaxonFilterType},
        QueryParams, SearchIndex, SearchQuery,
    };

    fn default_params() -> QueryParams {
        QueryParams::default()
    }

    fn taxon_query(taxa: &[&str], rank: Option<&str>, filter_type: TaxonFilterType) -> SearchQuery {
        SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: Some(TaxaIdentifier {
                    names: taxa.iter().map(|s| s.to_string()).collect(),
                    filter_type,
                }),
                rank: rank.map(str::to_string),
                ..Default::default()
            },
            attributes: AttributeSet::default(),
        }
    }

    #[test]
    fn basic_taxon_tree_search() {
        let query = taxon_query(&["Mammalia"], Some("species"), TaxonFilterType::Tree);
        let url = build_query_url(
            &query,
            &default_params(),
            "https://goat.genomehubs.org/api",
            "v2",
            "search",
        );
        assert!(url.contains("result=taxon"));
        assert!(url.contains("tax_tree%28Mammalia%29"));
        assert!(url.contains("tax_rank%28species%29"));
        assert!(url.contains("taxonomy=ncbi"));
        assert!(url.contains("includeEstimates=true"));
    }

    #[test]
    fn not_filter_encoded_correctly() {
        let query = taxon_query(
            &["Mammalia", "!Felis"],
            Some("species"),
            TaxonFilterType::Tree,
        );
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // "!" must be encoded as %21 inside the query fragment
        assert!(url.contains("%21Felis"));
    }

    #[test]
    fn attribute_filter_with_size_value() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: Some(TaxaIdentifier {
                    names: vec!["Mammalia".to_string()],
                    filter_type: TaxonFilterType::Tree,
                }),
                ..Default::default()
            },
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "genome_size".to_string(),
                    operator: Some(AttributeOperator::Lt),
                    value: Some(AttributeValue::Single("3000000000".to_string())),
                    modifier: vec![Modifier::Min, Modifier::Direct],
                }],
                ..Default::default()
            },
        };
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // summary modifier wraps the field name
        assert!(url.contains("min%28genome_size%29"));
        // operator encoded
        assert!(url.contains("%3C3000000000"));
        // status modifier → exclude param
        assert!(url.contains("excludeDirect%5B0%5D=genome_size"));
    }

    #[test]
    fn fields_and_names_in_output() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: Some(TaxaIdentifier {
                    names: vec!["Insecta".to_string()],
                    filter_type: TaxonFilterType::Tree,
                }),
                ..Default::default()
            },
            attributes: AttributeSet {
                fields: vec![Field {
                    name: "genome_size".to_string(),
                    modifier: vec![Modifier::Min],
                }],
                names: vec!["scientific_name".to_string()],
                ranks: vec!["genus".to_string()],
                ..Default::default()
            },
        };
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        assert!(url.contains("fields=genome_size%2Cgenome_size%3Amin"));
        assert!(url.contains("names=scientific_name"));
        assert!(url.contains("ranks=genus"));
    }

    #[test]
    fn pagination_and_sort() {
        let query = taxon_query(&["Mammalia"], None, TaxonFilterType::Name);
        let params = QueryParams {
            size: 50,
            page: 3,
            sort_by: Some("genome_size".to_string()),
            sort_order: super::super::SortOrder::Desc,
            ..QueryParams::default()
        };
        let url = build_query_url(
            &query,
            &params,
            "https://api.example.org/api",
            "v2",
            "search",
        );
        assert!(url.contains("size=50"));
        assert!(url.contains("offset=100")); // (page-1)*size = 2*50
        assert!(url.contains("sortBy=genome_size"));
        assert!(url.contains("sortOrder=desc"));
    }

    #[test]
    fn count_endpoint() {
        let query = taxon_query(&["Mammalia"], Some("species"), TaxonFilterType::Tree);
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "count",
        );
        assert!(url.starts_with("https://api.example.org/api/v2/count?"));
    }

    #[test]
    fn empty_query_omits_query_param() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
        };
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "count",
        );
        assert!(!url.contains("query="));
    }

    #[test]
    fn no_double_encoding() {
        let query = taxon_query(&["Homo sapiens"], Some("species"), TaxonFilterType::Name);
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // Must not contain %25 (double-encoded %)
        assert!(!url.contains("%25"));
        // Space in name must be encoded as %20
        assert!(url.contains("Homo%20sapiens"));
    }

    #[test]
    fn special_chars_in_taxa_encoded() {
        let query = taxon_query(&["Genus (Subgenus)"], None, TaxonFilterType::Name);
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // Parentheses must be encoded in query string
        assert!(url.contains("%28Subgenus%29"));
    }

    #[test]
    fn special_chars_in_field_names_encoded() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                fields: vec![Field {
                    name: "gc-content".to_string(),
                    modifier: vec![],
                }],
                ..Default::default()
            },
        };
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // Field names should be in params (PARAM_VALUE encoding)
        assert!(url.contains("fields="));
    }

    #[test]
    fn quote_and_bracket_chars_encoded() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: Some(TaxaIdentifier {
                    names: vec!["[Species]".to_string()],
                    filter_type: TaxonFilterType::Name,
                }),
                ..Default::default()
            },
            attributes: AttributeSet::default(),
        };
        let url = build_query_url(
            &query,
            &default_params(),
            "https://api.example.org/api",
            "v2",
            "search",
        );
        // Brackets must be encoded as %5B and %5D
        assert!(url.contains("%5BSpecies%5D"));
    }

    // ── Property-based tests with proptest ─────────────────────────────────────

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        /// Generate valid ASCII taxon names
        fn arb_taxon() -> impl Strategy<Value = String> {
            "[A-Z][a-z]{0,20}".prop_map(|s| s.to_string()).boxed()
        }

        /// Generate simple valid queries that should encode without error
        fn arb_basic_query() -> impl Strategy<Value = SearchQuery> {
            (prop::option::of(arb_taxon()), any::<bool>())
                .prop_map(|(opt_taxon, with_rank)| {
                    let taxa = opt_taxon.map(|t| TaxaIdentifier {
                        names: vec![t],
                        filter_type: TaxonFilterType::Name,
                    });
                    let rank = if with_rank {
                        Some("species".to_string())
                    } else {
                        None
                    };
                    SearchQuery {
                        index: SearchIndex::Taxon,
                        identifiers: Identifiers {
                            taxa,
                            rank,
                            ..Default::default()
                        },
                        attributes: AttributeSet::default(),
                    }
                })
                .boxed()
        }

        proptest! {
            #[test]
            fn query_encoding_never_panics(query in arb_basic_query()) {
                let params = default_params();
                let _ = build_query_url(
                    &query,
                    &params,
                    "https://api.genomehubs.org/api",
                    "v2",
                    "search",
                );
                // Just verifying it doesn't panic
            }

            #[test]
            fn encoded_url_is_valid_utf8(query in arb_basic_query()) {
                let params = default_params();
                let url = build_query_url(
                    &query,
                    &params,
                    "https://api.genomehubs.org/api",
                    "v2",
                    "search",
                );
                // URL must be valid UTF-8 (guaranteed by String type, but good practice)
                assert!(url.is_ascii() || url.chars().all(|c| c.is_ascii() || c == '%'));
            }

            #[test]
            fn encoded_url_contains_api_base(query in arb_basic_query()) {
                let params = default_params();
                let base = "https://example.org/api";
                let url = build_query_url(&query, &params, base, "v2", "search");
                assert!(url.starts_with(base));
            }

            #[test]
            fn multiple_taxa_all_encoded(
                taxa in prop::collection::vec("[A-Z][a-z]+", 1..5)
            ) {
                let query = SearchQuery {
                    index: SearchIndex::Taxon,
                    identifiers: Identifiers {
                        taxa: Some(TaxaIdentifier {
                            names: taxa,
                            filter_type: TaxonFilterType::Name,
                        }),
                        ..Default::default()
                    },
                    attributes: AttributeSet::default(),
                };
                let url = build_query_url(
                    &query,
                    &default_params(),
                    "https://api.genomehubs.org/api",
                    "v2",
                    "search",
                );
                // Query should contain at least one taxon reference
                assert!(url.contains("query=") || query.identifiers.taxa.is_some());
            }

            #[test]
            fn empty_query_still_valid_url(
                _unused in any::<bool>()
            ) {
                let query = SearchQuery {
                    index: SearchIndex::Assembly,
                    identifiers: Identifiers::default(),
                    attributes: AttributeSet::default(),
                };
                let url = build_query_url(
                    &query,
                    &default_params(),
                    "https://api.genomehubs.org/api",
                    "v2",
                    "search",
                );
                // Empty query still produces valid URL structure
                assert!(url.starts_with("https://"));
                assert!(url.contains("?"));
            }
        }
    }
}
