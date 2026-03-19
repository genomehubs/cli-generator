//! API field fetching and local disk caching.
//!
//! [`FieldFetcher`] fetches the `resultFields` JSON from a genomehubs API
//! endpoint and deserialises it into [`FieldDef`] structs.  Results are
//! cached on disk (under `~/.cache/genomehubs-cli-generator/{site}/`) with a
//! 24-hour TTL so subsequent runs are instant without network access.
//!
//! The cache is deliberately transparent: if the cached file exists and is
//! fresh it is returned immediately; otherwise a network request is made and
//! the result is written back to disk.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::config::{IndexDef, SiteConfig};

/// How long a cached field list is considered fresh.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

// ── FieldDef ──────────────────────────────────────────────────────────────────

/// Metadata for a single API field, parsed from the `resultFields` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    /// Internal field name used in API queries, e.g. `"genome_size"`.
    ///
    /// Not present in the inner JSON object (the name is the map key), so we
    /// default to an empty string and set it manually in [`parse_single_field`].
    #[serde(default)]
    pub name: String,
    /// Display group for grouping related fields, e.g. `"genome_size"`.
    #[serde(default)]
    pub display_group: Option<String>,
    /// Human-readable label, e.g. `"Genome size"`.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Short description of the field.
    #[serde(default)]
    pub description: Option<String>,
    /// Field data type as reported by the API, e.g. `"long"`, `"keyword"`.
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
    /// For keyword fields: the set of allowed enum values.
    #[serde(default)]
    pub constraint: Option<FieldConstraint>,
    /// Display priority level (1 = primary, 2 = secondary).
    #[serde(default)]
    pub display_level: Option<u8>,
    /// Alternative names by which this field is also known, e.g. `["ebp_metric_date"]`.
    ///
    /// When a flag's `fields` list or `patterns` references a synonym, the
    /// canonical field name is included in the resolved field set.
    #[serde(default)]
    pub synonyms: Vec<String>,
    /// Processed type used for validation, e.g. `"long"`, `"keyword"`, `"date"`.
    ///
    /// Distinct from `field_type` (the raw storage type); used to decide which
    /// operators are legal (no `<`/`>` for keyword fields).
    #[serde(default)]
    pub processed_type: Option<String>,
    /// Direction of value inheritance across the taxonomy tree.
    ///
    /// `"up"` means values propagate toward the root (ancestors), `"down"`
    /// toward leaves (descendants), `"both"` for bidirectional propagation.
    /// `None` means the field is not inherited.
    #[serde(default)]
    pub traverse_direction: Option<String>,
    /// Valid summary modifiers for this field, e.g. `["min", "max", "median"]`.
    ///
    /// Only aggregate/traversal fields have a non-empty list.
    #[serde(default)]
    pub summary: Vec<String>,
}

/// Constraint metadata for a field, used to enumerate keyword values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConstraint {
    /// Allowed values for keyword-type fields.
    #[serde(rename = "enum", default)]
    pub enum_values: Vec<String>,
}

// ── Cache envelope ────────────────────────────────────────────────────────────

/// Wraps a field list with a timestamp so TTL can be checked without
/// additional filesystem metadata calls.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    fetched_at: DateTime<Utc>,
    fields: Vec<FieldDef>,
}

// ── FieldFetcher ──────────────────────────────────────────────────────────────

/// Fetches and caches API field definitions for a site.
pub struct FieldFetcher {
    cache_dir: PathBuf,
    force_fresh: bool,
    /// When `true` the cache never expires: archive APIs have frozen schemas.
    archive_mode: bool,
}

impl FieldFetcher {
    /// Create a new fetcher.
    ///
    /// `cache_dir` is typically `~/.cache/genomehubs-cli-generator/{site}`.
    /// Set `force_fresh` to `true` to bypass the cache and always hit the network.
    /// Set `archive_mode` to `true` to keep cached data indefinitely (for frozen archive APIs).
    pub fn new(cache_dir: PathBuf, force_fresh: bool) -> Self {
        Self {
            cache_dir,
            force_fresh,
            archive_mode: false,
        }
    }

    /// Enable archive mode: cached field lists are kept indefinitely.
    pub fn with_archive_mode(mut self, archive: bool) -> Self {
        self.archive_mode = archive;
        self
    }

    /// Build the default cache directory for a named site.
    ///
    /// Returns `None` if the OS cache directory cannot be determined.
    pub fn default_cache_dir(site_name: &str) -> Option<PathBuf> {
        dirs::cache_dir().map(|base| base.join("genomehubs-cli-generator").join(site_name))
    }

