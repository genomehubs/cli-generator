//! Integration tests for the generated `goat-cli` project.
//!
//! These tests run `cli-generator new goat` into a temporary directory and
//! inspect the generated source files to catch regressions in:
//!
//! - Field-to-flag mapping (`cli_flags.rs`)
//! - URL construction (`client.rs`, `main.rs`)
//! - Cargo dependency injection (`Cargo.toml`)
//!
//! Field and flag assertions are derived directly from `sites/goat-cli-options.yaml`
//! so they stay in sync automatically when the config changes.
//!
//! They do **not** compile the generated project (that would be very slow).
//! The CI `generated-cli-tests` job compiles and runs the binary instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use cli_generator::core::config::CliOptionsConfig;

fn cli_generator_bin() -> String {
    env!("CARGO_BIN_EXE_cli-generator").to_string()
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn goat_cli_options() -> CliOptionsConfig {
    let path = workspace_root().join("sites/goat-cli-options.yaml");
    CliOptionsConfig::from_file(&path).expect("failed to load sites/goat-cli-options.yaml")
}

/// Run `cli-generator new goat` into `output_dir` and return the path to the
/// generated `goat-cli/` subdirectory.
fn generate_goat_cli(output_dir: &Path) -> PathBuf {
    let config_dir = workspace_root().join("sites");
    let status = Command::new(cli_generator_bin())
        .args([
            "new",
            "goat",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--config",
            config_dir.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn cli-generator");
    assert!(status.success(), "cli-generator new goat failed");
    output_dir.join("goat-cli")
}

// ── cli_flags.rs: all explicit fields from config must appear ─────────────────

#[test]
fn all_taxon_fields_from_config_present_in_cli_flags() {
    let opts = goat_cli_options();
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let flags = fs::read_to_string(project.join("src/generated/cli_flags.rs")).unwrap();

    let taxon_groups = opts.indexes.get("taxon").expect("no taxon index in config");
    for group in &taxon_groups.field_groups {
        for field in &group.fields {
            assert!(
                flags.contains(field.as_str()),
                "flag '--{}': expected field '{}' in generated cli_flags.rs",
                group.flag,
                field,
            );
        }
    }
}

#[test]
fn all_assembly_fields_from_config_present_in_cli_flags() {
    let opts = goat_cli_options();
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let flags = fs::read_to_string(project.join("src/generated/cli_flags.rs")).unwrap();

    let assembly_groups = opts
        .indexes
        .get("assembly")
        .expect("no assembly index in config");
    for group in &assembly_groups.field_groups {
        for field in &group.fields {
            assert!(
                flags.contains(field.as_str()),
                "flag '--{}': expected field '{}' in generated cli_flags.rs",
                group.flag,
                field,
            );
        }
    }
}

// ── client.rs ────────────────────────────────────────────────────────────────

#[test]
fn include_estimates_param_in_client() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let client = fs::read_to_string(project.join("src/generated/client.rs")).unwrap();
    assert!(
        client.contains("includeEstimates"),
        "includeEstimates must appear in generated client.rs"
    );
}

#[test]
fn search_url_function_exported_in_client() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let client = fs::read_to_string(project.join("src/generated/client.rs")).unwrap();
    assert!(
        client.contains("pub fn search_url("),
        "search_url must be a public function in generated client.rs"
    );
}

// ── main.rs ──────────────────────────────────────────────────────────────────

#[test]
fn taxon_filter_enum_in_main() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    for item in &[
        "TaxonFilter",
        "tax_name",
        "tax_tree",
        "tax_lineage",
        "url: bool",
    ] {
        assert!(
            main_rs.contains(item),
            "expected '{item}' in generated main.rs"
        );
    }
}

#[test]
fn taxon_filter_defaults_to_name_in_main() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(
        main_rs.contains("default_value = \"name\""),
        "taxon_filter default must be \"name\" in generated main.rs"
    );
}

// ── Cargo.toml ───────────────────────────────────────────────────────────────

#[test]
fn phf_dependency_injected_in_cargo_toml() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        cargo_toml.contains("phf"),
        "phf dependency must be present in generated Cargo.toml"
    );
}

#[test]
fn cli_generator_git_dep_injected_in_cargo_toml() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        cargo_toml.contains("cli-generator"),
        "cli-generator git dep must be present in generated Cargo.toml"
    );
}
