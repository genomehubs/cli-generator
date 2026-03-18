//! `cli-generator new` — scaffold a new site CLI repository.
//!
//! 1. Checks that `cargo-generate` is installed.
//! 2. Loads site config and CLI options config.
//! 3. Fetches API field definitions (or uses a fresh cache).
//! 4. Runs `cargo generate` to scaffold the repo from `rust-py-template`.
//! 5. Renders generated files into `src/generated/` and `src/cli_meta.rs`.
//! 6. Copies config files into the new repo's `config/` directory.
//! 7. Stamps `[package.metadata.cli-gen]` in the new repo's `Cargo.toml`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::core::{
    codegen::CodeGenerator,
    config::{CliOptionsConfig, SiteConfig},
    fetch::FieldFetcher,
};

/// Run the `new` subcommand.
///
/// `site_name` must match a `.yaml` file in `sites_dir`.
/// `output_dir` is the parent directory — the generated repo is created as
/// `{output_dir}/{site_name}-cli`.
pub fn run(
    site_name: &str,
    sites_dir: &Path,
    output_dir: &Path,
    force_fresh: bool,
    template_path: Option<&Path>,
) -> Result<()> {
    ensure_cargo_generate_installed()?;

    let site = load_site_config(site_name, sites_dir)?;
    let options = load_cli_options(site_name, sites_dir)?;

    let cache_dir = FieldFetcher::default_cache_dir(site_name)
        .context("could not determine OS cache directory")?;
    let fetcher = FieldFetcher::new(cache_dir, force_fresh).with_archive_mode(site.archive);
    let fields_by_index = fetcher
        .fetch_all(&site)
        .with_context(|| format!("fetching field definitions for site '{site_name}'"))?;

    let repo_name = format!("{site_name}-cli");
    let template = resolve_template(template_path);

    scaffold_repo(&template, &repo_name, output_dir)?;

    let repo_dir = output_dir.join(&repo_name);
    let gen = CodeGenerator::new()?;
    let rendered = gen.render_all(&site, &options, &fields_by_index)?;

    write_generated_files(&repo_dir, &rendered)?;
    copy_config_files(site_name, sites_dir, &repo_dir)?;
    stamp_cargo_toml(&repo_dir, &site)?;
    patch_pyproject_toml(&repo_dir)?;

    println!("✓  Generated '{repo_name}' in {}", repo_dir.display());
    Ok(())
}

/// Return the `~/.cargo/bin` directory derived from `$CARGO_HOME` or `$HOME`.
fn cargo_home_bin() -> PathBuf {
    let base = std::env::var("CARGO_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/.cargo")
    });
    PathBuf::from(base).join("bin")
}

/// Abort with a clear message if `cargo-generate` is not on PATH.
fn ensure_cargo_generate_installed() -> Result<()> {
    if which::which("cargo-generate").is_ok() {
        return Ok(());
    }
    if cargo_home_bin().join("cargo-generate").exists() {
        return Ok(());
    }
    bail!(
        "cargo-generate is required but was not found on PATH.\n\
         Install it with:\n\n    cargo install cargo-generate\n"
    )
}

/// Determine the template path: caller override → sibling `rust-py-template` → GitHub URL.
fn resolve_template(override_path: Option<&Path>) -> String {
    if let Some(p) = override_path {
        return format!("--path {}", p.display());
    }
    // Try a sibling `rust-py-template` directory first (typical dev setup).
    // Use canonicalize so the path stays valid after current_dir() changes.
    let sibling = PathBuf::from("../rust-py-template");
    if let Ok(abs) = sibling.canonicalize() {
        if abs.is_dir() {
            return format!("--path {}", abs.display());
        }
    }
    "--git https://github.com/genomehubs/rust-py-template".to_string()
}