    /// Fetch field definitions for every index in `site_config`.
    ///
    /// Returns a map from index name to its associated field list.
    pub fn fetch_all(&self, site_config: &SiteConfig) -> Result<HashMap<String, Vec<FieldDef>>> {
        site_config
            .indexes
            .iter()
            .map(|index| {
                let url = site_config.result_fields_url(index);
                let fields = self.fetch_index(&site_config.name, index, &url)?;
                Ok((index.name.clone(), fields))
            })
            .collect()
    }

    /// Fetch field definitions for a single index, using the cache when valid.
    fn fetch_index(&self, site_name: &str, index: &IndexDef, url: &str) -> Result<Vec<FieldDef>> {
        let cache_path = self.cache_path(site_name, &index.name);

        if !self.force_fresh {
            if let Some(cached) = self.load_cache(&cache_path)? {
                return Ok(cached);
            }
        }

        let fields = self
            .fetch_from_api(url, &index.name)
            .with_context(|| format!("fetching fields for index '{}' from {url}", index.name))?;

        self.write_cache(&cache_path, &fields)
            .with_context(|| format!("writing cache to {}", cache_path.display()))?;

        Ok(fields)
    }

    /// Return the cache file path for a given site and index.
    fn cache_path(&self, site_name: &str, index_name: &str) -> PathBuf {
        self.cache_dir
            .join(site_name)
            .join(format!("fields-{index_name}.json"))
    }

    /// Load and validate the cache; return `None` if missing or stale.
    ///
    /// In archive mode the TTL check is skipped — a frozen API's schema never changes.
    fn load_cache(&self, path: &Path) -> Result<Option<Vec<FieldDef>>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path)?;
        let envelope: CacheEnvelope =
            serde_json::from_str(&text).context("deserialising cache file")?;

        if !self.archive_mode {
            let age = Utc::now()
                .signed_duration_since(envelope.fetched_at)
                .to_std()
                .unwrap_or(CACHE_TTL);

            if age >= CACHE_TTL {
                return Ok(None);
            }
        }

        Ok(Some(envelope.fields))
    }

    /// Serialise `fields` to a JSON cache file, creating parent directories.
    fn write_cache(&self, path: &Path, fields: &[FieldDef]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let envelope = CacheEnvelope {
            fetched_at: Utc::now(),
            fields: fields.to_vec(),
        };
        let json = serde_json::to_string_pretty(&envelope)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Perform the actual HTTP GET and parse the response into [`FieldDef`]s.
    fn fetch_from_api(&self, url: &str, index_name: &str) -> Result<Vec<FieldDef>> {
        let response = reqwest::blocking::get(url)
            .with_context(|| format!("HTTP GET {url}"))?
            .error_for_status()
            .with_context(|| format!("non-2xx response from {url}"))?;

        let body: serde_json::Value = response.json().context("deserialising JSON response")?;
        parse_result_fields(&body, index_name)
    }
}

/// Parse the `resultFields` JSON response into a [`Vec<FieldDef>`].
///
/// The API returns `{ "fields": { "<name>": { ... }, ... }, ... }`.
fn parse_result_fields(body: &serde_json::Value, index_name: &str) -> Result<Vec<FieldDef>> {
    let fields_map = body
        .get("fields")
        .and_then(|v| v.as_object())
        .with_context(|| {
            format!("expected 'fields' object in resultFields response for index '{index_name}'")
        })?;

    let mut fields: Vec<FieldDef> = fields_map
        .iter()
        .filter_map(|(name, value)| parse_single_field(name, value))
        .collect();

    fields.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(fields)
}

