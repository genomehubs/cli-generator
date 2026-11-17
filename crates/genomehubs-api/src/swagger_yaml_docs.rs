//! Documents the options carried inside `query_yaml` and `params_yaml`.
//!
//! Request bodies pass these options as YAML strings, so the generated schema
//! carries no structure. The Markdown below is written to every schema property
//! with either name so each endpoint documents the same options.

use serde_json::{Map, Value};

/// Markdown describing the keys accepted in a `query_yaml` document.
pub const QUERY_YAML_DOC: &str = r#"YAML document describing **what** to search for. Column selection (`fields`) belongs here, not in `params_yaml`.

**Identifiers**

- `index`: `taxon` (default), `assembly`, `sample`, or `feature`.
- `taxa`: list of taxon names or IDs. Prefix with `!` to exclude (e.g. `!Felis`). Leading or trailing `*` wildcards are supported.
- `taxon_filter_type`: `name` (default, exact match), `tree` (descendants), or `lineage` (ancestors). Applies to `taxa`.
- `rank`: restrict results to one taxonomic rank (e.g. `species`).
- `assemblies`: list of assembly accession IDs (e.g. `GCF_000002305.6`).
- `samples`: list of sample accession IDs.

**Attributes**

- `attributes`: list of filter conditions. Each item has:
  - `name` (required): attribute name or synonym.
  - `operator`: `eq`, `ne`, `lt`, `le`, `gt`, `ge`, `exists`, or `missing`. The symbols `=`, `!=`, `<`, `<=`, `>`, `>=` are also accepted. Omit `operator` for a presence test; `exists` and `missing` take no `value`.
  - `value`: a string, or a list of strings for set membership. Size suffixes and scientific notation are expanded (`3G` becomes `3000000000`, `500M` becomes `500000000`, `1e9` becomes `1000000000`).
  - `modifier` (alias `modifiers`): summary modifiers `min`, `max`, `median`, `mean`, `sum`, `list`, `length`; status modifiers `direct`, `ancestral`, `descendant`, `estimated`, `missing`.
- `fields`: list of columns to return. Each item is a field name, or an object `{name, modifier}` using the same modifiers.
- `names`: taxon name classes to return, e.g. `scientific_name`, `common_name`, `synonym`, `tolid_prefix`, `authority`. Filter suffixes such as `common_name:*bat*` are supported.
- `ranks`: ancestor rank columns to return, e.g. `genus`, `family`.
- `exclude_ancestral`, `exclude_descendant`, `exclude_direct`, `exclude_missing`: lists of field names. Drop values for those fields that are inferred from ancestors, inferred from descendants, directly estimated, or missing, respectively.

**Multi-query and chaining**

- `queries`: list of nested query documents, combined using `combine_with`.
- `combine_with`: `AND` (default) or `OR`.
- `named_queries`: map of name to sub-query `{index, filter_expr, inherit_scope, limit}`. `limit` defaults to 500 and is capped at 10000. Reference a sub-query result in an attribute value as `queryA.field` or `queryA.summary(field)`.
- `lineage_rank_summary`: list of `{rank, fields}` pairs. Adds per-ancestor aggregations to the `lineage_summary` response field."#;

/// Markdown describing the keys accepted in a `params_yaml` document.
pub const PARAMS_YAML_DOC: &str = r#"YAML document describing **how** to fetch and present results. Unknown keys are rejected.

- `size`: records per page (default `10`).
- `page`: 1-based page number (default `1`). Offset is `(page - 1) * size`.
- `search_after`: cursor array for paginated continuation. Pass the `searchAfter` value returned by the previous page.
- `sort_by`: field to sort by (default ascending). Ignored when `sort` is set.
- `sort_order`: `asc` (default) or `desc`. Applies to `sort_by`.
- `sort`: list of `{by, order}` pairs applied in order, e.g. `[{by: genome_size, order: desc}]`.
- `include_estimates`: include ancestrally derived estimates (default `false`).
- `tidy`: request tidy (long) format (default `false`).
- `taxonomy`: taxonomy backbone (default `ncbi`).
- `include_lineage`: include the full lineage array in each record (default `false`).
- `include_taxon_names`: include the `taxon_names` array in each record (default `false`).
- `lineage_summary_mode`: `background` (default) uses all descendant taxa; `matched` uses only the query results.
- `id_set`: list of IDs to restrict results to. Maximum 65,536 entries.
- `id_type`: `taxon`, `assembly`, `sample`, or `feature`. The ID field `id_set` filters on; defaults to the query index."#;

/// Set the description of every `query_yaml` and `params_yaml` schema property.
///
/// Walks `components.schemas` in a serialised OpenAPI document and overwrites
/// the description of matching properties.
pub fn apply_to_json(openapi: &mut Value) {
    let Some(schemas) = openapi
        .pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for schema in schemas.values_mut() {
        let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
            continue;
        };
        set_description(properties, "query_yaml", QUERY_YAML_DOC);
        set_description(properties, "params_yaml", PARAMS_YAML_DOC);
    }
}

