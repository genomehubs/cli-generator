use crate::AppState;
use genomehubs_query::query::SearchIndex;

/// Resolve an ES index name from a `SearchIndex` variant and the server's
/// configured index suffix.
///
/// Example: `SearchIndex::Taxon` + suffix `"--ncbi--goat--2021.10.15"`
/// → `"taxon--ncbi--goat--2021.10.15"`
pub fn resolve_index(index: &SearchIndex, state: &AppState) -> String {
    let base = match index {
        SearchIndex::Taxon => "taxon",
        SearchIndex::Assembly => "assembly",
        SearchIndex::Sample => "sample",
    };
    match &state.index_suffix {
        Some(suf) => format!("{base}{suf}"),
        None => base.to_string(),
    }
}

/// Resolve an explicit index type name (string) instead of a `SearchIndex`.
/// Used by endpoints that accept `result` as a query param.
pub fn resolve_index_str(result: &str, state: &AppState) -> String {
    let base = match result {
        "assembly" => "assembly",
        "sample" => "sample",
        _ => "taxon",
    };
    match &state.index_suffix {
        Some(suf) => format!("{base}{suf}"),
        None => base.to_string(),
    }
}
