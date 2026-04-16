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
fn tera_dep_injected_in_cargo_toml() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        cargo_toml.contains("tera"),
        "tera dependency must be present in generated Cargo.toml"
    );
}

// ── cli-generator preview command ─────────────────────────────────────────────

/// Test that `cli-generator preview --site` produces valid output.
#[test]
fn preview_new_site_produces_output() {
    let config_dir = workspace_root().join("sites");

    let output = Command::new(cli_generator_bin())
        .args([
            "preview",
            "--site",
            "goat",
            "--config",
            config_dir.to_str().unwrap(),
        ])
        .output()
        .expect("failed to spawn cli-generator preview");

    assert!(
        output.status.success(),
        "cli-generator preview --site goat failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain rendered file paths
    assert!(
        stdout.contains("src/generated/") || stdout.contains("==="),
        "preview output should contain rendered file paths"
    );
    // Should contain actual generated code
    assert!(
        stdout.contains("pub ") || stdout.contains("fn "),
        "preview output should contain generated Rust code"
    );
}

/// Test that `cli-generator preview --repo` on an existing repo shows diffs.
#[test]
fn preview_update_repo_diffs_changes() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());

    // Modify an existing file to trigger a diff
    let cli_flags_path = project.join("src/generated/cli_flags.rs");
    let original = fs::read_to_string(&cli_flags_path).expect("failed to read cli_flags.rs");
    let modified = original.replace("pub ", "// pub ");
    fs::write(&cli_flags_path, &modified).expect("failed to modify cli_flags.rs");

    // Run preview update
    let output = Command::new(cli_generator_bin())
        .args(["preview", "--repo", project.to_str().unwrap()])
        .output()
        .expect("failed to spawn cli-generator preview");

    assert!(
        output.status.success(),
        "cli-generator preview --repo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should report file status
    assert!(
        stdout.contains("CHANGED") || stdout.contains("unchanged"),
        "preview output should report file status"
    );
}

// ── cli-generator update command ───────────────────────────────────────────────

/// Test that `cli-generator update` successfully updates an existing repo.
#[test]
fn update_command_modifies_existing_repo() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());

    // Modify a generated file to simulate stale state
    let cli_flags_path = project.join("src/generated/cli_flags.rs");
    fs::write(&cli_flags_path, "// stale file\n").expect("failed to write stale file");

    let stale_content = fs::read_to_string(&cli_flags_path).expect("failed to read modified file");
    assert_eq!(
        stale_content, "// stale file\n",
        "setup: file should be stale"
    );

    // Run update
    let output = Command::new(cli_generator_bin())
        .args(["update", project.to_str().unwrap()])
        .output()
        .expect("failed to spawn cli-generator update");

    assert!(
        output.status.success(),
        "cli-generator update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the file was regenerated
    let updated_content = fs::read_to_string(&cli_flags_path).expect("failed to read updated file");
    assert_ne!(
        updated_content, stale_content,
        "update command should have regenerated the file"
    );
    assert!(
        updated_content.contains("pub "),
        "regenerated file should contain valid Rust code"
    );
}

/// Test that `cli-generator update` preserves non-generated files.
#[test]
fn update_command_preserves_hand_written_files() {
    let tmp = TempDir::new().unwrap();
    let project = generate_goat_cli(tmp.path());

    // Mark a custom hand-written file
    let custom_file = project.join("src/custom_module.rs");
    let custom_content = "// My custom code\npub fn my_function() {}";
    fs::write(&custom_file, custom_content).expect("failed to write custom file");

    // Run update
    let output = Command::new(cli_generator_bin())
        .args(["update", project.to_str().unwrap()])
        .output()
        .expect("failed to spawn cli-generator update");

    assert!(
        output.status.success(),
        "cli-generator update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify custom file is untouched
    let preserved = fs::read_to_string(&custom_file).expect("failed to read custom file");
    assert_eq!(
        preserved, custom_content,
        "update command should not modify hand-written files"
    );
}

// ── cli-generator validate command ────────────────────────────────────────────

/// Test that `cli-generator validate` unit logic works with matching hashes.
///
/// We test the core `run` function directly via Rust rather than via CLI
/// since the CLI invocation requires a properly-set repo which is complex to
/// construct in a test.
#[test]
fn validate_succeeds_with_matching_config_hash() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config").join("site.yaml");
    let cargo_toml_path = tmp.path().join("Cargo.toml");

    // Create a minimal site.yaml
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(&config_path, "name: test\napi_path: /api/test\n").unwrap();

    // Compute the hash of the config file
    use sha2::{Digest, Sha256};
    let config_bytes = std::fs::read(&config_path).unwrap();
    let hash = hex::encode(Sha256::digest(&config_bytes));

    // Create a Cargo.toml with the matching hash
    let cargo_toml_content = format!(
        "[package]\nname = \"test-cli\"\n\n[package.metadata.cli-gen]\nconfig-hash = \"{}\"\n",
        hash
    );
    std::fs::write(&cargo_toml_path, cargo_toml_content).unwrap();

    // This should succeed
    let result = cli_generator::commands::validate::run(tmp.path());
    assert!(result.is_ok(), "validate should succeed with matching hash");
}

/// Test that `cli-generator validate` detects mismatched hashes.
#[test]
fn validate_fails_with_mismatched_config_hash() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config").join("site.yaml");
    let cargo_toml_path = tmp.path().join("Cargo.toml");

    // Create a minimal site.yaml
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(&config_path, "name: test\napi_path: /api/test\n").unwrap();

    // Create a Cargo.toml with a different hash (doesn't match the actual file)
    let cargo_toml_content = "[package]\nname = \"test-cli\"\n\n[package.metadata.cli-gen]\nconfig-hash = \"wronghash123\"\n";
    std::fs::write(&cargo_toml_path, cargo_toml_content).unwrap();

    // This should fail
    let result = cli_generator::commands::validate::run(tmp.path());
    assert!(result.is_err(), "validate should fail with mismatched hash");
}

/// Test that `cli-generator validate` errors when config-hash is missing.
#[test]
fn validate_fails_when_hash_missing() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config").join("site.yaml");
    let cargo_toml_path = tmp.path().join("Cargo.toml");

    // Create a minimal site.yaml
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(&config_path, "name: test\napi_path: /api/test\n").unwrap();

    // Create a Cargo.toml without metadata section
    let cargo_toml_content = "[package]\nname = \"test-cli\"\n";
    std::fs::write(&cargo_toml_path, cargo_toml_content).unwrap();

    // This should fail
    let result = cli_generator::commands::validate::run(tmp.path());
    assert!(
        result.is_err(),
        "validate should fail when config-hash is missing"
    );
}
