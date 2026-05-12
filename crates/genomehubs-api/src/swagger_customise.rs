//! Runtime customisation for the Swagger UI, loaded from a YAML file.
//!
//! Point the API at a customisation file by adding one line to
//! `es_integration.toml` (or its environment-variable override):
//!
//! ```toml
//! swagger_examples = "config/swagger-examples-goat.yaml"
//! ```
//!
//! The YAML file controls two things:
//!
//! * **`info`** — overrides the OpenAPI `info` block (title, description,
//!   contact, licence) shown at the top of the Swagger UI page.
//! * **`examples`** — provides site-specific request-body examples for POST
//!   endpoints.  Any examples listed for an endpoint *replace* the
//!   compile-time examples for that endpoint; endpoints not mentioned keep
//!   their compile-time defaults.
//!
//! Relative paths in `swagger_examples` are resolved from the process working
//! directory (the project root when run via `cargo run`).  Use absolute paths
//! inside Docker containers.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

// ── Public types ─────────────────────────────────────────────────────────────

/// Top-level structure of a swagger customisation YAML file.
#[derive(Debug, Deserialize)]
pub struct SwaggerCustomisation {
    /// Override fields in the OpenAPI `info` object.
    pub info: Option<InfoOverride>,

    /// Per-endpoint request-body examples.
    ///
    /// All examples provided for a given `path` + `method` pair replace any
    /// compile-time examples for that endpoint atomically.
    #[serde(default)]
    pub examples: Vec<EndpointExample>,
}

/// Overrideable subset of the OpenAPI `info` object.
#[derive(Debug, Deserialize)]
pub struct InfoOverride {
    /// Replaces the API title shown in the Swagger UI banner.
    pub title: Option<String>,
    /// Replaces the API description (Markdown supported).
    pub description: Option<String>,
    /// Replaces the contact block.
    pub contact: Option<ContactOverride>,
    /// Replaces the licence block.
    pub license: Option<LicenseOverride>,
}

/// Contact details shown in the Swagger UI info section.
#[derive(Debug, Deserialize)]
pub struct ContactOverride {
    pub name: Option<String>,
    pub url: Option<String>,
    pub email: Option<String>,
}

/// Licence details shown in the Swagger UI info section.
#[derive(Debug, Deserialize)]
pub struct LicenseOverride {
    /// Licence name (required by OpenAPI spec).
    pub name: String,
    pub url: Option<String>,
}

/// A single named request-body example for one path + method.
#[derive(Debug, Deserialize)]
pub struct EndpointExample {
    /// API path, e.g. `"/api/v3/count"`.
    pub path: String,
    /// HTTP method in lowercase, e.g. `"post"`.
    pub method: String,
    /// Key used in the OpenAPI `examples` map, e.g. `"mammalia_count"`.
    pub name: String,
    /// One-line description shown in the Swagger UI example dropdown.
    pub summary: Option<String>,
    /// The example request body (serialised as a JSON object).
    pub value: Value,
}

// ── Public functions ──────────────────────────────────────────────────────────

/// Load a [`SwaggerCustomisation`] from a YAML file on disk.
///
/// Returns `Err(message)` on I/O or parse failure; the caller should log the
/// message and continue with the unmodified spec.
pub fn load(path: &str) -> Result<SwaggerCustomisation, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read swagger_examples file '{}': {}", path, e))?;
    serde_yaml::from_str(&raw)
        .map_err(|e| format!("YAML parse error in swagger_examples '{}': {}", path, e))
}

