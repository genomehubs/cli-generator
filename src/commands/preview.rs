//! `cli-generator preview` — dry-run that prints what would be generated.
//!
//! Renders all templates as `new` or `update` would but writes them to a
//! temporary directory and then diffs each file against the on-disk state
//! (for an existing repo) or prints the complete rendered source (for a new
//! site).

use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::new::{load_cli_options, load_site_config};
use crate::core::{codegen::CodeGenerator, config::CliOptionsConfig, fetch::FieldFetcher};

/// Render and preview output for a **new** site, printing all generated files
/// to stdout.
pub fn run_new(site_name: &str, sites_dir: &Path, force_fresh: bool) -> Result<()> {
    let site = load_site_config(site_name, sites_dir)?;
    let options = load_cli_options(site_name, sites_dir)?;

    let fields_by_index = fetch_fields(&site.name, force_fresh, &site)?;
    let gen = CodeGenerator::new()?;
    let rendered_by_lang = gen.render_all(&site, &options, &fields_by_index)?;

    // Flatten and print all language outputs
    let mut all_rendered = std::collections::HashMap::new();
    for (language, rendered) in rendered_by_lang {
        for (path, content) in rendered {
            all_rendered.insert(format!("[{language}] {path}"), content);
        }
    }
    print_rendered(&all_rendered);
    Ok(())
}

/// Render and preview updates for an **existing** repo, diffing against
/// current on-disk content.
pub fn run_update(repo_path: &Path, force_fresh: bool) -> Result<()> {
    let config_dir = repo_path.join("config");

    let site = crate::core::config::SiteConfig::from_file(&config_dir.join("site.yaml"))
        .context("loading config/site.yaml")?;
    let options = load_options_from_config_dir(&config_dir)?;

    let fields_by_index = fetch_fields(&site.name, force_fresh, &site)?;
    let gen = CodeGenerator::new()?;
    let rendered_by_lang = gen.render_all(&site, &options, &fields_by_index)?;

    // Flatten all language outputs and diff against disk
    let mut all_rendered = std::collections::HashMap::new();
    for (_language, rendered) in rendered_by_lang {
        for (path, content) in rendered {
            all_rendered.insert(path, content);
        }
    }
    diff_against_disk(repo_path, &all_rendered);
    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn fetch_fields(
    site_name: &str,
    force_fresh: bool,
    site: &crate::core::config::SiteConfig,
) -> Result<std::collections::HashMap<String, Vec<crate::core::fetch::FieldDef>>> {
    let cache_dir = FieldFetcher::default_cache_dir(site_name)
        .context("could not determine OS cache directory")?;
    FieldFetcher::new(cache_dir, force_fresh)
        .fetch_all(site)
        .with_context(|| format!("fetching fields for site '{site_name}'"))
}

fn load_options_from_config_dir(config_dir: &Path) -> Result<CliOptionsConfig> {
    let path = config_dir.join("cli-options.yaml");
    if path.exists() {
        CliOptionsConfig::from_file(&path).context("loading config/cli-options.yaml")
    } else {
        Ok(CliOptionsConfig {
            indexes: std::collections::HashMap::new(),
        })
    }
}

/// Print every rendered file path and content.
fn print_rendered(rendered: &std::collections::HashMap<String, String>) {
    let mut keys: Vec<&String> = rendered.keys().collect();
    keys.sort();
    for path in keys {
        println!("=== {path} ===");
        println!("{}", rendered[path]);
    }
}

/// For each rendered file, compare against the on-disk version and print a
/// simple diff summary.
fn diff_against_disk(repo_root: &Path, rendered: &std::collections::HashMap<String, String>) {
    let mut keys: Vec<&String> = rendered.keys().collect();
    keys.sort();
    for rel_path in keys {
        let disk_path = repo_root.join(rel_path);
        let new_content = rendered[rel_path].as_str();
        match std::fs::read_to_string(&disk_path) {
            Ok(existing) if existing == new_content => {
                println!("  unchanged  {rel_path}");
            }
            Ok(_) => {
                println!("  CHANGED    {rel_path}");
                // Print new content so callers can diff externally if needed.
                println!("--- new content ---");
                println!("{new_content}");
                println!("---");
            }
            Err(_) => {
                println!("  NEW        {rel_path}");
                println!("{new_content}");
            }
        }
    }
}
