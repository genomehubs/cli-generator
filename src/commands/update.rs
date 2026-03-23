//! `cli-generator update` — re-render generated files in an existing site CLI repo.
//!
//! Reads `config/site.yaml` from the target repo, optionally refreshes the
//! field cache, then overwrites only the paths under `src/generated/` and
//! `src/cli_meta.rs`.  Hand-written files (`src/core/`, `src/main.rs`, etc.)
//! are never touched.

use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::new::write_generated_files;
use crate::core::{codegen::CodeGenerator, config::CliOptionsConfig, fetch::FieldFetcher};

/// Run the `update` subcommand.
///
/// `repo_path` must point to the root of a previously-generated site CLI repo.
/// When `config_dir` is supplied it takes precedence over the repo's own
/// `config/` directory, allowing a different set of site configs to be used.
pub fn run(repo_path: &Path, config_dir: Option<&Path>, force_fresh: bool) -> Result<()> {
    let resolved_config = config_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo_path.join("config"));

    let site = crate::core::config::SiteConfig::from_file(&resolved_config.join("site.yaml"))
        .context("loading site.yaml from config directory")?;

    let options = load_cli_options_from_repo(&resolved_config)?;

    let cache_dir = FieldFetcher::default_cache_dir(&site.name)
        .context("could not determine OS cache directory")?;
    let fetcher = FieldFetcher::new(cache_dir, force_fresh).with_archive_mode(site.archive);
    let fields_by_index = fetcher
        .fetch_all(&site)
        .with_context(|| format!("fetching field definitions for site '{}'", site.name))?;

    let gen = CodeGenerator::new()?;
    let rendered_by_lang = gen.render_all(&site, &options, &fields_by_index)?;

    // Flatten all language outputs and write to repo
    let mut all_files = std::collections::HashMap::new();
    for (_language, rendered) in rendered_by_lang {
        all_files.extend(rendered);
    }
    write_generated_files(repo_path, &all_files).context("writing generated files")?;

    println!("✓  Updated generated files in {}", repo_path.display());
    Ok(())
}

/// Load `config/cli-options.yaml` from an existing repo, returning an empty
/// config when the file does not exist.
fn load_cli_options_from_repo(config_dir: &Path) -> Result<CliOptionsConfig> {
    let path = config_dir.join("cli-options.yaml");
    if path.exists() {
        CliOptionsConfig::from_file(&path).context("loading config/cli-options.yaml")
    } else {
        Ok(CliOptionsConfig {
            indexes: std::collections::HashMap::new(),
        })
    }
}