/// Attempt to deserialise one field entry; return `None` on parse failure
/// (soft-skip malformed entries rather than aborting the whole fetch).
fn parse_single_field(name: &str, value: &serde_json::Value) -> Option<FieldDef> {
    let mut field: FieldDef = serde_json::from_value(value.clone()).ok()?;
    field.name = name.to_string();
    Some(field)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_response() -> serde_json::Value {
        serde_json::json!({
            "fields": {
                "genome_size": {
                    "display_group": "genome_size",
                    "display_name": "Genome size",
                    "description": "Genome size in bases",
                    "type": "long",
                    "display_level": 1
                },
                "assembly_level": {
                    "display_group": "assembly",
                    "display_name": "Assembly level",
                    "type": "keyword",
                    "constraint": { "enum": ["chromosome", "scaffold", "contig"] },
                    "display_level": 1
                }
            }
        })
    }

    #[test]
    fn parse_result_fields_extracts_two_entries() {
        let body = sample_response();
        let fields = parse_result_fields(&body, "taxon").unwrap();
        assert_eq!(fields.len(), 2);
        // sorted alphabetically
        assert_eq!(fields[0].name, "assembly_level");
        assert_eq!(fields[1].name, "genome_size");
    }

    #[test]
    fn parse_result_fields_preserves_display_group() {
        let body = sample_response();
        let fields = parse_result_fields(&body, "taxon").unwrap();
        let gs = fields.iter().find(|f| f.name == "genome_size").unwrap();
        assert_eq!(gs.display_group.as_deref(), Some("genome_size"));
    }

    #[test]
    fn parse_result_fields_preserves_enum_constraint() {
        let body = sample_response();
        let fields = parse_result_fields(&body, "taxon").unwrap();
        let al = fields.iter().find(|f| f.name == "assembly_level").unwrap();
        let constraint = al.constraint.as_ref().unwrap();
        assert!(constraint.enum_values.contains(&"chromosome".to_string()));
    }

    #[test]
    fn parse_result_fields_errors_on_missing_fields_key() {
        let body = serde_json::json!({ "status": "ok" });
        assert!(parse_result_fields(&body, "taxon").is_err());
    }

    #[test]
    fn parse_single_field_deserialises_synonyms() {
        let value = serde_json::json!({
            "display_group": "assembly",
            "display_name": "EBP metric date",
            "type": "date",
            "synonyms": ["ebp_metric_date"]
        });
        let field = parse_single_field("ebp_standard_date", &value).unwrap();
        assert_eq!(field.name, "ebp_standard_date");
        assert_eq!(field.synonyms, vec!["ebp_metric_date"]);
    }

    #[test]
    fn parse_single_field_defaults_synonyms_to_empty() {
        let value = serde_json::json!({
            "display_group": "assembly",
            "type": "keyword"
        });
        let field = parse_single_field("no_synonyms_field", &value).unwrap();
        assert!(field.synonyms.is_empty());
    }

    #[test]
    fn cache_roundtrip_restores_fields() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false);
        let fields = vec![FieldDef {
            name: "genome_size".to_string(),
            display_group: Some("genome_size".to_string()),
            display_name: Some("Genome size".to_string()),
            description: None,
            field_type: Some("long".to_string()),
            constraint: None,
            display_level: Some(1),
            synonyms: vec![],
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        }];
        let cache_path = dir.path().join("site").join("fields-taxon.json");
        fetcher.write_cache(&cache_path, &fields).unwrap();
        let loaded = fetcher.load_cache(&cache_path).unwrap().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "genome_size");
    }

    #[test]
    fn archive_mode_cache_never_expires() {
        use chrono::Duration as ChronoDuration;

        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false).with_archive_mode(true);

        // Write a cache entry whose timestamp is 48 hours in the past.
        let cache_path = dir.path().join("site").join("fields-taxon.json");
        let stale_envelope = CacheEnvelope {
            fetched_at: Utc::now() - ChronoDuration::hours(48),
            fields: vec![FieldDef {
                name: "old_field".to_string(),
                display_group: None,
                display_name: None,
                description: None,
                field_type: None,
                constraint: None,
                display_level: None,
                synonyms: vec![],
                processed_type: None,
                traverse_direction: None,
                summary: vec![],
            }],
        };
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            &cache_path,
            serde_json::to_string_pretty(&stale_envelope).unwrap(),
        )
        .unwrap();

        // In archive mode the stale entry must still be returned.
        let loaded = fetcher.load_cache(&cache_path).unwrap();
        assert!(loaded.is_some(), "archive mode should return stale cache");
        assert_eq!(loaded.unwrap()[0].name, "old_field");
    }

    #[test]
    fn field_def_defaults() {
        let field = FieldDef {
            name: "test_field".to_string(),
            display_group: None,
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec![],
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        };
        assert_eq!(field.name, "test_field");
        assert!(field.synonyms.is_empty());
        assert!(field.summary.is_empty());
    }

    #[test]
    fn field_def_with_synonyms() {
        let field = FieldDef {
            name: "canonical_name".to_string(),
            display_group: None,
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec!["old_name".to_string(), "alternate_name".to_string()],
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        };
        assert_eq!(field.synonyms.len(), 2);
        assert!(field.synonyms.contains(&"old_name".to_string()));
    }

    #[test]
    fn field_constraint_enum_values() {
        let constraint = FieldConstraint {
            enum_values: vec![
                "chromosome".to_string(),
                "scaffold".to_string(),
                "contig".to_string(),
            ],
        };
        assert_eq!(constraint.enum_values.len(), 3);
        assert!(constraint.enum_values.contains(&"scaffold".to_string()));
    }

    #[test]
    fn parse_result_fields_preserves_synonyms() {
        let body = serde_json::json!({
            "fields": {
                "my_field": {
                    "display_group": "group",
                    "display_name": "My Field",
                    "synonyms": ["old_field", "legacy_name"],
                }
            }
        });
        let fields = parse_result_fields(&body, "taxon").unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].synonyms.len(), 2);
    }

    #[test]
    fn field_def_with_traverse_direction() {
        let field = FieldDef {
            name: "test_field".to_string(),
            display_group: None,
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec![],
            processed_type: None,
            traverse_direction: Some("up".to_string()),
            summary: vec![],
        };
        assert_eq!(field.traverse_direction, Some("up".to_string()));
    }

    #[test]
    fn field_def_with_summary_modifiers() {
        let field = FieldDef {
            name: "test_field".to_string(),
            display_group: None,
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec![],
            processed_type: None,
            traverse_direction: None,
            summary: vec!["min".to_string(), "max".to_string()],
        };
        assert_eq!(field.summary.len(), 2);
        assert!(field.summary.contains(&"min".to_string()));
    }

    #[test]
    fn field_fetcher_constructor() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false);
        // Fetcher should be constructed successfully
        assert!(!fetcher.force_fresh); // Default from constructor
    }

    #[test]
    fn field_fetcher_with_archive_mode() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false).with_archive_mode(true);
        // Should be chainable and constructable
        assert!(fetcher.archive_mode);
    }

    #[test]
    fn fetch_from_api_handles_http_500_error() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(500)
            .with_body("Internal Server Error")
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-2xx response"));
        mock.assert();
    }

    #[test]
    fn fetch_from_api_handles_http_502_error() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(502)
            .with_body("Bad Gateway")
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-2xx response"));
        mock.assert();
    }

    #[test]
    fn fetch_from_api_handles_malformed_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(200)
            .with_body("{invalid json")
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("deserialising JSON"));
        mock.assert();
    }

    #[test]
    fn fetch_from_api_handles_missing_fields_key() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(200)
            .with_body(r#"{"status": "ok"}"#)
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 'fields' object"));
        mock.assert();
    }

    #[test]
    fn fetch_from_api_handles_empty_fields_object() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(200)
            .with_body(r#"{"fields": {}}"#)
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_ok());
        let fields = result.unwrap();
        assert!(fields.is_empty());
        mock.assert();
    }

    #[test]
    fn fetch_from_api_parses_valid_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/any")
            .with_status(200)
            .with_body(
                r#"{"fields": {"genome_size": {"type": "long", "display_group": "genome"}}}"#,
            )
            .create();

        let fetcher = FieldFetcher::new(tempfile::tempdir().unwrap().path().to_path_buf(), false);
        let url = format!("{}/any", server.url());
        let result = fetcher.fetch_from_api(&url, "taxon");

        assert!(result.is_ok());
        let fields = result.unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "genome_size");
        mock.assert();
    }

    #[test]
    fn load_cache_handles_corrupted_json() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false);
        let cache_path = dir.path().join("site").join("fields-taxon.json");

        // Create parent dirs and write corrupted JSON
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&cache_path, "{invalid json syntax}").unwrap();

        // Should error on deserialization
        let result = fetcher.load_cache(&cache_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("deserialising cache file"));
    }

    #[test]
    fn load_cache_rejects_stale_entry() {
        use chrono::Duration as ChronoDuration;

        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false);

        // Write a cache entry 48 hours in the past (older than 24h TTL)
        let cache_path = dir.path().join("site").join("fields-taxon.json");
        let stale_envelope = CacheEnvelope {
            fetched_at: Utc::now() - ChronoDuration::hours(48),
            fields: vec![FieldDef {
                name: "stale_field".to_string(),
                display_group: None,
                display_name: None,
                description: None,
                field_type: None,
                constraint: None,
                display_level: None,
                synonyms: vec![],
                processed_type: None,
                traverse_direction: None,
                summary: vec![],
            }],
        };
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            &cache_path,
            serde_json::to_string_pretty(&stale_envelope).unwrap(),
        )
        .unwrap();

        // Should return None (cache expired)
        let result = fetcher.load_cache(&cache_path).unwrap();
        assert!(
            result.is_none(),
            "stale cache should be rejected outside archive mode"
        );
    }

    #[test]
    fn load_cache_accepts_fresh_entry() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = FieldFetcher::new(dir.path().to_path_buf(), false);

        // Write a cache entry just now (definitely fresh)
        let cache_path = dir.path().join("site").join("fields-taxon.json");
        let fresh_envelope = CacheEnvelope {
            fetched_at: Utc::now(),
            fields: vec![FieldDef {
                name: "fresh_field".to_string(),
                display_group: None,
                display_name: None,
                description: None,
                field_type: None,
                constraint: None,
                display_level: None,
                synonyms: vec![],
                processed_type: None,
                traverse_direction: None,
                summary: vec![],
            }],
        };
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            &cache_path,
            serde_json::to_string_pretty(&fresh_envelope).unwrap(),
        )
        .unwrap();

        // Should return Some (cache is valid)
        let result = fetcher.load_cache(&cache_path).unwrap();
        assert!(result.is_some(), "fresh cache should be accepted");
        assert_eq!(result.unwrap()[0].name, "fresh_field");
    }
}
