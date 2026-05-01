use genomehubs_query::{build_url_for_endpoint, query::{SearchQuery, QueryParams}};

#[test]
fn builder_returns_empty_for_invalid_yaml() {
    // User-supplied YAML used `op: eq` which is not the expected `operator: eq`.
        let bad_query = "index: taxon\nattributes:\n- name: assembly_level\n  op: eq\n  value: chromosome\n";
    let params = "size: 0\n";
    let url = build_url_for_endpoint(bad_query, params, "http://localhost:9200", "v2", "count");
    assert!(url.is_empty(), "expected empty URL for invalid YAML, got {}", url);
}

#[test]
fn builder_returns_url_for_valid_yaml() {
        let good_query = "index: taxon\nattributes:\n- name: assembly_level\n  operator: eq\n  value: chromosome\n";
    let params = "size: 0\n";
    // Try parsing to get a clearer error if parsing fails.
    println!("GOOD QUERY:\n{}", good_query);
    match SearchQuery::from_yaml(good_query) {
        Ok(_) => {}
        Err(e) => panic!("SearchQuery::from_yaml failed: {}", e),
    }
    match QueryParams::from_yaml(params) {
        Ok(_) => {}
        Err(e) => panic!("QueryParams::from_yaml failed: {}", e),
    }

    let url = build_url_for_endpoint(good_query, params, "http://localhost:9200", "v2", "count");
    assert!(!url.is_empty(), "expected non-empty URL for valid YAML");
}

#[test]
fn builder_accepts_minimal_query() {
    let q = "index: taxon\n";
    let params = "size: 0\n";
    let url = build_url_for_endpoint(q, params, "http://localhost:9200", "v2", "count");
    assert!(!url.is_empty(), "expected non-empty URL for minimal query");
}
