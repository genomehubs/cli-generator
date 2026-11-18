//! CLI entry point for `cli-generator`.
//!
//! Parses command-line arguments with clap and delegates all work to the
//! `commands` modules.  No business logic lives here — this file is
//! intentionally thin.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use cli_generator::cli_meta;
use cli_generator::commands;
use cli_generator::core::config::{RuntimeConfig, SitePresetRegistry};
use cli_generator::core::metadata::MetadataOptions;
use cli_generator::core::runtime_query::{
    resolve_result_index, LookupRequest, QueryInput, QueryRequest,
};

#[derive(Parser)]
#[command(name = cli_meta::NAME, about = cli_meta::ABOUT, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    runtime: RuntimeArgs,
}

/// Query input options shared by `describe` and `validate`.
#[derive(Args)]
struct QueryArgs {
    /// Result index to use (defaults to the preset's index, then `taxon`).
    #[arg(long)]
    result: Option<String>,

    /// Legacy v2 query expression, e.g. `tax_tree(Mammalia)`.
    #[arg(long)]
    query: Option<String>,

    /// v3 `SearchQuery` YAML body, e.g. `index: taxon` with `taxa` and `attributes`.
    #[arg(long = "query-yaml", value_name = "YAML", conflicts_with_all = ["query", "result", "fields"])]
    query_yaml: Option<String>,

    /// Fields to use (v2 only); repeat the flag or separate with commas.
    #[arg(long = "field", value_name = "FIELD", value_delimiter = ',')]
    fields: Vec<String>,
}

impl QueryArgs {
    /// Convert the parsed flags into a query input for the runtime layer.
    fn into_input(self, runtime: &RuntimeConfig) -> QueryInput {
        match self.query_yaml {
            Some(yaml) => QueryInput::V3Yaml(yaml),
            None => QueryInput::V2(QueryRequest {
                result: resolve_result_index(self.result.as_deref(), runtime),
                query: self.query,
                fields: self.fields,
                size: None,
            }),
        }
    }
}

/// Runtime options that select and configure the target GenomeHubs instance.
#[derive(Args)]
struct RuntimeArgs {
    /// Named site preset, e.g. `goat` or `boat` (env: GENOMEHUBS_SITE).
    #[arg(long, global = true)]
    site: Option<String>,

    /// API base URL, e.g. `https://goat.genomehubs.org/api` (env: GENOMEHUBS_API_BASE).
    #[arg(long, global = true)]
    api_base: Option<String>,

    /// UI base URL (env: GENOMEHUBS_UI_BASE).
    #[arg(long, global = true)]
    ui_base: Option<String>,

    /// Ignore cached metadata and fetch the field list from the API.
    #[arg(long, global = true)]
    refresh_metadata: bool,

