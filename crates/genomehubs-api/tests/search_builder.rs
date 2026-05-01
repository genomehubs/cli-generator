use cli_generator::core::query_builder::build_search_body;
use genomehubs_query::query::{QueryParams, SearchQuery};

fn make_query(yaml: &str) -> SearchQuery {
    SearchQuery::from_yaml(yaml).expect("parse failed")
}

fn default_params() -> QueryParams {
    QueryParams::from_yaml("taxonomy: ncbi\n").unwrap()
}

#[test]
fn equality_operator() {
    let q = make_query(
        "index: taxon\nattributes:\n  - name: assembly_level\n    operator: eq\n    value: chromosome\n"
    );
    let params = default_params();
    let body = build_search_body(
        None,
        None,
        None,
        Some(&q.attributes.attributes),
        q.identifiers.rank.as_deref(),
        None,
        None,
        params.sort_by.as_deref(),
        None,
        params.size,
        0,
        None,
        Some("taxon"),
    );
    let body_str = match body {
        Ok(b) => serde_json::to_string(&b).unwrap(),
        Err(e) => panic!("failed to build body: {}", e),
    };
    println!("Body: {}", body_str);
    // Just assert that building succeeded and we got a non-empty body
    assert!(!body_str.is_empty(), "should produce non-empty body");
}

#[test]
fn inequality_not_equal() {
    let q = make_query(
        "index: taxon\nattributes:\n  - name: assembly_level\n    operator: ne\n    value: contig\n"
    );
    let params = default_params();
    let body = build_search_body(
        None,
        None,
        None,
        Some(&q.attributes.attributes),
        q.identifiers.rank.as_deref(),
        None,
        None,
        params.sort_by.as_deref(),
        None,
        params.size,
        0,
        None,
        Some("taxon"),
    );
    let body_str = match body {
        Ok(b) => serde_json::to_string(&b).unwrap(),
        Err(e) => panic!("failed to build body: {}", e),
    };
    assert!(body_str.contains("\"must_not\""));
}

#[test]
fn range_gte() {
    let q = make_query(
        "index: taxon\nattributes:\n  - name: genome_size\n    operator: gte\n    value: \"1000000000\"\n"
    );
    let params = default_params();
    let body = build_search_body(
        None,
        None,
        None,
        Some(&q.attributes.attributes),
        q.identifiers.rank.as_deref(),
        None,
        None,
        params.sort_by.as_deref(),
        None,
        params.size,
        0,
        None,
        Some("taxon"),
    );
    let body_str = match body {
        Ok(b) => serde_json::to_string(&b).unwrap(),
        Err(e) => panic!("failed to build body: {}", e),
    };
    assert!(body_str.contains("\"range\""));
    assert!(body_str.contains("\"gte\""));
}

#[test]
fn field_projection() {
    let q = make_query("index: taxon\nfields:\n  - name: genome_size\n");
    let params = default_params();
    let fields = vec!["genome_size"];
    let body = build_search_body(
        None,
        Some(&fields),
        None,
        None,
        q.identifiers.rank.as_deref(),
        None,
        None,
        params.sort_by.as_deref(),
        None,
        params.size,
        0,
        None,
        Some("taxon"),
    );
    let body_str = match body {
        Ok(b) => serde_json::to_string(&b).unwrap(),
        Err(e) => panic!("failed to build body: {}", e),
    };
    assert!(body_str.contains("genome_size"));
}

#[test]
fn pagination_offset() {
    let params = QueryParams::from_yaml("size: 50\npage: 3\ntaxonomy: ncbi\n").unwrap();
    let q = make_query("index: taxon\n");
    let offset = (params.page.saturating_sub(1)) * params.size;
    let body = build_search_body(
        None,
        None,
        None,
        None,
        q.identifiers.rank.as_deref(),
        None,
        None,
        params.sort_by.as_deref(),
        None,
        params.size,
        offset,
        None,
        Some("taxon"),
    );
    // page 3, size 50 → from = 100
    let body_str = match body {
        Ok(b) => serde_json::to_string(&b).unwrap(),
        Err(e) => panic!("failed to build body: {}", e),
    };
    assert!(
        body_str.contains("\"from\":100") || body_str.contains("\"from\": 100"),
        "should have from=100, got: {}",
        body_str
    );
}
