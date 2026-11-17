//! Attribute and field name checks for search, count, batch, and report requests.
//!
//! Names are checked against the startup metadata cache, which holds one group of
//! attribute documents per index. The portable validator in
//! `genomehubs_query::validation` performs the matching. Only the keys that carry
//! attribute and field names are passed to it. Identifier and name-class checks
//! are excluded until their configuration is verified against live metadata.
//!
//! Report requests also check the names used in each named query's `filter_expr`
//! against that query's index, and check that every chain reference points to a
//! defined named query. Chain-reference values are not checked as attribute values.
//! Chain references on top-level keyword fields of their index are not checked against metadata.

use std::collections::{BTreeSet, HashMap};

use genomehubs_query::query::chain::{chain_ref_errors, ChainRef, NamedQuerySpec};
use genomehubs_query::query::{SearchIndex, SearchQuery};
use serde_json::{json, Map, Value as JsonValue};
use serde_yaml::{Mapping, Value as YamlValue};

use crate::es_metadata::MetadataCache;
use crate::report::filter_expr::{parse_filter_string, FilterTerm};
use crate::AppState;

/// Returns the cache group key (also the index name in the attributes index) for a search index.
pub fn index_key(index: &SearchIndex) -> &'static str {
    match index {
        SearchIndex::Taxon => "taxon",
        SearchIndex::Assembly => "assembly",
        SearchIndex::Sample => "sample",
        SearchIndex::Feature => "feature",
    }
}

/// Returns the attribute and field names checked by [`names_error`], or `None` when all are known.
///
/// Returns `None` when the cache has no group for `index`, so a missing cache never
/// rejects a request. Otherwise returns the validator messages joined with `"; "`.
pub async fn validate_names(
    state: &AppState,
    index: &SearchIndex,
    query_yaml: &str,
) -> Option<String> {
    let cache = state.cache.as_ref()?;
    let guard = cache.read().await;
    scoped_names_error(&guard, index_key(index), query_yaml)
}

/// Checks names in the top-level query and in every nested `queries` item.
///
/// Each scope is checked against the cached group for its own index. Returns the
/// messages joined with `"; "`, or `None` when every name is known.
pub fn scoped_names_error(
    cache: &MetadataCache,
    top_index: &'static str,
    query_yaml: &str,
) -> Option<String> {
    let messages: Vec<String> = name_scopes(query_yaml, top_index, cache)
        .into_iter()
        .filter_map(|(key, names_yaml)| group_names_error(&cache.attr_types, key, &names_yaml))
        .collect();
    (!messages.is_empty()).then(|| messages.join("; "))
}

/// Returns the names error for one scope, or `None` when the cache has no group for `key`.
fn group_names_error(attr_types: &JsonValue, key: &str, names_yaml: &str) -> Option<String> {
    let group = attr_types.get(key)?;
    names_error(group, key, names_yaml)
}

/// Returns `(index key, names YAML)` for the top-level query and each nested `queries` item.
fn name_scopes(
    query_yaml: &str,
    top_index: &'static str,
    cache: &MetadataCache,
) -> Vec<(&'static str, String)> {
    let Ok(parsed) = serde_yaml::from_str::<YamlValue>(query_yaml) else {
        return Vec::new();
    };
    let mut scopes = Vec::new();
    collect_scopes(&parsed, top_index, cache, &mut scopes);
    scopes
}

/// Appends the names YAML of `node` and, recursively, of its nested `queries` items.
fn collect_scopes(
    node: &YamlValue,
    index: &'static str,
    cache: &MetadataCache,
    scopes: &mut Vec<(&'static str, String)>,
) {
    let identity = cache.keyword_fields_of(index);
    if let Some(names_yaml) = kept_names_yaml(&without_identity_chains(node, identity), index) {
        scopes.push((index, names_yaml));
    }
    let items = node.get("queries").and_then(YamlValue::as_sequence);
    for item in items.into_iter().flatten() {
        if let Some(item_index) = item_index_key(item) {
            collect_scopes(item, item_index, cache, scopes);
        }
    }
}

/// Returns the cache key of a nested query's `index`, defaulting to `taxon` as `SearchQuery` does.
fn item_index_key(item: &YamlValue) -> Option<&'static str> {
    let index = match item.get("index") {
        Some(value) => serde_yaml::from_value::<SearchIndex>(value.clone()).ok()?,
        None => SearchIndex::Taxon,
    };
    Some(index_key(&index))
}

