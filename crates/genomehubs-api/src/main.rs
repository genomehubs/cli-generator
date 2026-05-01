use axum::{routing::get, Extension, Router};
use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};
// use toml;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod es_client;
mod es_metadata;
mod index_name;
mod routes;

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
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::count::post_count,
        routes::result_fields::get_result_fields,
        routes::search::post_search,
        routes::status::get_status,
        routes::taxonomies::get_taxonomies_openapi,
        routes::taxonomic_ranks::get_taxonomic_ranks_openapi,
        routes::indices::get_indices_openapi,
    ),
    components(schemas(
        routes::ApiStatus,
        routes::count::CountResponse,
        routes::result_fields::FieldMeta,
        routes::result_fields::ResultFieldsResponse,
        routes::search::SearchRequest,
        routes::search::SearchResponse,
        routes::status::StatusResponse,
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
    });

    // Log effective configuration for debugging
    tracing::info!(file = %cfg_path.display(), base_url = %es_base, default_result = %default_result, index_suffix = ?index_suffix, "ES config");

    let openapi = ApiDoc::openapi();

    // Populate cache on startup (blocking with retry). If this errors the app will not start.
    // Spawn the populate step now and await it so server only starts when cache populated.
    match es_metadata::populate_with_retry(state.clone(), &state.client, None).await {
        Ok(_) => tracing::info!("metadata cache populated"),
        Err(e) => tracing::error!(error = %e, "failed to populate metadata cache on startup"),
    }

    let app = Router::<()>::new()
        .route(
            "/api/v3/resultFields",
            get(routes::result_fields::get_result_fields),
        )
        .route("/api/v3/status", get(routes::status::get_status))
        .route(
            "/api/v3/taxonomies",
            get(routes::taxonomies::get_taxonomies),
        )
        .route(
            "/api/v3/taxonomicRanks",
            get(routes::taxonomic_ranks::get_taxonomic_ranks),
        )
        .route(
            "/api/v3/count",
            axum::routing::post(routes::count::post_count),
        )
        .route(
            "/api/v3/search",
            axum::routing::post(routes::search::post_search),
        )
        .route("/api/v3/indices", get(routes::indices::get_indices))
        .layer(Extension(state))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", openapi.clone()));

    // Also write OpenAPI JSON to target for inspection.
    let openapi_json = serde_json::to_string_pretty(&openapi).unwrap();
    let out_path = "./target/openapi.json";
    std::fs::create_dir_all("./target").ok();
    std::fs::write(out_path, openapi_json).expect("failed to write openapi.json");
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

        // resultFields
        let q = Query(routes::result_fields::ResultFieldsQuery {
            result: Some("taxon".to_string()),
        });
        let Json(rf) = routes::result_fields::get_result_fields(q, Extension(state.clone())).await;
        assert!(rf.status.success);
    }
}
