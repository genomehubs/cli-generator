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

    // Rename the Python package directory from the cargo-generate default
    // (`{site}_cli`) to the configured SDK name (`{sdk_name}`).
    let sdk_name = site.resolved_sdk_name();
    rename_python_package(&repo_dir, &site.name, &sdk_name)?;

    let gen = CodeGenerator::new()?;
    let rendered_by_lang = gen.render_all(&site, &options, &fields_by_index)?;

    for (language, rendered) in rendered_by_lang {
        // All generated files go to repo root (templates contain proper subdirectory paths)
        write_generated_files(&repo_dir, &rendered)?;
        postprocess_language(&repo_dir, &language)?;
    }

    // Copy embedded cli_generator modules into the generated repo to avoid external dependency
    copy_embedded_modules(&repo_dir)?;

    patch_python_init(&repo_dir, &sdk_name, &site.display_name)?;
    copy_config_files(site_name, sites_dir, &repo_dir)?;
    stamp_cargo_toml(&repo_dir, &site)?;
    patch_pyproject_toml(&repo_dir)?;
    create_r_package(&repo_dir, &site)?;
    create_js_package(&repo_dir, &site)?;
    create_quarto_docs(&repo_dir, &site)?;
    ensure_license_file(&repo_dir)?;

    println!("✓  Generated '{repo_name}' in {}", repo_dir.display());
    Ok(())
}

/// Run language-specific post-processing on generated files.
///
/// When files are generated, apply language-appropriate formatting:
/// - Python: `black` and `isort` for consistent style
/// - R: styler (Phase 2)
/// - Rust: `rustfmt` and `clippy --fix` (Phase 2)
///
/// These tools are idempotent and non-fatal if not installed.
fn postprocess_language(dir: &Path, language: &str) -> Result<()> {
    match language {
        "python" => {
            // Format with black (idempotent, non-fatal if missing)
            let _ = std::process::Command::new("black")
                .arg("--line-length")
                .arg("120")
                .arg(dir)
                .status()
                .map_err(|e| eprintln!("warn: black not installed: {e}"));

            // Sort imports with isort (idempotent, non-fatal if missing)
            let _ = std::process::Command::new("isort")
                .arg("--profile")
                .arg("black")
                .arg("--line-length")
                .arg("120")
                .arg(dir)
                .status()
                .map_err(|e| eprintln!("warn: isort not installed: {e}"));
        }
        "rust" => {
            // Phase 2: Add rustfmt + clippy --fix
        }
        "r" => {
            // Phase 2: Add styler
        }
        _ => {
            // Unknown language, skip postprocessing
        }
    }
    Ok(())
}

