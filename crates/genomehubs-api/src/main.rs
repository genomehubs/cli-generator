use axum::{routing::get, Extension, Router};
use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
// use toml;
use utoipa::OpenApi;
use utoipa_swagger_ui::{SwaggerUi, Url};

mod es_client;
mod es_metadata;
mod fetch_records;
mod index_name;
mod phylopic_client;
mod report;
mod routes;
mod swagger_customise;

#[derive(Clone)]
pub struct AppState {
    pub es_base: String,
    pub default_result: String,
    pub default_taxonomy: String,
    pub default_version: String,
    pub hub_name: String,
    pub index_separator: String,
    pub index_suffix: Option<String>,
    pub cache: Option<std::sync::Arc<tokio::sync::RwLock<es_metadata::MetadataCache>>>,
    pub client: reqwest::Client,
    pub phylopic_cache: Arc<tokio::sync::RwLock<phylopic_client::PhylopicCache>>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "GenomeHubs API",
        description = "GenomeHubs Application Programming Interface.\n\nProvides a low-level API to retrieve records, look up taxa, search and generate reports across all taxon, assembly, and sample data.\n\nSee the [GenomeHubs documentation](https://genomehubs.org) for further details.",
        version = "3.0.0",
        contact(
            name = "GenomeHubs",
            url = "https://genomehubs.org"
        ),
        license(
            name = "MIT License",
            url = "https://github.com/genomehubs/genomehubs/blob/main/LICENSE"
        )
    ),
    tags(
        (name = "Data", description = "Retrieve search results, records and reports"),
        (name = "Metadata", description = "List available indices, fields, taxonomies and ranks"),
        (name = "External", description = "Fetch external resources"),
        (name = "Status", description = "API health and version information")
    ),
    paths(
        routes::count::get_count,
        routes::count::post_count,
        routes::count_batch::post_count_batch,
        routes::lookup::get_lookup,
        routes::metadata::get_metadata,
        routes::phylopic::get_phylopic,
        routes::phylopic::post_phylopic_batch,
        routes::record::get_record,
        routes::report::get_report,
        routes::report::post_report,
        routes::result_fields::get_result_fields,
        routes::search::get_search,
        routes::search::post_search,
        routes::search_batch::post_search_batch,
        routes::status::get_status,
        routes::summary::get_summary,
        routes::taxonomies::get_taxonomies_openapi,
        routes::taxonomic_ranks::get_taxonomic_ranks_openapi,
        routes::indices::get_indices_openapi,
    ),
    components(schemas(
        routes::ApiStatus,
        routes::count::CountResponse,
        routes::count_batch::CountBatchItem,
        routes::count_batch::CountBatchRequest,
        routes::count_batch::CountBatchResponse,
        routes::count_batch::CountBatchResultItem,
        routes::lookup::LookupResponse,
        routes::lookup::LookupResult,
        phylopic_client::PhylopicRecord,
        phylopic_client::PhylopicSource,
        routes::phylopic::PhylopicBatchRequest,
        routes::phylopic::PhylopicBatchResponse,
        routes::phylopic::PhylopicResponse,
        routes::record::RecordItem,
        routes::record::RecordQuery,
        routes::record::RecordResponse,
        routes::report::ReportRequest,
        routes::report::ReportResponse,
        routes::metadata::MetadataResponse,
        routes::result_fields::FieldMeta,
        routes::result_fields::ResultFieldsResponse,
        routes::search::SearchRequest,
        routes::search::SearchResponse,
        routes::search_batch::SearchBatchItem,
        routes::search_batch::SearchBatchRequest,
        routes::search_batch::SearchBatchResponse,
        routes::search_batch::SearchBatchResultItem,
        routes::status::StatusResponse,
        routes::summary::SummaryItem,
        routes::summary::SummaryQuery,
        routes::summary::SummaryResponse,
        routes::taxonomies::TaxonomiesResponse,
        routes::taxonomic_ranks::RanksResponse,
        routes::indices::IndicesResponse,
    ))
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Load ES integration config (allow override via ES_INTEGRATION_CONFIG)
    #[derive(serde::Deserialize)]
    struct EsConfig {
        base_url: String,
        default_result: Option<String>,
        default_taxonomy: Option<String>,
        default_version: Option<String>,
        hub_name: Option<String>,
        index_separator: Option<String>,
        /// Optional path to a swagger customisation YAML file.
        swagger_examples: Option<String>,
    }

    // Locate config: env override -> search upwards for config/es_integration.toml -> example
    let cfg_path = if let Ok(envp) = std::env::var("ES_INTEGRATION_CONFIG") {
        PathBuf::from(envp)
    } else {
        let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut found: Option<PathBuf> = None;
        loop {
            let candidate = dir.join("config/es_integration.toml");
            if candidate.exists() {
                found = Some(candidate);
                break;
            }
            if !dir.pop() {
                break;
            }
        }
        found.unwrap_or_else(|| PathBuf::from("config/es_integration.toml.example"))
    };

    let mut es_base = "http://localhost:9200".to_string();
    let mut default_result = "taxon".to_string();
    let mut default_taxonomy = "ncbi".to_string();
    let mut default_version = "2021.10.15".to_string();
    let mut hub_name = "goat".to_string();
    let mut index_separator = "--".to_string();
    let mut swagger_examples_path: Option<String> = None;

    match fs::read_to_string(&cfg_path) {
        Ok(raw) => match toml::from_str::<EsConfig>(&raw) {
            Ok(cfg) => {
                es_base = cfg.base_url;
                if let Some(def) = cfg.default_result {
                    default_result = def;
                }
                if let Some(tax) = cfg.default_taxonomy {
                    default_taxonomy = tax;
                }
                if let Some(ver) = cfg.default_version {
                    default_version = ver;
                }
                if let Some(hub) = cfg.hub_name {
                    hub_name = hub;
                }
                if let Some(sep) = cfg.index_separator {
                    index_separator = sep;
                }
                swagger_examples_path = cfg.swagger_examples;
            }
            Err(e) => {
                tracing::error!(path = %cfg_path.display(), error = %e, "Failed to parse ES integration config");
            }
        },
        Err(_) => {
            tracing::warn!(path = %cfg_path.display(), "ES integration config not found — using defaults");
        }
    }

    let index_suffix = format!(
        "{}{}{}{}{}",
        index_separator, default_taxonomy, index_separator, hub_name, index_separator
    ) + &default_version;

    // Create the HTTP client once and share it across handlers
    let client = reqwest::Client::new();

    // Fetch initial PhyloPic build number (best-effort; 0 means "unknown").
    let initial_build = phylopic_client::fetch_current_build(&client)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not fetch initial PhyloPic build — starting at 0");
            0
        });

    let phylopic_cache = Arc::new(tokio::sync::RwLock::new(phylopic_client::PhylopicCache {
        current_build: initial_build,
        ..Default::default()
    }));
    tracing::info!(build = initial_build, "PhyloPic build fetched");

    let state = Arc::new(AppState {
        es_base: es_base.clone(),
        default_result: default_result.clone(),
        default_taxonomy: default_taxonomy.clone(),
        default_version: default_version.clone(),
        hub_name: hub_name.clone(),
        index_separator: index_separator.clone(),
        index_suffix: Some(index_suffix.clone()),
        cache: Some(std::sync::Arc::new(tokio::sync::RwLock::new(
            es_metadata::MetadataCache::default(),
        ))),
        client: client.clone(),
        phylopic_cache: phylopic_cache.clone(),
    });

    // Log effective configuration for debugging
    tracing::info!(file = %cfg_path.display(), base_url = %es_base, default_result = %default_result, index_suffix = ?index_suffix, "ES config");

    // Spawn background task to refresh the PhyloPic build number every 24 hours.
    let refresh_client = client.clone();
    let refresh_cache = phylopic_cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86_400));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            match phylopic_client::fetch_current_build(&refresh_client).await {
                Ok(build) => {
                    let mut cache = refresh_cache.write().await;
                    cache.current_build = build;
                    tracing::info!(build, "PhyloPic build number refreshed");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "PhyloPic build refresh failed; retaining current build");
                }
            }
        }
    });

    let openapi = ApiDoc::openapi();

    // Build the runtime-patched OpenAPI JSON value.
    // Apply site-specific customisations from the swagger_examples file (if set).
    let mut openapi_val = serde_json::to_value(&openapi).expect("OpenAPI serialisation failed");
    if let Some(ref path) = swagger_examples_path {
        match swagger_customise::load(path) {
            Ok(customisation) => {
                swagger_customise::apply_to_json(&mut openapi_val, &customisation);
                tracing::info!(path, "Swagger customisation applied");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load swagger customisation — using defaults");
            }
        }
    }

    // Populate cache on startup (blocking with retry). If this errors the app will not start.
    // Spawn the populate step now and await it so server only starts when cache populated.
    match es_metadata::populate_with_retry(state.clone(), &state.client, None).await {
        Ok(_) => tracing::info!("metadata cache populated"),
        Err(e) => tracing::error!(error = %e, "failed to populate metadata cache on startup"),
    }

    let app = Router::<()>::new()
        .route(
            "/api/v3/count",
            axum::routing::get(routes::count::get_count).post(routes::count::post_count),
        )
        .route(
            "/api/v3/count/batch",
            axum::routing::post(routes::count_batch::post_count_batch),
        )
        .route("/api/v3/lookup", get(routes::lookup::get_lookup))
        .route("/api/v3/metadata", get(routes::metadata::get_metadata))
        .route(
            "/api/v3/metadata/indices",
            get(routes::indices::get_indices),
        )
        .route(
            "/api/v3/metadata/fields",
            get(routes::result_fields::get_result_fields),
        )
        .route(
            "/api/v3/metadata/ranks",
            get(routes::taxonomic_ranks::get_taxonomic_ranks),
        )
        .route(
            "/api/v3/metadata/taxonomies",
            get(routes::taxonomies::get_taxonomies),
        )
        .route("/api/v3/phylopic", get(routes::phylopic::get_phylopic))
        .route(
            "/api/v3/phylopic/batch",
            axum::routing::post(routes::phylopic::post_phylopic_batch),
        )
        .route("/api/v3/record", get(routes::record::get_record))
        .route(
            "/api/v3/report",
            axum::routing::get(routes::report::get_report).post(routes::report::post_report),
        )
        .route(
            "/api/v3/search",
            axum::routing::get(routes::search::get_search).post(routes::search::post_search),
        )
        .route(
            "/api/v3/search/batch",
            axum::routing::post(routes::search_batch::post_search_batch),
        )
        .route("/api/v3/status", get(routes::status::get_status))
        .route("/api/v3/summary", get(routes::summary::get_summary))
        .layer(Extension(state))
        .merge(SwaggerUi::new("/swagger-ui").external_url_unchecked(
            Url::new("API Documentation", "/api-doc/openapi.json"),
            openapi_val,
        ));

    // Write the patched OpenAPI JSON to target/ for inspection.
    let out_path = "./target/openapi.json";
    std::fs::create_dir_all("./target").ok();
    std::fs::write(out_path, serde_json::to_string_pretty(&openapi).unwrap())
        .expect("failed to write openapi.json");
    tracing::info!(path = %out_path, "Wrote OpenAPI");

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!(addr = %addr, "Listening on");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;
    use axum::Extension;
    use axum::Json;
    use serde_json::json;

    #[tokio::test]
    async fn status_and_cache_routes_work() {
        // build a dummy cache
        let cache = es_metadata::MetadataCache {
            taxonomies: vec!["ncbi".to_string(), "other".to_string()],
            indices: vec!["attributes".to_string(), "taxon".to_string()],
            taxonomic_ranks: vec!["species".to_string(), "genus".to_string()],
            attr_types: json!({
                "taxon": {"kingdom": {"type": "keyword", "summary": "kingdom"}}
            }),
            last_updated: Some("now".to_string()),
            has_sayt_field: true,
            has_trigram_field: false,
        };
        let state = std::sync::Arc::new(AppState {
            es_base: "http://localhost:9200".to_string(),
            default_result: "taxon".to_string(),
            default_taxonomy: "ncbi".to_string(),
            default_version: "2021.10.15".to_string(),
            hub_name: "goat".to_string(),
            index_separator: "--".to_string(),
            index_suffix: Some("--ncbi--goat--2021.10.15".to_string()),
            cache: Some(std::sync::Arc::new(tokio::sync::RwLock::new(cache))),
            client: reqwest::Client::new(),
            phylopic_cache: Arc::new(tokio::sync::RwLock::new(
                phylopic_client::PhylopicCache::default(),
            )),
        });

        // status
        let Json(status_body) = routes::status::get_status(Extension(state.clone())).await;
        assert!(status_body.status.success);
        assert!(status_body.ready);

        // taxonomies
        let Json(tax) = routes::taxonomies::get_taxonomies(Extension(state.clone())).await;
        assert!(tax.status.success);
        assert!(tax.taxonomies.contains(&"ncbi".to_string()));

        // ranks
        let Json(ranks) =
            routes::taxonomic_ranks::get_taxonomic_ranks(Extension(state.clone())).await;
        assert!(ranks.status.success);
        assert!(ranks.ranks.contains(&"species".to_string()));

        // indices
        let Json(idx) = routes::indices::get_indices(Extension(state.clone())).await;
        assert!(idx.status.success);
        assert!(idx.indices.contains(&"attributes".to_string()));

        // metadata (aggregated)
        let Json(meta) = routes::metadata::get_metadata(Extension(state.clone())).await;
        assert!(meta.status.success);
        assert!(meta.indices.contains(&"taxon".to_string()));
        assert!(meta.taxonomies.contains(&"ncbi".to_string()));
        assert!(meta.ranks.contains(&"species".to_string()));
        assert_eq!(meta.versions, vec!["2021.10.15".to_string()]);

        // resultFields
        let q = Query(routes::result_fields::ResultFieldsQuery {
            result: Some("taxon".to_string()),
        });
        let Json(rf) = routes::result_fields::get_result_fields(q, Extension(state.clone())).await;
        assert!(rf.status.success);
    }
}
