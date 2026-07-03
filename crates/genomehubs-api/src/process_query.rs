//! functions to modify a query before sending it to the API, e.g. to add lineage IDs for a given taxon name

use crate::{es_client, request_shape::query_to_body_input, AppState};
use cli_generator::core::query_builder::build_search_body;
use genomehubs_query::query::{
    AttributeSet, Identifiers, QueryParams, SearchQuery, TaxaIdentifier, TaxonFilterType,
};

pub async fn get_lineage_ids(
    client: &reqwest::Client,
    es_base: &str,
    idx: &str,
    query: &SearchQuery,
    params: &QueryParams,
) -> Result<Vec<String>, String> {
    let mod_query = SearchQuery {
        identifiers: Identifiers {
            taxa: Some(TaxaIdentifier {
                filter_type: TaxonFilterType::Name,
                ..query.identifiers.taxa.as_ref().unwrap().clone()
            }),
            ..Default::default()
        },
        attributes: AttributeSet {
            fields: vec![],
            ..Default::default()
        },
        ..Default::default()
    };
    let mod_params = QueryParams {
        size: 10000,                // fetch all matching taxa (up to 10k)
        page: 1,                    // start from the beginning
        include_taxon_names: false, // we only need the taxon IDs
        include_lineage: true,      // we need the lineage to get the ancestor IDs
        ..params.clone()
    };
    let search_body_input =
        query_to_body_input("search", &mod_query, &mod_params, None, None, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => return Err(format!("failed to build ES body: {}", e)),
    };

    let raw = match es_client::execute_search(client, es_base, idx, &body).await {
        Ok(v) => v,
        Err(e) => return Err(format!("failed to execute ES search: {}", e)),
    };

    // Extract taxon IDs from hits
    let hits = raw
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|h| h.as_array());
    if let Some(hits_array) = hits {
        let mut id_set = std::collections::HashSet::new();
        for hit in hits_array {
            let lineage = hit
                .get("_source")
                .and_then(|s| s.get("lineage"))
                .and_then(|l| l.as_array());

            if let Some(lineage_array) = lineage {
                for lineage_item in lineage_array {
                    let lineage_id = lineage_item
                        .get("taxon_id")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string();
                    id_set.insert(lineage_id.clone());
                }
            }
        }
        if id_set.is_empty() {
            return Err("no lineage IDs found for the given taxa".to_string());
        } else if id_set.len() > 10000 {
            return Err(format!(
                "too many lineage IDs found ({}), limit is 10,000",
                id_set.len()
            ));
        }
        Ok(id_set.into_iter().collect())
    } else {
        Err(format!(
            "unexpected ES search response: {}",
            raw.to_string().chars().take(512).collect::<String>()
        ))
    }
}

pub fn inject_lineage_ids_into_query(query: &SearchQuery, lineage_ids: Vec<String>) -> SearchQuery {
    let identifiers = &query.identifiers;
    SearchQuery {
        identifiers: Identifiers {
            taxa: Some(TaxaIdentifier {
                names: lineage_ids,
                filter_type: TaxonFilterType::Name,
            }),
            ..identifiers.clone()
        },
        ..query.clone()
    }
}

pub async fn process_query(
    state: &AppState,
    idx: &str,
    query: &SearchQuery,
    params: &QueryParams,
) -> Result<SearchQuery, String> {
    let identifiers = &query.identifiers;
    let mut query = query.clone();
    if let Some(taxa) = &identifiers.taxa {
        // if filter type is Lineage, then we need to do an inital search to get the taxon IDs for the lineage, and then inject those into the count query
        if taxa.filter_type == TaxonFilterType::Lineage {
            let lineage_ids =
                match get_lineage_ids(&state.client, &state.es_base, idx, &query, params).await {
                    Ok(ids) => ids,
                    Err(e) => {
                        return Err(format!("failed to get lineage IDs: {}", e));
                    }
                };
            // inject the lineage IDs into the count query
            query = inject_lineage_ids_into_query(&query, lineage_ids);
        }
    }
    Ok(query)
}
