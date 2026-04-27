use anyhow::Context;
use std::collections::HashMap;

use crate::core::query::{
    AttributeSet, Field, Identifiers, QueryParams, SearchIndex, SearchQuery, TaxaIdentifier,
    TaxonFilterType,
};

fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// Convert a map of URL query parameters into a `SearchQuery` + `QueryParams` pair.
///
/// Supports two modes:
/// - `query_yaml` present: parse YAML directly into `SearchQuery`.
/// - Otherwise parse common flat params: `result`, `taxa`, `taxon_filter_type`,
///   `names`, `ranks`, `fields`, plus pagination/sort params.
pub fn parse_url_params(
    params: &HashMap<String, String>,
) -> anyhow::Result<(SearchQuery, QueryParams)> {
    // If YAML was provided directly prefer it — SDKs can emit YAML.
    if let Some(yaml) = params
        .get("query_yaml")
        .or_else(|| params.get("search_query_yaml"))
    {
        let query: SearchQuery = serde_yaml::from_str(yaml)
            .with_context(|| "parsing SearchQuery YAML from query_yaml param")?;
        let params_obj = if let Some(pyaml) = params.get("params_yaml") {
            serde_yaml::from_str(pyaml)
                .with_context(|| "parsing QueryParams YAML from params_yaml")?
        } else {
            QueryParams::default()
        };
        return Ok((query, params_obj));
    }

    // Build SearchQuery from common flat params.
    let index = params
        .get("result")
        .map(|s| match s.as_str() {
            "assembly" => SearchIndex::Assembly,
            "sample" => SearchIndex::Sample,
            _ => SearchIndex::Taxon,
        })
        .unwrap_or(SearchIndex::Taxon);

    // Identifiers
    let identifiers = {
        let taxa = params.get("taxa").map(|s| split_list(s));
        let taxon_filter_type = params
            .get("taxon_filter_type")
            .map(|v| match v.as_str() {
                "tree" => TaxonFilterType::Tree,
                "lineage" => TaxonFilterType::Lineage,
                _ => TaxonFilterType::Name,
            })
            .unwrap_or_default();

        Identifiers {
            taxa: taxa.map(|names| TaxaIdentifier {
                names,
                filter_type: taxon_filter_type,
            }),
            assemblies: params
                .get("assemblies")
                .map(|s| split_list(s))
                .unwrap_or_default(),
            samples: params
                .get("samples")
                .map(|s| split_list(s))
                .unwrap_or_default(),
            rank: params.get("rank").cloned(),
        }
    };

    // Attributes / fields
    let attributes = {
        let fields = params
            .get("fields")
            .map(|s| {
                split_list(s)
                    .into_iter()
                    .map(|name| Field {
                        name,
                        modifier: Vec::new(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let names = params
            .get("names")
            .map(|s| split_list(s))
            .unwrap_or_default();
        let ranks = params
            .get("ranks")
            .map(|s| split_list(s))
            .unwrap_or_default();

        AttributeSet {
            attributes: Vec::new(),
            fields,
            names,
            ranks,
            exclude_ancestral: Vec::new(),
            exclude_descendant: Vec::new(),
            exclude_direct: Vec::new(),
            exclude_missing: Vec::new(),
        }
    };

    // QueryParams: pagination, sort, tidy flags
    let mut qp = QueryParams::default();
    if let Some(size) = params.get("size") {
        if let Ok(v) = size.parse::<usize>() {
            qp.size = v;
        }
    }
    if let Some(page) = params.get("page") {
        if let Ok(v) = page.parse::<usize>() {
            qp.page = v;
        }
    }
    if let Some(sort_by) = params.get("sortBy").or_else(|| params.get("sort_by")) {
        qp.sort_by = Some(sort_by.clone());
    }
    if let Some(sort_order) = params.get("sortOrder").or_else(|| params.get("sort_order")) {
        qp.sort_order = match sort_order.as_str() {
            "desc" => crate::core::query::SortOrder::Desc,
            _ => crate::core::query::SortOrder::Asc,
        };
    }
    if let Some(taxonomy) = params.get("taxonomy") {
        qp.taxonomy = taxonomy.clone();
    }
    if let Some(tidy) = params.get("tidy") {
        qp.tidy = tidy == "true" || tidy == "1";
    }

    let query = SearchQuery {
        index,
        identifiers,
        attributes,
    };

    Ok((query, qp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_taxa_and_fields() {
        let mut params = HashMap::new();
        params.insert("result".to_string(), "taxon".to_string());
        params.insert("taxa".to_string(), "Mammalia, !Felis".to_string());
        params.insert("taxon_filter_type".to_string(), "tree".to_string());
        params.insert(
            "fields".to_string(),
            "genome_size, gc_percentage".to_string(),
        );

        let (query, qp) = parse_url_params(&params).expect("parse");
        assert_eq!(query.index, SearchIndex::Taxon);
        let taxa = query.identifiers.taxa.expect("taxa present");
        assert_eq!(taxa.names, vec!["Mammalia", "!Felis"]);
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
        assert_eq!(query.attributes.fields.len(), 2);
        assert_eq!(qp.size, 10);
    }

    #[test]
    fn parse_query_yaml_preferred() {
        let mut params = HashMap::new();
        let yaml = "index: assembly\nassemblies: [GCF_000002305.6]";
        params.insert("query_yaml".to_string(), yaml.to_string());
        let (query, _qp) = parse_url_params(&params).expect("parse yaml");
        assert_eq!(query.index, SearchIndex::Assembly);
        assert_eq!(query.identifiers.assemblies, vec!["GCF_000002305.6"]);
    }
}
