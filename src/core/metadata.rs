//! Runtime metadata lookup for the generic CLI.
//!
//! Fetches the `metadata/fields` list for an index from any GenomeHubs API and
//! caches it on disk.  Nothing about field definitions is compiled into the
//! binary: the cache is keyed by site (or by API base for custom instances).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::core::fetch::{FieldDef, FieldFetcher};

/// Number of SHA-256 bytes used in the cache key for an API base URL.
const API_BASE_HASH_BYTES: usize = 4;

/// Options controlling where metadata is cached and whether it is refreshed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataOptions {
    /// Root directory under which per-site metadata caches are written.
    pub cache_root: PathBuf,
    /// When `true`, ignore any cached metadata and fetch from the API.
    pub refresh: bool,
}

impl MetadataOptions {
    /// Build options from an explicit cache directory or the OS default.
    pub fn new(cache_dir: Option<&Path>, refresh: bool) -> Result<Self> {
        let cache_root = match cache_dir {
            Some(dir) => dir.to_path_buf(),
            None => default_cache_root()?,
        };
        Ok(Self {
            cache_root,
            refresh,
        })
    }
}

/// Return the default metadata cache root, `{OS cache}/genomehubs-cli-generator/runtime`.
pub fn default_cache_root() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|base| base.join("genomehubs-cli-generator").join("runtime"))
        .context("no OS cache directory available; pass --metadata-cache-dir")
}

/// Build the `metadata/fields` URL for an index, e.g. `…/api/v3/metadata/fields?result=taxon`.
pub fn metadata_fields_url(api_base: &str, index_name: &str) -> Result<String> {
    let endpoint = format!("{}/v3/metadata/fields", api_base.trim_end_matches('/'));
    let url = reqwest::Url::parse_with_params(&endpoint, &[("result", index_name)])?;
    Ok(url.to_string())
}

/// Build the cache key for an API base: `{site}-{short hash}`.
///
/// The hash keeps custom instances (no preset name) and differing API bases
/// from sharing a cache entry.  Custom instances use the label `custom`.
pub fn cache_key(site_name: &str, api_base: &str) -> String {
    let digest = Sha256::digest(api_base.as_bytes());
    let short_hash = hex::encode(&digest[..API_BASE_HASH_BYTES]);
    let label = if site_name.is_empty() {
        "custom"
    } else {
        site_name
    };
    format!("{label}-{short_hash}")
}

/// Load the field definitions for an index, using the cache when fresh.
pub fn load_result_fields(
    api_base: &str,
    site_name: &str,
    index_name: &str,
    options: &MetadataOptions,
) -> Result<Vec<FieldDef>> {
    let url = metadata_fields_url(api_base, index_name)?;
    let key = cache_key(site_name, api_base);
    let fetcher = FieldFetcher::new(options.cache_root.clone(), options.refresh);
    fetcher.fetch_index(&key, index_name, &url)
}

/// Return the requested field names that are not known to the index.
///
/// Modifiers after a colon (e.g. `genome_size:min`) are ignored, and a field
/// is also accepted when it is listed as a synonym of a known field.
pub fn unknown_field_names(fields: &[FieldDef], requested: &[String]) -> Vec<String> {
    requested
        .iter()
        .map(|raw| raw.split(':').next().unwrap_or("").trim().to_string())
        .filter(|name| !name.is_empty() && !is_known_field(fields, name))
        .collect()
}

/// Return `true` when `name` matches a field name or synonym.
fn is_known_field(fields: &[FieldDef], name: &str) -> bool {
    fields
        .iter()
        .any(|field| field.name == name || field.synonyms.iter().any(|s| s == name))
}

