use std::collections::HashMap;

fn run_flat_params_example() {
    let mut params = HashMap::new();
    params.insert("result".to_string(), "taxon".to_string());
    params.insert("taxa".to_string(), "Mammalia, !Felis".to_string());
    params.insert("taxon_filter_type".to_string(), "tree".to_string());
    params.insert(
        "fields".to_string(),
        "genome_size, gc_percentage".to_string(),
    );
    params.insert("size".to_string(), "20".to_string());
    params.insert("sortOrder".to_string(), "desc".to_string());

    let (query, qparams) =
        cli_generator::core::query::adapter::parse_url_params(&params).expect("parse");

    println!("--- Flat URL params example ---");
    println!("SearchQuery:\n{}", serde_yaml::to_string(&query).unwrap());
    println!("QueryParams:\n{}", serde_yaml::to_string(&qparams).unwrap());
}

fn run_yaml_example() {
    let mut params = HashMap::new();
    params.insert(
        "query_yaml".to_string(),
        "index: assembly\nassemblies: [GCF_000002305.6]\n".to_string(),
    );
    params.insert("params_yaml".to_string(), "size: 5\npage: 2\n".to_string());

    let (query, qparams) =
        cli_generator::core::query::adapter::parse_url_params(&params).expect("parse yaml");

    println!("--- YAML example ---");
    println!("SearchQuery:\n{}", serde_yaml::to_string(&query).unwrap());
    println!("QueryParams:\n{}", serde_yaml::to_string(&qparams).unwrap());
}

fn main() {
    run_flat_params_example();
    println!();
    run_yaml_example();
}
