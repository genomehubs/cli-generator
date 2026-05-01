//! `cli-generator validate` — check whether a generated repo is in sync with
//! its config.
//!
//! Computes a SHA-256 hash of the current `config/site.yaml` in the target
//! repo and compares it to the `config-hash` stamped in
//! `[package.metadata.cli-gen]` inside `Cargo.toml`.  Exits with an error if
//! they differ, or if the metadata section is missing.

use std::path::Path;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// Run the `validate` subcommand.
///
/// Returns `Ok(())` when the repo is in sync, or an error describing what is
/// stale.
pub fn run(repo_path: &Path) -> Result<()> {
    let config_yaml_path = repo_path.join("config").join("site.yaml");
    let cargo_toml_path = repo_path.join("Cargo.toml");

    let config_bytes = std::fs::read(&config_yaml_path).context("reading config/site.yaml")?;
    let current_hash = hex::encode(Sha256::digest(&config_bytes));

    let stored_hash = read_stored_hash(&cargo_toml_path)?;

    if current_hash == stored_hash {
        tracing::info!(hash = %current_hash, "✓  Repo is in sync (hash {}).", current_hash);
        Ok(())
    } else {
        bail!(
            "Generated files are STALE.\n\
             Stored hash : {stored_hash}\n\
             Current hash: {current_hash}\n\n\
             Run `cli-generator update {path}` to regenerate.",
            path = repo_path.display()
        )
    }
}

/// Extract the value of `config-hash` from `[package.metadata.cli-gen]` in
/// the given `Cargo.toml`.
fn read_stored_hash(cargo_toml_path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(cargo_toml_path).context("reading Cargo.toml")?;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("config-hash") {
            // Matches: `config-hash = "abc123"` or `config-hash       = "abc123"`
            if let Some(value_part) = rest.trim().strip_prefix('=') {
                let hash = value_part.trim().trim_matches('"').to_string();
                if !hash.is_empty() {
                    return Ok(hash);
                }
            }
        }
    }
    bail!(
        "Could not find `config-hash` in [package.metadata.cli-gen] inside `{}`.\n\
         Was this repo created or updated by cli-generator?",
        cargo_toml_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn read_stored_hash_finds_compact_key() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            r#"[package.metadata.cli-gen]
config-hash = "deadbeef""#
        )
        .unwrap();
        assert_eq!(read_stored_hash(f.path()).unwrap(), "deadbeef");
    }

    #[test]
    fn read_stored_hash_finds_padded_key() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            r#"[package.metadata.cli-gen]
config-hash       = "cafebabe""#
        )
        .unwrap();
        assert_eq!(read_stored_hash(f.path()).unwrap(), "cafebabe");
    }

    #[test]
    fn read_stored_hash_errors_when_missing() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[package]\nname = \"my-crate\"").unwrap();
        assert!(read_stored_hash(f.path()).is_err());
    }
}
