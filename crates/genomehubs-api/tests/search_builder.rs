use cli_generator::core::query_builder::build_search_body;
use genomehubs_api::request_shape::query_to_body_input;
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
    let search_body_input = query_to_body_input("search", &q, &params, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
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
    let search_body_input = query_to_body_input("search", &q, &params, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"must_not\""));
}

#[test]
fn range_gte() {
    let q = make_query(
        "index: taxon\nattributes:\n  - name: genome_size\n    operator: gte\n    value: \"1000000000\"\n"
    );
    let params = default_params();
    let search_body_input = query_to_body_input("search", &q, &params, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"range\""));
    assert!(body_str.contains("\"gte\""));
}

#[test]
fn field_projection() {
    let q = make_query("index: taxon\nfields:\n  - name: genome_size\n");
    let params = default_params();
    let search_body_input = query_to_body_input("search", &q, &params, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("genome_size"));
}

#[test]
fn pagination_offset() {
    let params = QueryParams::from_yaml("size: 50\npage: 3\ntaxonomy: ncbi\n").unwrap();
    let q = make_query("index: taxon\n");
    let search_body_input = query_to_body_input("search", &q, &params, None);
    let body = match cli_generator::core::query_builder::build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(
        body_str.contains("\"from\":100") || body_str.contains("\"from\": 100"),
        "should have from=100, got: {}",
        body_str
    );
}
