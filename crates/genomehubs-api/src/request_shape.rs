//! convert QueryParams/SearchQuery to SearchBodyInput

use std::collections::HashSet;

use cli_generator::core::query_builder::SearchBodyInput;
use genomehubs_query::query::{Attribute, QueryParams, SearchQuery, SortConfig};

// Convert a `SearchQuery` plus `QueryParams` to a `SearchBodyInput` for use in the API request body.

pub fn query_to_body_input(
    endpoint: &str,
    query: &SearchQuery,
    params: &QueryParams,
    types_map: Option<cli_generator::core::attr_types::TypesMap>,
    ranks_set: Option<HashSet<String>>,
    names_set: Option<HashSet<String>>,
) -> SearchBodyInput {
    let group = match query.index {
        genomehubs_query::query::SearchIndex::Taxon => "taxon",
        genomehubs_query::query::SearchIndex::Assembly => "assembly",
        genomehubs_query::query::SearchIndex::Sample => "sample",
        genomehubs_query::query::SearchIndex::Feature => "feature",
    };

    let mut fields_slice: Option<Vec<String>> = if query.attributes.fields.is_empty() {
        None
    } else {
        Some(
            query
                .attributes
                .fields
                .iter()
                .map(|f| f.name.clone())
                .collect(),
        )
    };
    let names_slice: Option<Vec<String>> = if query.attributes.names.is_empty() {
        None
    } else {
        Some(query.attributes.names.to_vec())
    };
    let ranks_slice: Option<Vec<String>> = if query.attributes.ranks.is_empty() {
        None
    } else {
        Some(query.attributes.ranks.to_vec())
    };
    let attributes_slice: Option<Vec<Attribute>> = if query.attributes.attributes.is_empty() {
        None
    } else {
        Some(query.attributes.attributes.to_vec())
    };

    // if there are exclusions, convert them to a HashMap<&str, Vec<&str>> for easier handling in the query builder

    let mut raw_exclusions: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (exclusion_type, values) in [
        ("ancestor", &query.attributes.exclude_ancestral),
        ("descendant", &query.attributes.exclude_descendant),
        ("direct", &query.attributes.exclude_direct),
        ("missing", &query.attributes.exclude_missing),
    ] {
        for field in values {
            raw_exclusions
                .entry(field.clone())
                .or_default()
                .push(exclusion_type.to_string());
        }
    }
    let exclusions = if raw_exclusions.is_empty() {
        None
    } else {
        // make sure all exclusion keys are in fields_slice, otherwise the query builder will ignore them and not apply the exclusions
        if let Some(fields) = &mut fields_slice {
            for field in raw_exclusions.keys() {
                if !fields.contains(field) {
                    fields.push(field.clone());
                }
            }
        } else {
            // if fields_slice is None, we need to create it and add all exclusion keys to it
            fields_slice = Some(raw_exclusions.keys().cloned().collect());
        }
        Some(raw_exclusions)
    };

    let taxa_query = query
        .identifiers
        .taxa
        .as_ref()
        .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

    let size = if endpoint == "count" {
        0 // for count endpoint, we don't need any hits, just the total count
    } else {
        params.size
    };

    let offset = (params.page.saturating_sub(1)) * size;

    let mut sort = params.sort.clone().unwrap_or_default();
    if sort.is_empty() {
        // use sort_by if sort is empty
        if let Some(sort_by) = &params.sort_by {
            sort.push(SortConfig {
                by: sort_by.clone(),
                order: Some(params.sort_order.clone()),
            });
        }
    }

    SearchBodyInput {
        query: taxa_query,
        attributes: attributes_slice,
        rank: query.identifiers.rank.clone(),
        group: Some(group.to_string()),
        fields: fields_slice,
        optional_fields: None, // not currently supported in the API
        include_estimates: params.include_estimates,
        types_map,
        names_set,
        ranks_set,
        names: names_slice,
        ranks: ranks_slice,
        exclusions,
        sort: if sort.is_empty() { None } else { Some(sort) },
        size,
        offset,
    }
}
