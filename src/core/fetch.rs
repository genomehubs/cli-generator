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
}

impl FieldFetcher {
    /// Create a new fetcher.
    ///
    /// `cache_dir` is typically `~/.cache/genomehubs-cli-generator/{site}`.
    /// Set `force_fresh` to `true` to bypass the cache and always hit the network.
    pub fn new(cache_dir: PathBuf, force_fresh: bool) -> Self {
        Self {
            cache_dir,
            force_fresh,
        }
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
    fn load_cache(&self, path: &Path) -> Result<Option<Vec<FieldDef>>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path)?;
        let envelope: CacheEnvelope =
            serde_json::from_str(&text).context("deserialising cache file")?;

        let age = Utc::now()
            .signed_duration_since(envelope.fetched_at)
            .to_std()
            .unwrap_or(CACHE_TTL);

        if age >= CACHE_TTL {
            return Ok(None);
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
        }];
        let cache_path = dir.path().join("site").join("fields-taxon.json");
        fetcher.write_cache(&cache_path, &fields).unwrap();
        let loaded = fetcher.load_cache(&cache_path).unwrap().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "genome_size");
    }
}