/// Apply a loaded customisation to a serialised OpenAPI JSON value in place.
///
/// `openapi` must be the [`serde_json::Value`] produced by serialising a
/// `utoipa::openapi::OpenApi` (e.g. `serde_json::to_value(&ApiDoc::openapi())`).
pub fn apply_to_json(openapi: &mut Value, customisation: &SwaggerCustomisation) {
    apply_info(openapi, customisation);

    // Group by (path, method) so each endpoint's examples are replaced atomically.
    let mut by_endpoint: HashMap<(&str, &str), Vec<&EndpointExample>> = HashMap::new();
    for ex in &customisation.examples {
        by_endpoint
            .entry((ex.path.as_str(), ex.method.as_str()))
            .or_default()
            .push(ex);
    }

    for ((path, method), examples) in by_endpoint {
        apply_endpoint_examples(openapi, path, method, &examples);
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn apply_info(openapi: &mut Value, customisation: &SwaggerCustomisation) {
    let Some(ov) = &customisation.info else {
        return;
    };

    if let Some(t) = &ov.title {
        openapi["info"]["title"] = json!(t);
    }
    if let Some(d) = &ov.description {
        openapi["info"]["description"] = json!(d);
    }
    if let Some(c) = &ov.contact {
        let mut m = serde_json::Map::new();
        if let Some(v) = &c.name {
            m.insert("name".into(), json!(v));
        }
        if let Some(v) = &c.url {
            m.insert("url".into(), json!(v));
        }
        if let Some(v) = &c.email {
            m.insert("email".into(), json!(v));
        }
        openapi["info"]["contact"] = Value::Object(m);
    }
    if let Some(l) = &ov.license {
        let mut m = serde_json::Map::new();
        m.insert("name".into(), json!(l.name));
        if let Some(v) = &l.url {
            m.insert("url".into(), json!(v));
        }
        openapi["info"]["license"] = Value::Object(m);
    }
}

fn apply_endpoint_examples(
    openapi: &mut Value,
    path: &str,
    method: &str,
    examples: &[&EndpointExample],
) {
    // Build a JSON Pointer (RFC 6901) to the "application/json" media-type object.
    // '~' → '~0',  '/' → '~1'
    let escaped_path = path.replace('~', "~0").replace('/', "~1");
    let pointer = format!(
        "/paths/{}/{}/requestBody/content/application~1json",
        escaped_path,
        method.to_lowercase()
    );

    let Some(media_type) = openapi.pointer_mut(&pointer) else {
        tracing::warn!(
            path,
            method,
            "swagger_examples: endpoint not found or has no application/json body — skipping"
        );
        return;
    };

    let Some(mt_obj) = media_type.as_object_mut() else {
        return;
    };

    let mut new_examples = serde_json::Map::new();
    for ex in examples {
        let mut obj = serde_json::Map::new();
        if let Some(s) = &ex.summary {
            obj.insert("summary".into(), json!(s));
        }
        obj.insert("value".into(), ex.value.clone());
        new_examples.insert(ex.name.clone(), Value::Object(obj));
    }
    mt_obj.insert("examples".into(), Value::Object(new_examples));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_openapi() -> Value {
        json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Default Title",
                "version": "3.0.0"
            },
            "paths": {
                "/api/v3/count": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {},
                                    "examples": {
                                        "compile_time": {
                                            "summary": "Compile-time example",
                                            "value": {"query_yaml": "original"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn info_title_overridden() {
        let mut spec = base_openapi();
        let c = SwaggerCustomisation {
            info: Some(InfoOverride {
                title: Some("GoaT API".into()),
                description: None,
                contact: None,
                license: None,
            }),
            examples: vec![],
        };
        apply_to_json(&mut spec, &c);
        assert_eq!(spec["info"]["title"], json!("GoaT API"));
    }

    #[test]
    fn info_contact_inserted() {
        let mut spec = base_openapi();
        let c = SwaggerCustomisation {
            info: Some(InfoOverride {
                title: None,
                description: Some("A description.".into()),
                contact: Some(ContactOverride {
                    name: Some("GoaT".into()),
                    url: Some("https://goat.genomehubs.org".into()),
                    email: Some("goat@genomehubs.org".into()),
                }),
                license: None,
            }),
            examples: vec![],
        };
        apply_to_json(&mut spec, &c);
        assert_eq!(spec["info"]["contact"]["name"], json!("GoaT"));
        assert_eq!(spec["info"]["description"], json!("A description."));
        // Original title untouched
        assert_eq!(spec["info"]["title"], json!("Default Title"));
    }

    #[test]
    fn examples_replace_compile_time() {
        let mut spec = base_openapi();
        let c = SwaggerCustomisation {
            info: None,
            examples: vec![EndpointExample {
                path: "/api/v3/count".into(),
                method: "post".into(),
                name: "mammalia_count".into(),
                summary: Some("Count Mammalia".into()),
                value: json!({"query_yaml": "tax_tree(Mammalia)", "params_yaml": "size: 0"}),
            }],
        };
        apply_to_json(&mut spec, &c);
        let examples = &spec["paths"]["/api/v3/count"]["post"]["requestBody"]["content"]
            ["application/json"]["examples"];
        // Compile-time example replaced
        assert!(examples["compile_time"].is_null());
        // New example present
        assert_eq!(
            examples["mammalia_count"]["summary"],
            json!("Count Mammalia")
        );
    }

    #[test]
    fn unknown_path_skipped_gracefully() {
        let mut spec = base_openapi();
        let c = SwaggerCustomisation {
            info: None,
            examples: vec![EndpointExample {
                path: "/api/v3/does_not_exist".into(),
                method: "post".into(),
                name: "x".into(),
                summary: None,
                value: json!({}),
            }],
        };
        // Should not panic
        apply_to_json(&mut spec, &c);
    }
}