/// Checks attribute and field names in `query_yaml` against one cached group.
///
/// Returns `None` when every name is known, or when the group is empty.
pub fn names_error(group: &JsonValue, index_key: &str, query_yaml: &str) -> Option<String> {
    let names_yaml = name_checks_yaml(query_yaml, index_key)?;
    let field_meta = build_field_meta(group);
    let synonyms = build_synonyms(group);
    let errors = run_name_validator(&names_yaml, &field_meta, &synonyms);
    (!errors.is_empty()).then(|| errors.join("; "))
}

/// Keeps only the name-bearing keys of a query YAML, with `index` set to `index_key`.
fn name_checks_yaml(query_yaml: &str, index_key: &str) -> Option<String> {
    let parsed: YamlValue = serde_yaml::from_str(query_yaml).ok()?;
    kept_names_yaml(&parsed, index_key)
}

/// Keeps only the name-bearing keys of a parsed query node, with `index` set to `index_key`.
///
/// Attribute values that are chain references are dropped, because they are resolved from a
/// named query at run time.
fn kept_names_yaml(node: &YamlValue, index_key: &str) -> Option<String> {
    let source = node.as_mapping()?;
    let mut kept = Mapping::new();
    kept.insert(YamlValue::from("index"), YamlValue::from(index_key));
    for key in ["attributes", "fields"] {
        if let Some(value) = source.get(key) {
            kept.insert(YamlValue::from(key), value.clone());
        }
    }
    if let Some(attributes) = kept.get_mut("attributes") {
        strip_chain_values(attributes);
    }
    serde_yaml::to_string(&kept).ok()
}

/// Removes `value` from each chain-reference attribute entry, because it is resolved at run time.
fn strip_chain_values(attributes: &mut YamlValue) {
    let Some(items) = attributes.as_sequence_mut() else {
        return;
    };
    for item in items.iter_mut().filter(|item| is_chain_value(item)) {
        if let Some(entry) = item.as_mapping_mut() {
            entry.remove("value");
        }
    }
}

/// Returns a copy of `node` without chain-reference attributes on the `identity` keyword fields.
///
/// Such entries name identifiers, which are not attribute metadata, so they are not name-checked.
fn without_identity_chains(node: &YamlValue, identity: &[String]) -> YamlValue {
    let mut copy = node.clone();
    if let Some(items) = copy
        .get_mut("attributes")
        .and_then(YamlValue::as_sequence_mut)
    {
        items.retain(|item| !is_identity_chain_entry(item, identity));
    }
    copy
}

/// Returns `true` when an attribute entry's `value` is a chain reference.
fn is_chain_value(item: &YamlValue) -> bool {
    item.get("value")
        .and_then(YamlValue::as_str)
        .is_some_and(|value| ChainRef::parse(value).is_some())
}

/// Returns `true` when an attribute entry is a chain reference on one of the `identity` keyword fields.
fn is_identity_chain_entry(item: &YamlValue, identity: &[String]) -> bool {
    is_chain_value(item)
        && item
            .get("name")
            .and_then(YamlValue::as_str)
            .is_some_and(|name| identity.iter().any(|field| field == name))
}

/// Runs the portable validator and returns its error messages.
fn run_name_validator(
    names_yaml: &str,
    field_meta: &JsonValue,
    synonyms: &JsonValue,
) -> Vec<String> {
    let errors_json = genomehubs_query::validate_query_json(
        names_yaml,
        &field_meta.to_string(),
        "{}",
        &synonyms.to_string(),
    );
    serde_json::from_str(&errors_json)
        .unwrap_or_else(|e| vec![format!("name validation failed: {e}")])
}

/// Builds the validator's field metadata map from one cached group.
fn build_field_meta(group: &JsonValue) -> JsonValue {
    let entries: Map<String, JsonValue> = group
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, doc)| (name.clone(), field_meta_entry(doc)))
        .collect();
    JsonValue::Object(entries)
}