/// Shell out to `cargo-generate` to scaffold the base repo.
fn scaffold_repo(template_flag: &str, repo_name: &str, output_dir: &Path) -> Result<()> {
    // Find cargo-generate: try PATH first, then $CARGO_HOME/bin directly.
    // The direct fallback handles CI environments where ~/.cargo/bin is
    // installed but not yet reflected in the process's PATH.
    let cargo_generate =
        which::which("cargo-generate").unwrap_or_else(|_| cargo_home_bin().join("cargo-generate"));

    // Build a PATH that includes $CARGO_HOME/bin so cargo-generate can in turn
    // find any cargo tooling it needs, regardless of the inherited PATH.
    let cargo_bin = cargo_home_bin();
    let current_path = std::env::var("PATH").unwrap_or_default();
    let augmented_path = if current_path.contains(cargo_bin.to_str().unwrap_or("")) {
        current_path
    } else {
        format!("{}:{current_path}", cargo_bin.display())
    };

    let status = std::process::Command::new(&cargo_generate)
        .arg("generate")
        .args(template_flag.split_whitespace())
        .arg("--name")
        .arg(repo_name)
        .arg("--define")
        .arg(format!("project-description=CLI for {repo_name}"))
        .arg("--define")
        .arg("author-name=genomehubs")
        .arg("--define")
        .arg("author-email=genomehubs@genomehubs.org")
        .arg("--define")
        .arg("python-min-version=3.9")
        .env("PATH", augmented_path)
        .current_dir(output_dir)
        .status()
        .with_context(|| format!("running cargo-generate ({})", cargo_generate.display()))?;

    if !status.success() {
        bail!("cargo-generate exited with status: {status}");
    }
    Ok(())
}

/// Write each rendered file to the appropriate path within `repo_dir`.
///
/// Parent directories are created as needed.
pub(crate) fn write_generated_files(
    repo_dir: &Path,
    rendered: &std::collections::HashMap<String, String>,
) -> Result<()> {
    for (rel_path, content) in rendered {
        let dest = repo_dir.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        std::fs::write(&dest, content).with_context(|| format!("writing {}", dest.display()))?;
    }
    Ok(())
}

/// Copy `sites/{site}.yaml` and `sites/{site}-cli-options.yaml` into
/// `{repo_dir}/config/`.
fn copy_config_files(site_name: &str, sites_dir: &Path, repo_dir: &Path) -> Result<()> {
    let config_dir = repo_dir.join("config");
    std::fs::create_dir_all(&config_dir)?;

    for (src_name, dest_name) in [
        (format!("{site_name}.yaml"), "site.yaml".to_string()),
        (
            format!("{site_name}-cli-options.yaml"),
            "cli-options.yaml".to_string(),
        ),
    ] {
        let src = sites_dir.join(&src_name);
        if src.exists() {
            std::fs::copy(&src, config_dir.join(&dest_name))
                .with_context(|| format!("copying {} to config/", src_name))?;
        }
    }
    Ok(())
}

/// Write `[package.metadata.cli-gen]` fields into the generated repo's `Cargo.toml`
/// and inject the additional dependencies that the generated code requires.
/// Patch the generated repo's `pyproject.toml` to add runtime and dev deps
/// needed by the generated Python code (`pyyaml` for `QueryBuilder` serialisation).
///
/// Idempotent — skips if the dep is already present.  Silent no-op if
/// `pyproject.toml` does not exist in the template output.
fn patch_pyproject_toml(repo_dir: &Path) -> Result<()> {
    let path = repo_dir.join("pyproject.toml");
    if !path.exists() {
        return Ok(());
    }
    let mut text = std::fs::read_to_string(&path).context("reading generated pyproject.toml")?;
    if !text.contains("pyyaml") {
        text = text.replacen(
            "maturin>=1.0\",",
            "maturin>=1.0\",\n    \"pyyaml>=6.0\",",
            1,
        );
    }
    std::fs::write(&path, text).context("writing patched pyproject.toml")?;
    Ok(())
}