/// Abort with a clear message if `cargo-generate` is not installed.
///
/// Runs `cargo generate --version` via the cargo subcommand mechanism,
/// which finds the binary relative to cargo itself — no PATH tricks needed.
fn ensure_cargo_generate_installed() -> Result<()> {
    let ok = std::process::Command::new("cargo")
        .args(["generate", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        bail!(
            "cargo-generate is required but was not found.\n\
             Install it with:\n\n    cargo install cargo-generate\n"
        );
    }
    Ok(())
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

/// Shell out to `cargo generate` to scaffold the base repo.
///
/// Uses `cargo` as the entry point so cargo locates the `cargo-generate`
/// subcommand relative to its own binary directory.  This works regardless
/// of whether `~/.cargo/bin` is on PATH in the calling environment.
fn scaffold_repo(template_flag: &str, repo_name: &str, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("creating output directory '{}'", output_dir.display()))?;

    let status = std::process::Command::new("cargo")
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
        .current_dir(output_dir)
        .status()
        .context("running cargo generate")?;

    if !status.success() {
        bail!("cargo generate exited with status: {status}");
    }
    Ok(())
}

/// Rename the Python package directory from the cargo-generate default to
/// the configured SDK name, and remove the stale `.pyi` stub (which will be
/// replaced by the generator's `site_cli.pyi` template output).
///
/// cargo-generate names the package `{project_name | snake_case}`, so for a
/// project called `goat-cli` it creates `python/goat_cli/`.  We rename that
/// to `python/{sdk_name}/` before writing generated files into it.
fn rename_python_package(repo_dir: &Path, site_name: &str, sdk_name: &str) -> Result<()> {
    let cargo_gen_pkg = format!("{}_cli", site_name.replace('-', "_"));
    if cargo_gen_pkg == sdk_name {
        return Ok(());
    }
    let src = repo_dir.join("python").join(&cargo_gen_pkg);
    let dst = repo_dir.join("python").join(sdk_name);
    if src.exists() {
        std::fs::rename(&src, &dst)
            .with_context(|| format!("renaming python/{cargo_gen_pkg} → python/{sdk_name}"))?;
    }
    // Remove the old .pyi stub; the generator writes a correctly-named one.
    let old_pyi = dst.join(format!("{cargo_gen_pkg}.pyi"));
    if old_pyi.exists() {
        std::fs::remove_file(&old_pyi)
            .with_context(|| format!("removing stale {cargo_gen_pkg}.pyi"))?;
    }
    Ok(())
}

/// Overwrite the cargo-generate `__init__.py` with SDK-appropriate exports.
///
/// The template `__init__.py` re-exports a placeholder `gc_content` function.
/// We replace it with the real SDK surface: `build_url`, `search`, `count`,
/// and the `QueryBuilder` class from `query.py`.
fn patch_python_init(repo_dir: &Path, sdk_name: &str, display_name: &str) -> Result<()> {
    let path = repo_dir.join("python").join(sdk_name).join("__init__.py");
    if !path.exists() {
        return Ok(());
    }
    let content = format!(
        r#""""{}  Python SDK.

Generated by cli-generator. Do not edit.
"""
from .{} import build_url, count, describe_query, render_snippet, search
from .query import QueryBuilder

__all__ = ["build_url", "count", "describe_query", "QueryBuilder", "render_snippet", "search"]
"#,
        display_name, sdk_name
    );
    std::fs::write(&path, content).context("writing generated __init__.py")?;
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

/// Copy embedded cli_generator modules into `{repo_dir}/src/embedded/` so the
/// generated code doesn't depend on an external cli-generator crate.
///
/// This avoids branch/version dependency issues and keeps generated repos self-contained.
fn copy_embedded_modules(repo_dir: &Path) -> Result<()> {
    let embedded_dir = repo_dir.join("src/embedded");
    std::fs::create_dir_all(&embedded_dir)?;

    let cli_gen_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    // Pure query types moved to the genomehubs-query subcrate.
    let subcrate_query_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/genomehubs-query/src/query");

    // Modules from the main src/ tree — need crate::core:: → crate::embedded::core:: rewriting.
    let main_modules = [
        "core/config.rs",
        "core/describe.rs",
        "core/fetch.rs",
        "core/snippet.rs",
        "core/query/validation.rs",
    ];

    // Modules from crates/genomehubs-query — use internal paths only, no rewriting needed.
    let subcrate_modules = [
        ("mod.rs", "core/query/mod.rs"),
        ("attributes.rs", "core/query/attributes.rs"),
        ("identifiers.rs", "core/query/identifiers.rs"),
        ("url.rs", "core/query/url.rs"),
    ];

    for module_path in &main_modules {
        let src_file = cli_gen_src.join(module_path);
        let dest_path = embedded_dir.join(module_path);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        if src_file.exists() {
            let content = std::fs::read_to_string(&src_file)
                .with_context(|| format!("reading {module_path}"))?
                .replace("crate::core::", "crate::embedded::core::");
            std::fs::write(&dest_path, content)
                .with_context(|| format!("writing {module_path}"))?;
        }
    }

    for (src_name, dest_rel) in &subcrate_modules {
        let src_file = subcrate_query_src.join(src_name);
        let dest_path = embedded_dir.join(dest_rel);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        if src_file.exists() {
            std::fs::copy(&src_file, &dest_path)
                .with_context(|| format!("copying subcrate {src_name}"))?;
        }
    }

    // Copy parse.rs from the subcrate root (not the query subdirectory).
    let subcrate_src =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/genomehubs-query/src");
    let parse_src = subcrate_src.join("parse.rs");
    let parse_dest = embedded_dir.join("core/parse.rs");
    if parse_src.exists() {
        std::fs::copy(&parse_src, &parse_dest).context("copying subcrate parse.rs")?;
    }

    // validation.rs is cli-generator-specific and not in the subcrate's mod.rs.
    let query_mod_path = embedded_dir.join("core/query/mod.rs");
    let mut query_mod =
        std::fs::read_to_string(&query_mod_path).context("reading embedded query/mod.rs")?;
    if !query_mod.contains("pub mod validation") {
        query_mod.push_str("\npub mod validation;\n");
        std::fs::write(&query_mod_path, query_mod)
            .context("appending validation to query/mod.rs")?;
    }

    // Create a root mod.rs that re-exports the core submodule
    let mod_rs_content = r#"//! Embedded cli_generator modules for code generation.
//!
//! These modules are embedded from the cli-generator tool to avoid external
//! dependency issues. They are used by the CLI for query description and URL building.

pub mod core;
"#;

    std::fs::write(embedded_dir.join("mod.rs"), mod_rs_content)
        .context("writing embedded/mod.rs")?;

    // Copy snippet Tera templates so `include_str!` in `snippet.rs` resolves correctly.
    // In the generated project snippet.rs lives at src/embedded/core/snippet.rs, so
    // `../../templates/snippets/` resolves to src/templates/snippets/.
    let cli_gen_templates =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/snippets");
    let dest_templates_dir = repo_dir.join("src/templates/snippets");
    std::fs::create_dir_all(&dest_templates_dir).context("creating src/templates/snippets")?;

    if cli_gen_templates.is_dir() {
        for entry in std::fs::read_dir(&cli_gen_templates).context("reading templates/snippets")? {
            let entry = entry.context("reading templates/snippets entry")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tera") {
                let file_name = path.file_name().expect("template file must have a name");
                std::fs::copy(&path, dest_templates_dir.join(file_name))
                    .with_context(|| format!("copying snippet template {}", path.display()))?;
            }
        }
    }

    // Create a core/mod.rs that declares all the submodules
    let core_mod_rs_content = r#"//! Core cli_generator modules.
//!
//! Re-exports query validation and URL building logic needed by generated code.

pub mod config;
pub mod describe;
pub mod fetch;
pub mod parse;
pub mod query;
pub mod snippet;
"#;

    std::fs::write(embedded_dir.join("core/mod.rs"), core_mod_rs_content)
        .context("writing embedded/core/mod.rs")?;

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

/// Create R package structure in the generated repository using the rextendr layout.
///
/// Generates:
/// - `r/{r_package_name}/DESCRIPTION` — package metadata
/// - `r/{r_package_name}/NAMESPACE` — R exports
/// - `r/{r_package_name}/R/_package.R` — package initialisation
/// - `r/{r_package_name}/R/query.R` — QueryBuilder class
/// - `r/{r_package_name}/R/extendr-wrappers.R` — pre-generated extendr wrappers
/// - `r/{r_package_name}/src/rust/src/lib.rs` — extendr FFI wrapper
/// - `r/{r_package_name}/src/rust/Cargo.toml` — Rust build config
/// - `r/{r_package_name}/src/entrypoint.c` — rextendr C entry point
/// - `r/{r_package_name}/src/{r_package_name}-win.def` — Windows exports
/// - `r/{r_package_name}/src/Makevars.in` / `Makevars.win.in` — build variables
/// - `r/{r_package_name}/configure` / `configure.win` — configure scripts (executable)
/// - `r/{r_package_name}/tools/msrv.R` / `tools/config.R` — build helpers
/// - `r/{r_package_name}/src/rust/src/embedded/` — embedded core modules
/// - `r/{r_package_name}/src/rust/src/generated/` — minimal generated stubs
fn create_r_package(repo_dir: &Path, site: &SiteConfig) -> Result<()> {
    let r_package_name = site.name.replace('-', "_").to_lowercase();
    let r_pkg_dir = repo_dir.join("r").join(&r_package_name);
    let rust_src_dir = r_pkg_dir.join("src/rust/src");

    // Create all required directories up front.
    for sub in &[
        "R",
        "src/rust/src/embedded/core/query",
        "src/rust/src/generated",
        "src/rust/src/templates/snippets",
        "tools",
    ] {
        std::fs::create_dir_all(r_pkg_dir.join(sub))
            .with_context(|| format!("creating r/{}/{}", r_package_name, sub))?;
    }

    let templates_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/r");

    // Build Tera template context.
    let mut context = tera::Context::new();
    context.insert("r_package_name", &r_package_name);
    context.insert("site_display_name", &site.display_name);
    context.insert("site_name", &site.name);
    context.insert("api_base", &site.api_base);

    // Render a .tera (or plain text with Tera vars) template and write to dest.
    let render_template = |template_rel: &str, dest: &Path| -> Result<()> {
        let tpl_path = templates_dir.join(template_rel);
        if !tpl_path.exists() {
            return Ok(());
        }
        let tpl = std::fs::read_to_string(&tpl_path)
            .with_context(|| format!("reading template {template_rel}"))?;
        let rendered = tera::Tera::one_off(&tpl, &context, true)
            .with_context(|| format!("rendering template {template_rel}"))?;
        std::fs::write(dest, rendered).with_context(|| format!("writing {}", dest.display()))?;
        Ok(())
    };

    // Render all Tera-templated files.
    render_template("DESCRIPTION.tera", &r_pkg_dir.join("DESCRIPTION"))?;
    render_template("NAMESPACE.tera", &r_pkg_dir.join("NAMESPACE"))?;
    render_template("_package.R.tera", &r_pkg_dir.join("R/_package.R"))?;
    render_template("query.R", &r_pkg_dir.join("R/query.R"))?;
    render_template(
        "extendr-wrappers.R.tera",
        &r_pkg_dir.join("R/extendr-wrappers.R"),
    )?;
    render_template("lib.rs.tera", &rust_src_dir.join("lib.rs"))?;
    render_template("Cargo.toml.tera", &r_pkg_dir.join("src/rust/Cargo.toml"))?;
    render_template("entrypoint.c.tera", &r_pkg_dir.join("src/entrypoint.c"))?;
    render_template(
        "win.def.tera",
        &r_pkg_dir.join(format!("src/{r_package_name}-win.def")),
    )?;

    // Copy Makevars files with literal {PKG_NAME} → r_package_name substitution.
    // These go through R's configure system, not Tera rendering.
    for makevars in &["Makevars.in", "Makevars.win.in"] {
        let src = templates_dir.join(makevars);
        if src.exists() {
            let content = std::fs::read_to_string(&src)
                .with_context(|| format!("reading {makevars}"))?
                .replace("{PKG_NAME}", &r_package_name);
            std::fs::write(r_pkg_dir.join("src").join(makevars), content)
                .with_context(|| format!("writing src/{makevars}"))?;
        }
    }

    // Copy configure scripts and mark them executable.
    for script in &["configure", "configure.win"] {
        let src = templates_dir.join(script);
        if src.exists() {
            let dest = r_pkg_dir.join(script);
            std::fs::copy(&src, &dest).with_context(|| format!("copying {script}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                    .with_context(|| format!("setting {script} executable"))?;
            }
        }
    }

    // Copy R build helper scripts.
    for tool in &["msrv.R", "config.R"] {
        let src = templates_dir.join("tools").join(tool);
        if src.exists() {
            std::fs::copy(&src, r_pkg_dir.join("tools").join(tool))
                .with_context(|| format!("copying tools/{tool}"))?;
        }
    }

    // Write site-specific cli_meta.rs.
    let api_base_versioned = format!("{}/{}", site.api_base, site.api_version);
    let cli_meta_content = format!(
        r#"//! CLI metadata constants.
//!
//! Generated by cli-generator. Do not edit — changes will be overwritten
//! on the next `cli-generator update` run.

/// Application name as displayed in `--help` output.
pub const NAME: &str = "{name}";

/// Short description shown in `--help`.
pub const ABOUT: &str = "CLI for {display_name}";

/// API base URL for {display_name} (includes version path component).
pub const API_BASE: &str = "{api_base_versioned}";

/// API base URL without the version path component.
pub const API_BASE_URL: &str = "{api_base}";

/// API version path component, e.g. `"v2"`.
pub const API_VERSION: &str = "{api_version}";
"#,
        name = site.name,
        display_name = site.display_name,
        api_base_versioned = api_base_versioned,
        api_base = site.api_base,
        api_version = site.api_version,
    );
    std::fs::write(rust_src_dir.join("cli_meta.rs"), cli_meta_content)
        .context("writing src/rust/src/cli_meta.rs")?;

    // Write minimal generated module stubs.
    // The real generated field metadata lives in the main Rust crate, not the R crate.
    std::fs::write(
        rust_src_dir.join("generated/mod.rs"),
        "pub mod cli_flags;\n",
    )
    .context("writing src/rust/src/generated/mod.rs")?;
    std::fs::write(
        rust_src_dir.join("generated/cli_flags.rs"),
        "// Minimal stub for R crate — generated field metadata not needed.\n",
    )
    .context("writing src/rust/src/generated/cli_flags.rs")?;

    // Copy embedded core modules into the R crate's src/rust/src/.
    copy_r_embedded_modules(&rust_src_dir).context("copying embedded modules into R crate")?;

    Ok(())
}

/// Copy the cli_generator core modules into the R crate's `src/rust/src/embedded/`.
///
/// Mirrors `copy_embedded_modules` but targets the rextendr layout:
/// the Rust source root is `src/rust/src/` instead of `src/`.
fn copy_r_embedded_modules(rust_src_dir: &Path) -> Result<()> {
    let embedded_dir = rust_src_dir.join("embedded");
    std::fs::create_dir_all(&embedded_dir)?;

    let cli_gen_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let subcrate_query_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/genomehubs-query/src/query");

    let main_modules = [
        "core/config.rs",
        "core/describe.rs",
        "core/fetch.rs",
        "core/snippet.rs",
        "core/query/validation.rs",
    ];

    let subcrate_modules = [
        ("mod.rs", "core/query/mod.rs"),
        ("attributes.rs", "core/query/attributes.rs"),
        ("identifiers.rs", "core/query/identifiers.rs"),
        ("url.rs", "core/query/url.rs"),
    ];

    for module_path in &main_modules {
        let src_file = cli_gen_src.join(module_path);
        let dest_path = embedded_dir.join(module_path);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }

        if src_file.exists() {
            let content = std::fs::read_to_string(&src_file)
                .with_context(|| format!("reading {module_path}"))?
                .replace("crate::core::", "crate::embedded::core::");
            std::fs::write(&dest_path, content)
                .with_context(|| format!("writing {module_path}"))?;
        }
    }

    for (src_name, dest_rel) in &subcrate_modules {
        let src_file = subcrate_query_src.join(src_name);
        let dest_path = embedded_dir.join(dest_rel);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }

        if src_file.exists() {
            std::fs::copy(&src_file, &dest_path)
                .with_context(|| format!("copying subcrate {src_name}"))?;
        }
    }

    let query_mod_path = embedded_dir.join("core/query/mod.rs");
    let mut query_mod =
        std::fs::read_to_string(&query_mod_path).context("reading embedded query/mod.rs")?;
    if !query_mod.contains("pub mod validation") {
        query_mod.push_str("\npub mod validation;\n");
        std::fs::write(&query_mod_path, query_mod)
            .context("appending validation to query/mod.rs")?;
    }

    std::fs::write(
        embedded_dir.join("mod.rs"),
        "//! Embedded cli_generator modules.\n//!\n//! Copied from the cli-generator tool to avoid external dependency issues.\n\npub mod core;\n",
    )
    .context("writing embedded/mod.rs")?;

    std::fs::write(
        embedded_dir.join("core/mod.rs"),
        "//! Core cli_generator modules.\n\npub mod config;\npub mod describe;\npub mod fetch;\npub mod query;\npub mod snippet;\n",
    )
    .context("writing embedded/core/mod.rs")?;

    // Copy snippet Tera templates.
    // snippet.rs lives at src/rust/src/embedded/core/snippet.rs, so
    // `../../templates/snippets/` resolves to src/rust/src/templates/snippets/.
    let cli_gen_snippets =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/snippets");
    let dest_snippets_dir = rust_src_dir.join("templates/snippets");
    std::fs::create_dir_all(&dest_snippets_dir).context("creating templates/snippets")?;

    if cli_gen_snippets.is_dir() {
        for entry in std::fs::read_dir(&cli_gen_snippets).context("reading snippets dir")? {
            let entry = entry.context("reading snippets entry")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tera") {
                let file_name = path.file_name().expect("template file must have a name");
                std::fs::copy(&path, dest_snippets_dir.join(file_name))
                    .with_context(|| format!("copying {}", path.display()))?;
            }
        }
    }

    Ok(())
}

/// Write a MIT `LICENSE` file if the generated repo does not already have one.
///
/// `cargo-generate` processes `LICENSE.liquid` from the template, but the
/// `{% now %}` tag fails silently in some cargo-generate versions, leaving the
/// file absent.  This function guarantees the file exists so that `maturin`
/// (which reads `license = "MIT"` from `Cargo.toml` and expects to find the
/// file) can build the Python wheel without error.
fn ensure_license_file(repo_dir: &Path) -> Result<()> {
    let license_path = repo_dir.join("LICENSE");
    if license_path.exists() {
        return Ok(());
    }
    let year = chrono::Utc::now().format("%Y").to_string();
    let license_text = format!(
        "MIT License\n\n\
         Copyright (c) {year} genomehubs\n\n\
         Permission is hereby granted, free of charge, to any person obtaining a copy\n\
         of this software and associated documentation files (the \"Software\"), to deal\n\
         in the Software without restriction, including without limitation the rights\n\
         to use, copy, modify, merge, publish, distribute, sublicense, and/or sell\n\
         copies of the Software, and to permit persons to whom the Software is\n\
         furnished to do so, subject to the following conditions:\n\n\
         The above copyright notice and this permission notice shall be included in all\n\
         copies or substantial portions of the Software.\n\n\
         THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\n\
         IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\n\
         FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\n\
         AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\n\
         LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\n\
         OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\n\
         SOFTWARE.\n"
    );
    std::fs::write(&license_path, license_text).context("writing LICENSE file")?;
    Ok(())
}

/// Scaffold a JavaScript SDK package in `js/{js_package_name}/`.
fn create_js_package(repo_dir: &Path, site: &SiteConfig) -> Result<()> {
    use tera::Context;

    let js_package_name = site.name.replace('-', "_").to_lowercase();
    let js_dir = repo_dir.join("js").join(&js_package_name);

    std::fs::create_dir_all(&js_dir)
        .with_context(|| format!("creating js/{}", &js_package_name))?;

    let template_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/js");

    // Path to the pre-built WASM package in the cli-generator repo
    let wasm_pkg_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/genomehubs-query/pkg");

    let mut context = Context::new();
    context.insert("js_package_name", &js_package_name);
    context.insert("site_name", &site.name);
    context.insert("site_display_name", &site.display_name);
    context.insert("api_base_url", &site.api_base);

    // 1. Render package.json
    if let Ok(tmpl) = std::fs::read_to_string(template_dir.join("package.json.tera")) {
        match tera::Tera::one_off(&tmpl, &context, false) {
            Ok(content) => {
                std::fs::write(js_dir.join("package.json"), content)
                    .with_context(|| format!("writing js/{}/package.json", &js_package_name))?;
            }
            Err(e) => eprintln!("warn: failed to render package.json template: {e}"),
        }
    }

    // 2. Render query.js (substitutes api_base_url and site name)
    if let Ok(tmpl) = std::fs::read_to_string(template_dir.join("query.js")) {
        match tera::Tera::one_off(&tmpl, &context, false) {
            Ok(content) => {
                std::fs::write(js_dir.join("query.js"), content)
                    .with_context(|| format!("writing js/{}/query.js", &js_package_name))?;
            }
            Err(e) => eprintln!("warn: failed to render query.js template: {e}"),
        }
    }

    // 3. Copy pre-built WASM package into js/{package}/pkg/
    if wasm_pkg_dir.is_dir() {
        let pkg_dest = js_dir.join("pkg");
        std::fs::create_dir_all(&pkg_dest)
            .with_context(|| format!("creating js/{}/pkg", &js_package_name))?;
        for entry in
            std::fs::read_dir(&wasm_pkg_dir).with_context(|| "reading WASM pkg directory")?
        {
            let entry = entry?;
            let fname = entry.file_name();
            // Copy everything except .gitignore (not needed in generated projects)
            if fname != ".gitignore" {
                std::fs::copy(entry.path(), pkg_dest.join(&fname))
                    .with_context(|| format!("copying WASM file {:?}", fname))?;
            }
        }
    } else {
        eprintln!(
            "warn: WASM package not found at {}. \
             Run: wasm-pack build --target nodejs --features wasm \
             in crates/genomehubs-query/",
            wasm_pkg_dir.display()
        );
    }

    // 4. Render build-wasm.sh script (documents how to rebuild)
    if let Ok(tmpl) = std::fs::read_to_string(template_dir.join("build-wasm.sh.tera")) {
        match tera::Tera::one_off(&tmpl, &context, false) {
            Ok(content) => {
                let script_path = js_dir.join("build-wasm.sh");
                std::fs::write(&script_path, content)
                    .with_context(|| format!("writing js/{}/build-wasm.sh", &js_package_name))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
                        .ok();
                }
            }
            Err(e) => eprintln!("warn: failed to render build-wasm.sh template: {e}"),
        }
    }

    Ok(())
}

/// Generate a Quarto documentation site skeleton in `{repo_dir}/docs/`.
///
/// Renders six `.qmd` pages covering installation, quick-start, and API
/// reference for all SDK languages, using site config as template context.
fn create_quarto_docs(repo_dir: &Path, site: &SiteConfig) -> Result<()> {
    use tera::Context;

    let docs_dir = repo_dir.join("docs");
    let ref_dir = docs_dir.join("reference");
    std::fs::create_dir_all(&ref_dir).with_context(|| "creating docs/reference")?;

    let template_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/docs");

    let r_package_name = site.name.replace('-', "_").to_lowercase();

    let mut context = Context::new();
    context.insert("site_name", &site.name);
    context.insert("site_display_name", &site.display_name);
    context.insert("api_base", &site.api_base);
    context.insert("api_version", &site.api_version);
    context.insert("sdk_name", &site.resolved_sdk_name());
    context.insert("r_package_name", &r_package_name);
    context.insert("indexes", &site.indexes);

    let render = |template_rel: &str, dest: &Path| -> Result<()> {
        let tpl_path = template_dir.join(template_rel);
        if !tpl_path.exists() {
            return Ok(());
        }
        let tpl = std::fs::read_to_string(&tpl_path)
            .with_context(|| format!("reading docs template {template_rel}"))?;
        let rendered = tera::Tera::one_off(&tpl, &context, false)
            .with_context(|| format!("rendering docs template {template_rel}"))?;
        std::fs::write(dest, rendered).with_context(|| format!("writing {}", dest.display()))?;
        Ok(())
    };

    render("_quarto.yml.tera", &docs_dir.join("_quarto.yml"))?;
    render("index.qmd.tera", &docs_dir.join("index.qmd"))?;
    render("quickstart.qmd.tera", &docs_dir.join("quickstart.qmd"))?;
    render(
        "reference/query-builder.qmd.tera",
        &ref_dir.join("query-builder.qmd"),
    )?;
    render("reference/parse.qmd.tera", &ref_dir.join("parse.qmd"))?;
    render("reference/cli.qmd.tera", &ref_dir.join("cli.qmd"))?;

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

    // Rename the lib crate to match the SDK package name so maturin builds a
    // `.so` with the correct importable name.
    let lib_name = format!("{}_cli", site.name.replace('-', "_"));
    let sdk_name = site.resolved_sdk_name();
    if lib_name != sdk_name {
        text = text.replacen(
            &format!("[lib]\nname = \"{lib_name}\""),
            &format!("[lib]\nname = \"{sdk_name}\""),
            1,
        );
    }

    // Add an empty [workspace] table so Cargo treats the generated project as its
    // own workspace root and does not walk up into the cli-generator workspace.
    if !text.contains("[workspace]") {
        text.push_str("\n# Standalone workspace root — prevents Cargo from walking up into parent workspaces.\n[workspace]\n");
    }

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
        ("serde_yaml", "serde_yaml = \"0.9\""),
        (
            "reqwest",
            "reqwest    = { version = \"0.12\", features = [\"json\", \"blocking\"] }",
        ),
        ("anyhow", "anyhow     = \"1\""),
        ("percent-encoding", "percent-encoding = \"2\""),
        (
            "chrono",
            "chrono = { version = \"0.4\", features = [\"serde\"] }",
        ),
        ("thiserror", "thiserror = \"1\""),
        ("dirs", "dirs = \"5\""),
        (
            "phf",
            "phf        = { version = \"0.11\", features = [\"macros\"] }",
        ),
        (
            "tera",
            "tera       = { version = \"1\", default-features = false }",
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
