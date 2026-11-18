Live query demo:

Run the example that issues a minimal count query against a configured
Elasticsearch instance (reads `config/es_integration.toml` or the path in
`ES_INTEGRATION_CONFIG`).

```bash
cargo run --example live_query_demo
```

Pass explicit params:

```bash
cargo run --example live_query_demo -- --result taxon --taxa "Mammalia" --fields "genome_size"
```