/// Converts one cached attribute document into the validator's `FieldMeta` shape.
///
/// `traverse_direction` is copied as-is, so `null` means the field is not inherited.
/// `constraint_enum` is taken from `constraint.enum` and is `null` for open-ended fields.
fn field_meta_entry(doc: &JsonValue) -> JsonValue {
    let processed_type = doc
        .get("processed_type")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    json!({
        "processed_type": processed_type,
        "traverse_direction": doc.get("traverse_direction").and_then(JsonValue::as_str),
        "summary": string_list(doc.get("summary")),
        "constraint_enum": enum_values(doc),
    })
}

/// Returns the non-empty `constraint.enum` list of a cached document, or `None`.
fn enum_values(doc: &JsonValue) -> Option<Vec<String>> {
    let values = string_list(doc.get("constraint").and_then(|c| c.get("enum")));
    (!values.is_empty()).then_some(values)
}

/// Returns the messages for a report query's chain references and names, or `None` when all pass.
///
/// Chain references and `filter_expr` syntax are checked without the cache. Names are checked
/// only when the cache is present, so a missing cache never rejects a request.
pub async fn validate_report_query(
    state: &AppState,
    query: &SearchQuery,
    query_yaml: &str,
) -> Option<String> {
    let (named, mut messages) = named_query_names(query);
    messages.extend(chain_messages(query));
    if let Some(cache) = state.cache.as_ref() {
        let guard = cache.read().await;
        messages.extend(scoped_names_error(
            &guard,
            index_key(&query.index),
            query_yaml,
        ));
        messages.extend(
            named
                .iter()
                .filter_map(|entry| named_query_name_error(&guard.attr_types, entry)),
        );
    }
    (!messages.is_empty()).then(|| messages.join("; "))
}