    /// Directory for cached metadata (defaults to the OS cache directory).
    #[arg(long, global = true, value_name = "DIR")]
    metadata_cache_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new site CLI repository from a live API schema.
    New {
        /// Config name (must match a .yaml file in the config directory).
        #[arg(value_name = "CONFIG")]
        config_name: String,

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

        /// Skip copying WASM packages into the generated JS SDK (useful for
        /// docs-only or CI builds that do not need the JavaScript SDK).
        #[arg(long)]
        no_wasm: bool,
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
        /// Config name for a new-site preview (scaffold).
        #[arg(long, value_name = "CONFIG", group = "preview_mode")]
        scaffold: Option<String>,

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

    /// Search a GenomeHubs instance and print the JSON response.
    Search {
        /// Result index to search (defaults to the preset's index, then `taxon`).
        #[arg(long)]
        result: Option<String>,

        /// Query expression, e.g. `tax_tree(Mammalia)`.
        #[arg(long)]
        query: Option<String>,

        /// Fields to return; repeat the flag or separate with commas.
        #[arg(long = "field", value_name = "FIELD", value_delimiter = ',')]
        fields: Vec<String>,

        /// Number of hits to return.
        #[arg(long)]
        size: Option<usize>,
    },

    /// Count matching records on a GenomeHubs instance and print the JSON response.
    Count {
        /// Result index to count (defaults to the preset's index, then `taxon`).
        #[arg(long)]
        result: Option<String>,

        /// Query expression, e.g. `tax_tree(Mammalia)`.
        #[arg(long)]
        query: Option<String>,

        /// Fields to validate against the index; repeat the flag or separate with commas.
        #[arg(long = "field", value_name = "FIELD", value_delimiter = ',')]
        fields: Vec<String>,
    },

    /// Resolve a search term to matching taxa and print the JSON response.
    Lookup {
        /// Free-text term to resolve, e.g. `Homo sapiens`.
        term: String,

        /// Result index to search (defaults to the preset's index, then `taxon`).
        #[arg(long)]
        result: Option<String>,

        /// Number of candidates to return.
        #[arg(long)]
        size: Option<usize>,
    },

    /// Describe a query in plain English, naming fields from the index metadata.
    Describe {
        #[command(flatten)]
        query: QueryArgs,

        /// Print the detailed description instead of the concise one.
        #[arg(long)]
        verbose: bool,
    },

    /// Validate a query against the index metadata and print any errors as JSON.
    Validate {
        #[command(flatten)]
        query: QueryArgs,
    },

    /// Check whether a generated repo is in sync with its config.
    Check {
        /// Path to the root of the site CLI repo to check.
        #[arg(default_value = ".")]
        repo: PathBuf,
    },

    /// Generate a plot spec from a local TSV/CSV data file (no API call).
    #[command(name = "local-report")]
    LocalReport {
        /// Input data file (.tsv, .tab, or .csv).  Reads stdin when omitted.
        #[arg(short, long, value_name = "FILE")]
        input: Option<PathBuf>,

        /// Report type: histogram, scatter, or bar.
        #[arg(short, long, default_value = "histogram")]
        report: String,

        /// Column to use for the x axis (defaults to first column).
        #[arg(long)]
        x: Option<String>,

        /// Column to use for the y axis (defaults to second column).
        #[arg(long)]
        y: Option<String>,

        /// Display options as a JSON string (e.g. `'{"title":"My plot"}'`).
        #[arg(short, long)]
        display: Option<String>,

        /// Override auto-detected field delimiter (`\t` or `,`).
        #[arg(long)]
        delimiter: Option<char>,

        /// Write the PlotSpec JSON to this file instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

fn main() {
    // Initialise structured logging from `RUST_LOG` / env filter.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(e) = run(Cli::parse()) {
        tracing::error!(error = ?e, "application error");
        std::process::exit(1);
    }
}

/// Runtime, preset, and metadata settings shared by the query commands.
struct QueryContext {
    runtime: RuntimeConfig,
    registry: SitePresetRegistry,
    metadata: MetadataOptions,
}

impl QueryContext {
    /// Resolve runtime settings from global flags, env vars, and built-in presets.
    fn from_args(args: &RuntimeArgs) -> anyhow::Result<Self> {
        let registry = SitePresetRegistry::builtin()?;
        let runtime = RuntimeConfig::resolve(
            args.site.as_deref(),
            args.api_base.as_deref(),
            args.ui_base.as_deref(),
            Some(&registry),
        );
        let metadata =
            MetadataOptions::new(args.metadata_cache_dir.as_deref(), args.refresh_metadata)?;
        Ok(Self {
            runtime,
            registry,
            metadata,
        })
    }
}

/// Dispatch a parsed command to the appropriate handler.
///
/// Returning `Result` keeps `main()` as a one-liner and makes the dispatch
/// logic testable.
fn run(cli: Cli) -> anyhow::Result<()> {
    let default_sites_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/sites"));

    match cli.command {
        Commands::New {
            config_name,
            output_dir,
            config,
            force_fresh,
            template,
            no_wasm,
        } => {
            let sites = config.unwrap_or(default_sites_dir);
            commands::new::run(
                &config_name,
                &sites,
                &output_dir,
                force_fresh,
                template.as_deref(),
                no_wasm,
            )
        }

        Commands::Update {
            repo,
            config,
            force_fresh,
        } => commands::update::run(&repo, config.as_deref(), force_fresh),

        Commands::Preview {
            scaffold,
            repo,
            config,
            force_fresh,
        } => {
            let sites = config.unwrap_or(default_sites_dir);
            match (scaffold, repo) {
                (Some(name), None) => commands::preview::run_new(&name, &sites, force_fresh),
                (None, Some(path)) => commands::preview::run_update(&path, force_fresh),
                _ => {
                    anyhow::bail!("supply exactly one of --scaffold or --repo")
                }
            }
        }

        Commands::Search {
            result,
            query,
            fields,
            size,
        } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let request = QueryRequest {
                result: resolve_result_index(result.as_deref(), &context.runtime),
                query,
                fields,
                size,
            };
            commands::search::run(
                &context.runtime,
                &context.registry,
                &request,
                &context.metadata,
            )
        }

        Commands::Count {
            result,
            query,
            fields,
        } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let request = QueryRequest {
                result: resolve_result_index(result.as_deref(), &context.runtime),
                query,
                fields,
                size: None,
            };
            commands::count::run(
                &context.runtime,
                &context.registry,
                &request,
                &context.metadata,
            )
        }

        Commands::Lookup { term, result, size } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let request = LookupRequest {
                search_term: term,
                result: resolve_result_index(result.as_deref(), &context.runtime),
                size,
            };
            commands::lookup::run(&context.runtime, &context.registry, &request)
        }

        Commands::Describe { query, verbose } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime);
            commands::describe::run(
                &context.runtime,
                &context.registry,
                &input,
                &context.metadata,
                verbose,
            )
        }

        Commands::Validate { query } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime);
            commands::validate::run(
                &context.runtime,
                &context.registry,
                &input,
                &context.metadata,
            )
        }

        Commands::Check { repo } => commands::check::run(&repo),

        Commands::LocalReport {
            input,
            report,
            x,
            y,
            display,
            delimiter,
            output,
        } => commands::local_report::run(
            input.as_deref(),
            &report,
            x.as_deref(),
            y.as_deref(),
            display.as_deref(),
            delimiter,
            output.as_deref(),
        ),
    }
}