fn stamp_cargo_toml(repo_dir: &Path, site: &SiteConfig) -> Result<()> {
    use sha2::{Digest, Sha256};

    let cargo_toml_path = repo_dir.join("Cargo.toml");
    let mut text =
        std::fs::read_to_string(&cargo_toml_path).context("reading generated Cargo.toml")?;

    let config_bytes = serde_yaml::to_string(site).unwrap_or_default();
    let hash = hex::encode(Sha256::digest(config_bytes.as_bytes()));
    let version = crate::cli_meta::VERSION;
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();

    text = text.replace(
        "generator-version = \"\"",
        &format!("generator-version = \"{version}\""),
    );
    text = text.replace(
        "config-hash       = \"\"",
        &format!("config-hash       = \"{hash}\""),
    );

    // Append a generated-at field if not already present.
    if !text.contains("generated-at") {
        text = text.replace(
            &format!("config-hash       = \"{hash}\""),
            &format!("config-hash       = \"{hash}\"\ngenerated-at      = \"{now}\""),
        );
    }

    // Inject deps required by the generated code that are not in the base template.
    text = inject_generated_deps(text);

    std::fs::write(&cargo_toml_path, text).context("writing stamped Cargo.toml")?;
    Ok(())
}

/// Ensure the generated repo's `Cargo.toml` has the deps needed by
/// `src/generated/client.rs` and `src/main.rs`.
///
/// We add deps idempotently — if the key is already present (e.g. on a
/// second `update` run) we leave it alone.
fn inject_generated_deps(mut text: String) -> String {
    // Make pyo3 optional so `cargo run` works without libpython.
    text = text.replace(
        "pyo3   = { version = \"0.22\", features = [\"abi3-py39\"] }",
        "pyo3   = { version = \"0.22\", features = [\"abi3-py39\"], optional = true }",
    );
    // Fix feature to use dep: syntax.
    text = text.replace(
        "extension-module = [\"pyo3/extension-module\"]",
        "extension-module = [\"dep:pyo3\", \"pyo3/extension-module\"]",
    );

    // Append missing deps after the serde line.
    let required_deps = [
        ("serde_json", "serde_json = \"1\""),
        (
            "reqwest",
            "reqwest    = { version = \"0.12\", features = [\"json\", \"blocking\"] }",
        ),
        ("anyhow", "anyhow     = \"1\""),
        (
            "phf",
            "phf        = { version = \"0.11\", features = [\"macros\"] }",
        ),
        (
            "cli-generator",
            "cli-generator = { git = \"https://github.com/genomehubs/cli-generator\" }",
        ),
    ];
    for (key, dep_line) in required_deps {
        if !text.contains(key) {
            // Insert immediately before [dev-dependencies] (always present in the template).
            text = text.replacen(
                "\n[dev-dependencies]",
                &format!("\n{dep_line}\n\n[dev-dependencies]"),
                1,
            );
        }
    }

    text
}

/// Load site config from `{sites_dir}/{site_name}.yaml`.
pub(crate) fn load_site_config(site_name: &str, sites_dir: &Path) -> Result<SiteConfig> {
    let path = sites_dir.join(format!("{site_name}.yaml"));
    SiteConfig::from_file(&path).with_context(|| format!("loading site config for '{site_name}'"))
}

/// Load CLI options config from `{sites_dir}/{site_name}-cli-options.yaml`.
///
/// Returns an empty config (no flag groups) when the file does not exist,
/// so that generation can proceed even without a cli-options file.
pub(crate) fn load_cli_options(site_name: &str, sites_dir: &Path) -> Result<CliOptionsConfig> {
    let path = sites_dir.join(format!("{site_name}-cli-options.yaml"));
    if path.exists() {
        CliOptionsConfig::from_file(&path)
            .with_context(|| format!("loading CLI options for '{site_name}'"))
    } else {
        Ok(CliOptionsConfig {
            indexes: std::collections::HashMap::new(),
        })
    }
}