/// Fail with a descriptive error when any requested field is unknown to the index.
pub fn validate_fields(
    api_base: &str,
    site_name: &str,
    index_name: &str,
    requested: &[String],
    options: &MetadataOptions,
) -> Result<()> {
    if requested.is_empty() {
        return Ok(());
    }
    let fields = load_result_fields(api_base, site_name, index_name, options)?;
    let unknown = unknown_field_names(&fields, requested);
    if unknown.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "unknown field(s) for index '{index_name}': {}",
        unknown.join(", ")
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const GOAT_API: &str = "https://goat.genomehubs.org/api";

    fn field(name: &str, synonyms: &[&str]) -> FieldDef {
        FieldDef {
            name: name.to_string(),
            display_group: None,
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: synonyms.iter().map(|s| s.to_string()).collect(),
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        }
    }

    #[test]
    fn metadata_fields_url_appends_v3_path_and_result() {
        let url = metadata_fields_url(GOAT_API, "taxon").unwrap();
        assert_eq!(
            url,
            "https://goat.genomehubs.org/api/v3/metadata/fields?result=taxon"
        );
    }

    #[test]
    fn metadata_fields_url_ignores_trailing_slash() {
        let url = metadata_fields_url("https://goat.genomehubs.org/api/", "assembly").unwrap();
        assert_eq!(
            url,
            "https://goat.genomehubs.org/api/v3/metadata/fields?result=assembly"
        );
    }

    #[test]
    fn cache_key_uses_site_label_and_is_stable() {
        let key = cache_key("goat", GOAT_API);
        assert!(key.starts_with("goat-"));
        assert_eq!(key, cache_key("goat", GOAT_API));
    }

    #[test]
    fn cache_key_uses_custom_label_without_site() {
        assert!(cache_key("", "https://example.com/api").starts_with("custom-"));
    }

    #[test]
    fn cache_key_differs_for_different_api_bases() {
        assert_ne!(
            cache_key("", "https://a.example.com/api"),
            cache_key("", "https://b.example.com/api")
        );
    }

    #[test]
    fn unknown_field_names_ignores_known_fields_modifiers_and_synonyms() {
        let fields = vec![field("genome_size", &["gs"]), field("assembly_level", &[])];
        let requested = vec![
            "genome_size:min".to_string(),
            "gs".to_string(),
            "assembly_level".to_string(),
            "bogus".to_string(),
        ];
        assert_eq!(
            unknown_field_names(&fields, &requested),
            vec!["bogus".to_string()]
        );
    }

    #[test]
    fn validate_fields_skips_fetch_when_no_fields_requested() {
        let options = MetadataOptions {
            cache_root: PathBuf::from("/nonexistent/cache"),
            refresh: false,
        };
        assert!(validate_fields("http://127.0.0.1:1/api", "x", "taxon", &[], &options).is_ok());
    }

    #[test]
    fn load_result_fields_fetches_and_caches() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/api/v3/metadata/fields")
            .match_query(mockito::Matcher::UrlEncoded(
                "result".into(),
                "taxon".into(),
            ))
            .with_status(200)
            .with_body(r#"{"fields":{"genome_size":{"type":"long"}}}"#)
            .expect(1)
            .create();

        let cache_dir = tempfile::tempdir().unwrap();
        let options = MetadataOptions {
            cache_root: cache_dir.path().to_path_buf(),
            refresh: false,
        };
        let api_base = format!("{}/api", server.url());

        let first = load_result_fields(&api_base, "", "taxon", &options).unwrap();
        let second = load_result_fields(&api_base, "", "taxon", &options).unwrap();

        assert_eq!(first[0].name, "genome_size");
        assert_eq!(second[0].name, "genome_size");
        mock.assert();
    }

    #[test]
    fn validate_fields_rejects_unknown_field_from_api() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/api/v3/metadata/fields")
            .match_query(mockito::Matcher::UrlEncoded(
                "result".into(),
                "taxon".into(),
            ))
            .with_status(200)
            .with_body(r#"{"fields":{"genome_size":{"type":"long"}}}"#)
            .create();

        let cache_dir = tempfile::tempdir().unwrap();
        let options = MetadataOptions {
            cache_root: cache_dir.path().to_path_buf(),
            refresh: false,
        };
        let api_base = format!("{}/api", server.url());
        let requested = vec!["nope".to_string()];

        let err = validate_fields(&api_base, "", "taxon", &requested, &options)
            .unwrap_err()
            .to_string();

        assert!(err.contains("unknown field(s) for index 'taxon': nope"));
        mock.assert();
    }
}