/// Returns one message per undefined or unsupported chain reference in the query.
fn chain_messages(query: &SearchQuery) -> Vec<String> {
    let no_specs = HashMap::new();
    let specs = query.named_queries.as_ref().unwrap_or(&no_specs);
    chain_ref_errors(&query.attributes.attributes, specs)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// The names used by one named query's `filter_expr`, with the group they are checked against.
struct NamedQueryNames {
    key: String,
    group_key: &'static str,
    names: Vec<String>,
}

/// Parses each named query's `filter_expr` into the names it uses.
///
/// Returns the names for each non-empty expression, and one message for each expression that does not parse.
fn named_query_names(query: &SearchQuery) -> (Vec<NamedQueryNames>, Vec<String>) {
    let Some(named_queries) = query.named_queries.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let mut entries: Vec<(&String, &NamedQuerySpec)> = named_queries.iter().collect();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    let mut names = Vec::new();
    let mut errors = Vec::new();
    for (key, spec) in entries {
        let expr = spec.filter_expr.trim();
        if expr.is_empty() {
            continue;
        }
        let index = spec.index.as_ref().unwrap_or(&query.index);
        match parse_filter_string(expr) {
            Ok(terms) => names.push(NamedQueryNames {
                key: key.clone(),
                group_key: index_key(index),
                names: terms
                    .iter()
                    .map(|term| filter_term_field(term).to_string())
                    .collect(),
            }),
            Err(message) => errors.push(format!("named query '{key}': {message}")),
        }
    }
    (names, errors)
}

/// Returns the field name a filter term refers to.
fn filter_term_field(term: &FilterTerm) -> &str {
    match term {
        FilterTerm::NumericRange { field, .. }
        | FilterTerm::AggRange { field, .. }
        | FilterTerm::KeywordMatch { field, .. }
        | FilterTerm::Exists { field } => field,
    }
}

/// Returns the names error for one named query, prefixed with its key, or `None` when all names are known.
fn named_query_name_error(attr_types: &JsonValue, entry: &NamedQueryNames) -> Option<String> {
    let group = attr_types.get(entry.group_key)?;
    let names_yaml = names_only_yaml(&entry.names, entry.group_key)?;
    names_error(group, entry.group_key, &names_yaml)
        .map(|message| format!("named query '{}': {message}", entry.key))
}

/// Builds a query YAML holding only the unique `names` as attributes of `index_key`.
fn names_only_yaml(names: &[String], index_key: &str) -> Option<String> {
    let unique: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let attributes: Vec<JsonValue> = unique
        .into_iter()
        .map(|name| json!({"name": name}))
        .collect();
    serde_yaml::to_string(&json!({"index": index_key, "attributes": attributes})).ok()
}

/// Builds a synonym → canonical name map from one cached group.
fn build_synonyms(group: &JsonValue) -> JsonValue {
    let mut synonyms = Map::new();
    for (name, doc) in group.as_object().into_iter().flatten() {
        for synonym in string_list(doc.get("synonyms")) {
            synonyms.insert(synonym, JsonValue::String(name.clone()));
        }
    }
    JsonValue::Object(synonyms)
}

/// Reads a JSON value that is either a string or an array of strings.
fn string_list(value: Option<&JsonValue>) -> Vec<String> {
    match value {
        Some(JsonValue::String(single)) => vec![single.clone()],
        Some(JsonValue::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_group() -> JsonValue {
        json!({
            "genome_size": {
                "processed_type": "long",
                "summary": ["primary", "min", "max"],
                "synonyms": ["gsize"]
            },
            "assembly_level": {
                "processed_type": "keyword",
                "summary": "",
                "synonyms": []
            }
        })
    }

    #[test]
    fn index_key_maps_each_search_index() {
        assert_eq!(index_key(&SearchIndex::Taxon), "taxon");
        assert_eq!(index_key(&SearchIndex::Assembly), "assembly");
        assert_eq!(index_key(&SearchIndex::Sample), "sample");
        assert_eq!(index_key(&SearchIndex::Feature), "feature");
    }

    #[test]
    fn known_attribute_and_summary_modifier_pass() {
        let yaml = "attributes:\n  - name: genome_size\n    operator: gt\n    value: 1000\n    modifiers: [min]\n";
        assert_eq!(names_error(&sample_group(), "taxon", yaml), None);
    }

    #[test]
    fn unknown_attribute_is_reported() {
        let yaml = "attributes:\n  - name: not_a_field\n    operator: gt\n    value: 1\n";
        assert_eq!(
            names_error(&sample_group(), "taxon", yaml),
            Some("unknown attribute 'not_a_field' for index 'taxon'".to_string())
        );
    }

    #[test]
    fn unknown_field_is_reported() {
        let yaml = "fields:\n  - name: not_a_field\n";
        assert_eq!(
            names_error(&sample_group(), "taxon", yaml),
            Some("unknown field 'not_a_field' for index 'taxon'".to_string())
        );
    }

    #[test]
    fn synonym_is_resolved_for_attributes_and_fields() {
        let yaml = "attributes:\n  - name: gsize\n    operator: gt\n    value: 1\nfields:\n  - name: gsize\n";
        assert_eq!(names_error(&sample_group(), "taxon", yaml), None);
    }

    #[test]
    fn empty_group_skips_name_checks() {
        let yaml = "attributes:\n  - name: anything\n    operator: gt\n    value: 1\n";
        assert_eq!(names_error(&json!({}), "taxon", yaml), None);
    }

    #[test]
    fn name_checks_yaml_drops_identifier_and_name_class_keys() {
        let yaml = "index: taxon\ntaxa: [Mammalia]\nnames: [bogus_class]\nattributes:\n  - name: genome_size\n";
        let kept = name_checks_yaml(yaml, "taxon").expect("valid yaml");
        assert!(kept.contains("attributes"));
        assert!(!kept.contains("taxa"));
        assert!(!kept.contains("bogus_class"));
    }

    #[test]
    fn name_checks_yaml_sets_index_from_parsed_type() {
        let yaml = "index: taxon\nattributes:\n  - name: assembly_span\n";
        let kept = name_checks_yaml(yaml, "assembly").expect("valid yaml");
        assert!(kept.contains("index: assembly"));
    }

    #[test]
    fn string_list_accepts_string_and_array() {
        assert_eq!(string_list(Some(&json!("min"))), vec!["min".to_string()]);
        assert_eq!(
            string_list(Some(&json!(["min", "max"]))),
            vec!["min".to_string(), "max".to_string()]
        );
        assert!(string_list(None).is_empty());
    }

    #[test]
    fn field_meta_entry_carries_traverse_direction() {
        let doc = json!({"processed_type": "keyword", "traverse_direction": "up"});
        assert_eq!(field_meta_entry(&doc)["traverse_direction"], "up");
    }

    #[test]
    fn field_meta_entry_keeps_null_traverse_when_absent() {
        let doc = json!({"processed_type": "long"});
        assert!(field_meta_entry(&doc)["traverse_direction"].is_null());
    }

    #[test]
    fn field_meta_entry_takes_constraint_enum() {
        let doc = json!({"processed_type": "keyword", "constraint": {"enum": ["chromosome", "scaffold"]}});
        assert_eq!(
            field_meta_entry(&doc)["constraint_enum"],
            json!(["chromosome", "scaffold"])
        );
    }

    #[test]
    fn field_meta_entry_ignores_numeric_constraint() {
        let doc = json!({"processed_type": "double", "constraint": {"min": 1}});
        assert!(field_meta_entry(&doc)["constraint_enum"].is_null());
    }

    #[test]
    fn enum_check_rejects_value_outside_cached_enum() {
        let group = json!({
            "assembly_level": {
                "processed_type": "keyword",
                "summary": ["enum"],
                "constraint": {"enum": ["chromosome", "scaffold"]}
            }
        });
        let yaml = "attributes:\n  - name: assembly_level\n    operator: eq\n    value: contig\n";
        assert!(names_error(&group, "taxon", yaml)
            .expect("enum violation expected")
            .contains("not valid for attribute 'assembly_level'"));
    }

    fn cache_of(attr_types: JsonValue) -> MetadataCache {
        MetadataCache {
            attr_types,
            ..Default::default()
        }
    }

    fn cache_with_keywords(group: JsonValue, keywords: &[&str]) -> MetadataCache {
        MetadataCache {
            attr_types: json!({"taxon": group}),
            keyword_fields: HashMap::from([(
                "taxon".to_string(),
                keywords.iter().map(|field| field.to_string()).collect(),
            )]),
            ..Default::default()
        }
    }

    fn nested_group_fixture() -> JsonValue {
        json!({
            "taxon": {"genome_size": {"processed_type": "long", "summary": ["min"]}},
            "assembly": {"assembly_span": {"processed_type": "long", "summary": []}}
        })
    }

    #[test]
    fn nested_query_is_checked_against_its_own_index() {
        let yaml = "index: taxon\nqueries:\n  - index: assembly\n    attributes:\n      - name: genome_size\n        operator: gt\n        value: 1\n";
        let message = scoped_names_error(&cache_of(nested_group_fixture()), "taxon", yaml)
            .expect("unknown name expected");
        assert!(
            message.contains("'genome_size' for index 'assembly'"),
            "{message}"
        );
    }

    #[test]
    fn nested_query_with_known_names_passes() {
        let yaml = "index: taxon\nqueries:\n  - index: taxon\n    attributes:\n      - name: genome_size\n        operator: gt\n        value: 1\n  - index: assembly\n    fields:\n      - name: assembly_span\n";
        assert_eq!(
            scoped_names_error(&cache_of(nested_group_fixture()), "taxon", yaml),
            None
        );
    }

    #[test]
    fn deeply_nested_query_is_checked_recursively() {
        let yaml = "index: taxon\nqueries:\n  - index: taxon\n    queries:\n      - index: assembly\n        fields:\n          - name: not_a_field\n";
        let message = scoped_names_error(&cache_of(nested_group_fixture()), "taxon", yaml)
            .expect("unknown name expected");
        assert!(
            message.contains("'not_a_field' for index 'assembly'"),
            "{message}"
        );
    }

    fn report_query(yaml: &str) -> SearchQuery {
        SearchQuery::from_yaml(yaml).expect("valid query yaml")
    }

    #[test]
    fn chain_value_on_enum_attribute_is_not_checked_as_enum() {
        let group = json!({
            "assembly_level": {
                "processed_type": "keyword",
                "summary": ["enum"],
                "constraint": {"enum": ["chromosome", "scaffold"]}
            }
        });
        let yaml = "attributes:\n  - name: assembly_level\n    operator: eq\n    value: queryA.assembly_level\n";
        assert_eq!(scoped_names_error(&cache_of(group), "taxon", yaml), None);
    }

    #[test]
    fn chain_reference_on_keyword_field_is_not_checked_against_metadata() {
        let cache = cache_with_keywords(sample_group(), &["taxon_id", "scientific_name"]);
        let yaml = "attributes:\n  - name: scientific_name\n    operator: eq\n    value: queryA.scientific_name\n";
        assert_eq!(scoped_names_error(&cache, "taxon", yaml), None);
    }

    #[test]
    fn chain_reference_on_non_keyword_field_is_checked_against_metadata() {
        let cache = cache_with_keywords(sample_group(), &["taxon_id"]);
        let yaml =
            "attributes:\n  - name: not_a_field\n    operator: eq\n    value: queryA.not_a_field\n";
        assert!(scoped_names_error(&cache, "taxon", yaml).is_some());
    }

    #[test]
    fn chain_value_is_not_checked_as_attribute_value() {
        let yaml =
            "attributes:\n  - name: genome_size\n    operator: gt\n    value: queryA.genome_size\n";
        assert_eq!(names_error(&sample_group(), "taxon", yaml), None);
    }

    #[test]
    fn chain_messages_reports_undefined_key() {
        let query = report_query("index: taxon\nattributes:\n  - name: taxon_id\n    operator: eq\n    value: queryZ.taxon_id\n");
        let messages = chain_messages(&query);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("queryZ"), "{}", messages[0]);
    }

    #[test]
    fn chain_messages_passes_defined_key() {
        let query = report_query("index: taxon\nnamed_queries:\n  queryA:\n    filter_expr: genome_size>1\nattributes:\n  - name: taxon_id\n    operator: eq\n    value: queryA.taxon_id\n");
        assert!(chain_messages(&query).is_empty());
    }

    #[test]
    fn chain_messages_reports_unsupported_summary() {
        let query = report_query("index: taxon\nnamed_queries:\n  queryA:\n    filter_expr: genome_size>1\nattributes:\n  - name: taxon_id\n    operator: eq\n    value: queryA.mean(genome_size)\n");
        let messages = chain_messages(&query);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("not supported yet"), "{}", messages[0]);
    }

    #[test]
    fn named_query_unknown_filter_name_is_checked_against_its_index() {
        let query = report_query("index: taxon\nnamed_queries:\n  queryA:\n    index: assembly\n    filter_expr: genome_size>1\n");
        let (named, errors) = named_query_names(&query);
        assert!(errors.is_empty());
        assert_eq!(named.len(), 1);
        let message = named_query_name_error(&nested_group_fixture(), &named[0])
            .expect("unknown name expected");
        assert_eq!(
            message,
            "named query 'queryA': unknown attribute 'genome_size' for index 'assembly'"
        );
    }

    #[test]
    fn named_query_without_index_inherits_top_level_index() {
        let query = report_query(
            "index: taxon\nnamed_queries:\n  queryA:\n    filter_expr: genome_size>1\n",
        );
        let (named, _) = named_query_names(&query);
        assert_eq!(named[0].group_key, "taxon");
        assert_eq!(
            named_query_name_error(&nested_group_fixture(), &named[0]),
            None
        );
    }

    #[test]
    fn named_query_filter_parse_error_is_reported() {
        let query = report_query(
            "index: taxon\nnamed_queries:\n  queryA:\n    filter_expr: genome_size>abc\n",
        );
        let (named, errors) = named_query_names(&query);
        assert!(named.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("named query 'queryA': "),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn named_query_empty_filter_is_skipped() {
        let query = report_query("index: taxon\nnamed_queries:\n  queryA:\n    filter_expr: ''\n");
        let (named, errors) = named_query_names(&query);
        assert!(named.is_empty());
        assert!(errors.is_empty());
    }
}
