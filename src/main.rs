//! CLI entry point for `cli-generator`.
//!
//! Parses command-line arguments with clap and delegates all work to the
//! `commands` modules.  No business logic lives here — this file is
//! intentionally thin.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use cli_generator::cli_meta;
use cli_generator::commands;

#[derive(Parser)]
#[command(name = cli_meta::NAME, about = cli_meta::ABOUT, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new site CLI repository from a live API schema.
    New {
        /// Site name (must match a .yaml file in the config directory).
        #[arg(value_name = "SITE")]
        site: String,

        /// Directory in which to create the new repo (defaults to current dir).
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,

        /// Directory containing site config .yaml files (overrides built-in sites/).
        #[arg(long)]
        config: Option<PathBuf>,

        /// Skip the field cache and fetch directly from the API.
        #[arg(long)]
        force_fresh: bool,

        /// Path to a local rust-py-template checkout (overrides GitHub URL).
        #[arg(long)]
        template: Option<PathBuf>,
    },

    /// Update generated files in an existing site CLI repo.
    Update {
        /// Path to the root of the site CLI repo to update.
        #[arg(default_value = ".")]
        repo: PathBuf,

        /// Directory containing site config .yaml files (overrides the repo's config/).
        #[arg(long)]
        config: Option<PathBuf>,

        /// Skip the field cache and fetch directly from the API.
        #[arg(long)]
        force_fresh: bool,
    },

    /// Preview what `new` or `update` would generate (dry run).
    Preview {
        /// Site name for a new-site preview.
        #[arg(long, group = "preview_mode")]
        site: Option<String>,

        /// Repo path for an update preview.
        #[arg(long, group = "preview_mode")]
        repo: Option<PathBuf>,

        /// Directory containing site config .yaml files (overrides built-in sites/).
        #[arg(long)]
        config: Option<PathBuf>,

        /// Skip the field cache and fetch directly from the API.
        #[arg(long)]
        force_fresh: bool,
    },

    /// Check whether a generated repo is in sync with its config.
    Validate {
        /// Path to the root of the site CLI repo to validate.
        #[arg(default_value = ".")]
        repo: PathBuf,
    },
}

fn main() {
    if let Err(e) = run(Cli::parse().command) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Dispatch a parsed command to the appropriate handler.
///
/// Returning `Result` keeps `main()` as a one-liner and makes the dispatch
/// logic testable.
fn run(command: Commands) -> anyhow::Result<()> {
    let default_sites_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/sites"));

    match command {
        Commands::New {
            site,
            output_dir,
            config,
            force_fresh,
            template,
        } => {
            let sites = config.unwrap_or(default_sites_dir);
            commands::new::run(&site, &sites, &output_dir, force_fresh, template.as_deref())
        }

        Commands::Update {
            repo,
            config,
            force_fresh,
        } => commands::update::run(&repo, config.as_deref(), force_fresh),

        Commands::Preview {
            site,
            repo,
            config,
            force_fresh,
        } => {
            let sites = config.unwrap_or(default_sites_dir);
            match (site, repo) {
                (Some(name), None) => commands::preview::run_new(&name, &sites, force_fresh),
                (None, Some(path)) => commands::preview::run_update(&path, force_fresh),
                _ => {
                    anyhow::bail!("supply exactly one of --site or --repo")
                }
            }
        }

        Commands::Validate { repo } => commands::validate::run(&repo),
    }
}
