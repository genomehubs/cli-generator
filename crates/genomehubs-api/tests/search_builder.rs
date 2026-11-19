use genomehubs_api::request_shape::query_to_body_input;
use genomehubs_query::query::{QueryParams, SearchQuery};
use genomehubs_query::query_builder::build_search_body;

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
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
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
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
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
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
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
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("genome_size"));
}

#[test]
fn mammalia_assembly_fields_and_range_filter() {
    let q = make_query(
        "index: assembly\ntaxa:\n  - Mammalia\ntaxon_filter_type: tree\nfields:\n  - name: assembly_span\n  - name: assembly_level\nattributes:\n  - name: assembly_span\n    operator: gte\n    value: \"1000000000\"\n"
    );
    let params = QueryParams::from_yaml("size: 10\ntaxonomy: ncbi\n").unwrap();
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    let body = match build_search_body(&search_body_input) {
        Ok(b) => b,
        Err(e) => panic!("failed to build body: {}", e),
    };
    let body_str = serde_json::to_string(&body).unwrap();
    println!("mammalia assembly body: {}", body_str);
    assert!(body_str.contains("assembly_span"));
    assert!(body_str.contains("attributes.long_value"));
    assert!(body_str.contains("\"gte\""));
    assert!(body_str.contains("\"range\""));
    assert!(!body_str.contains("\"aggregation_source\":\"direct\""));
    assert!(!body_str.contains("\"aggregation_source\":\"descendant\""));
}

#[test]
fn pagination_offset() {
    let params = QueryParams::from_yaml("size: 50\npage: 3\ntaxonomy: ncbi\n").unwrap();
    let q = make_query("index: taxon\n");
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    let body = match genomehubs_query::query_builder::build_search_body(&search_body_input) {
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

#[test]
fn assembly_and_sample_identifiers_reach_body() {
    let q = make_query(
        "index: assembly\nassemblies:\n  - GCA_000001405.29\nsamples:\n  - SAMN00000001\n",
    );
    let params = default_params();
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    let body = build_search_body(&search_body_input).unwrap();
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"assembly_id\":[\"GCA_000001405.29\"]"));
    assert!(body_str.contains("\"sample_id\":[\"SAMN00000001\"]"));
}

#[test]
fn rank_only_query_is_not_scoped_to_rank_string() {
    let q = make_query("index: taxon\nrank: species\n");
    let params = default_params();
    let mut search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    search_body_input.root_taxon_id = Some("1".to_string());
    let body = build_search_body(&search_body_input).unwrap();
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"taxon_id\":\"1\""));
    assert!(!body_str.contains("\"taxon_id\":\"species\""));
}

#[test]
fn rank_only_query_without_discovered_root_is_unscoped() {
    let q = make_query("index: taxon\nrank: species\n");
    let params = default_params();
    let search_body_input = query_to_body_input("search", &q, &params, None, None, None);
    let body = build_search_body(&search_body_input).unwrap();
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(!body_str.contains("\"lineage\""));
}