/// Overwrite the `description` of one property when it exists.
fn set_description(properties: &mut Map<String, Value>, property: &str, text: &str) {
    if let Some(prop) = properties.get_mut(property).and_then(Value::as_object_mut) {
        prop.insert("description".to_string(), Value::String(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genomehubs_query::query::{QueryParams, SearchQuery};
    use serde_json::json;

    const QUERY_KEYS: &[&str] = &[
        "index",
        "taxa",
        "taxon_filter_type",
        "rank",
        "assemblies",
        "samples",
        "attributes",
        "fields",
        "names",
        "ranks",
        "exclude_ancestral",
        "exclude_descendant",
        "exclude_direct",
        "exclude_missing",
        "queries",
        "combine_with",
        "named_queries",
        "lineage_rank_summary",
    ];

    const PARAMS_KEYS: &[&str] = &[
        "size",
        "page",
        "search_after",
        "sort_by",
        "sort_order",
        "sort",
        "include_estimates",
        "tidy",
        "taxonomy",
        "include_lineage",
        "include_taxon_names",
        "lineage_summary_mode",
        "id_set",
        "id_type",
    ];

    const QUERY_SAMPLE: &str = r#"
index: taxon
taxa: [Mammalia, "!Felis"]
taxon_filter_type: tree
rank: species
assemblies: [GCF_000002305.6]
samples: [SRR1234567]
attributes:
  - name: genome_size
    operator: gt
    value: 3G
    modifier: [min]
fields:
  - name: genome_size
    modifier: [min]
names: [scientific_name]
ranks: [genus]
exclude_ancestral: [genome_size]
exclude_descendant: [genome_size]
exclude_direct: [genome_size]
exclude_missing: [genome_size]
combine_with: OR
named_queries:
  queryA:
    index: assembly
    filter_expr: "assembly_span>1000000000"
    limit: 200
lineage_rank_summary:
  - rank: genus
    fields: [genome_size]
"#;

    const PARAMS_SAMPLE: &str = r#"
size: 25
page: 2
search_after: [1, "abc"]
sort_by: genome_size
sort_order: desc
sort:
  - by: genome_size
    order: desc
include_estimates: true
tidy: false
taxonomy: ncbi
include_lineage: false
include_taxon_names: false
lineage_summary_mode: matched
id_set: ["9606"]
id_type: taxon
"#;

    #[test]
    fn query_doc_names_every_documented_key() {
        for key in QUERY_KEYS {
            assert!(
                QUERY_YAML_DOC.contains(&format!("`{key}`")),
                "query_yaml doc is missing `{key}`"
            );
        }
    }

    #[test]
    fn params_doc_names_every_documented_key() {
        for key in PARAMS_KEYS {
            assert!(
                PARAMS_YAML_DOC.contains(&format!("`{key}`")),
                "params_yaml doc is missing `{key}`"
            );
        }
    }

    #[test]
    fn query_sample_with_documented_keys_parses() {
        SearchQuery::from_yaml(QUERY_SAMPLE).expect("documented query keys should parse");
    }

    #[test]
    fn params_sample_with_documented_keys_parses() {
        QueryParams::from_yaml(PARAMS_SAMPLE).expect("documented params keys should parse");
    }

    #[test]
    fn apply_sets_description_on_yaml_properties_only() {
        let mut spec = json!({
            "components": {
                "schemas": {
                    "SearchRequest": {
                        "properties": {
                            "query_yaml": {"type": "string"},
                            "params_yaml": {"type": "string"}
                        }
                    },
                    "Other": {
                        "properties": {"name": {"type": "string"}}
                    }
                }
            }
        });
        apply_to_json(&mut spec);
        let request = &spec["components"]["schemas"]["SearchRequest"]["properties"];
        assert_eq!(request["query_yaml"]["description"], json!(QUERY_YAML_DOC));
        assert_eq!(
            request["params_yaml"]["description"],
            json!(PARAMS_YAML_DOC)
        );
        assert!(spec["components"]["schemas"]["Other"]["properties"]["name"]
            .get("description")
            .is_none());
    }

    #[test]
    fn generated_spec_documents_every_yaml_property() {
        use crate::ApiDoc;
        use utoipa::OpenApi;

        let mut spec = serde_json::to_value(ApiDoc::openapi()).expect("serialise OpenAPI");
        apply_to_json(&mut spec);
        let schemas = spec["components"]["schemas"]
            .as_object()
            .expect("schemas object");
        let mut documented = 0;
        for schema in schemas.values() {
            let Some(properties) = schema.get("properties") else {
                continue;
            };
            if let Some(query) = properties.get("query_yaml") {
                assert_eq!(query["description"], json!(QUERY_YAML_DOC));
                documented += 1;
            }
            if let Some(params) = properties.get("params_yaml") {
                assert_eq!(params["description"], json!(PARAMS_YAML_DOC));
            }
        }
        assert!(documented > 0, "no schema exposes query_yaml");
    }
}
