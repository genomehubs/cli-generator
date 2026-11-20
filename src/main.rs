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
use cli_generator::core::report_document::resolve_report_request;
use cli_generator::core::runtime_query::{
    apply_overrides, parse_taxon_filter, phylopic_request, resolve_result_index, v3_query_input,
    v3_query_input_from_v2, LookupRequest, MetadataRequest, PhylopicFilter, PlotOptions,
    QueryOverrides, QueryRequest, RecordRequest,
};
use genomehubs_query::query::SplitQueryDocument;

#[derive(Parser)]
#[command(name = cli_meta::NAME, about = cli_meta::ABOUT, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    runtime: RuntimeArgs,
}

/// Query input options shared by `search`, `count`, `describe`, and `validate`.
#[derive(Args)]
struct QueryArgs {
    /// Result index to use (defaults to the preset's index, then `taxon`).
    #[arg(long)]
    result: Option<String>,

    /// Legacy v2 query expression, translated to v3, e.g. `tax_tree(Mammalia)`.
    #[arg(long)]
    query: Option<String>,

    /// v3 query YAML, or a combined query and params YAML (flat or nested under `query:`/`params:`). File path or inline YAML.
    #[arg(long = "query-yaml", value_name = "YAML", conflicts_with_all = ["query", "result"])]
    query_yaml: Option<String>,

    /// v3 params YAML, e.g. `size: 10`. File path or inline YAML.
    #[arg(long = "params-yaml", value_name = "YAML", conflicts_with_all = ["query"])]
    params_yaml: Option<String>,

    /// Return fields; repeat the flag or separate with commas. Replaces `fields` in the query YAML.
    #[arg(long = "field", value_name = "FIELD", value_delimiter = ',')]
    fields: Vec<String>,

    /// Hit count: overrides `size` from the query or params YAML.
    #[arg(long, value_name = "N")]
    size: Option<usize>,

    /// Override `page` in the params YAML.
    #[arg(long, value_name = "N")]
    page: Option<usize>,

    /// Override `sort_by` in the params YAML.
    #[arg(long = "sort-by", value_name = "FIELD")]
    sort_by: Option<String>,

    /// Override `sort_order` in the params YAML: `asc` or `desc`.
    #[arg(long = "sort-order", value_name = "ORDER")]
    sort_order: Option<String>,

    /// Override `tidy` in the params YAML. A bare flag means true.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, default_missing_value = "true")]
    tidy: Option<bool>,

    /// Override `include_estimates` in the params YAML. A bare flag means true.
    #[arg(long = "include-estimates", value_name = "BOOL", num_args = 0..=1, default_missing_value = "true")]
    include_estimates: Option<bool>,

    /// Taxon name to filter on; repeat the flag or separate with commas. Replaces `taxa` in the query YAML.
    #[arg(long = "taxon", value_name = "NAME", value_delimiter = ',')]
    taxa: Vec<String>,

    /// Taxonomic rank to filter on, e.g. `species`. Replaces `rank` in the query YAML.
    #[arg(long = "taxon-rank", value_name = "RANK")]
    taxon_rank: Option<String>,

    /// Taxon filter type: `name`, `tree`, or `lineage`. Replaces `taxon_filter_type` in the query YAML.
    #[arg(long = "taxon-filter", value_name = "TYPE", value_parser = ["name", "tree", "lineage"])]
    taxon_filter: Option<String>,

    /// Assembly accession to filter on; repeat the flag or separate with commas. Replaces `assemblies` in the query YAML.
    #[arg(long = "assembly", value_name = "ACCESSION", value_delimiter = ',')]
    assemblies: Vec<String>,

    /// Sample accession to filter on; repeat the flag or separate with commas. Replaces `samples` in the query YAML.
    #[arg(long = "sample", value_name = "ACCESSION", value_delimiter = ',')]
    samples: Vec<String>,

    /// Rank columns to return, e.g. `phylum`; repeat the flag or separate with commas. Replaces `ranks` in the query YAML.
    #[arg(long = "rank", value_name = "RANK", value_delimiter = ',')]
    ranks: Vec<String>,

    /// Name classes to return, e.g. `scientific_name`; repeat the flag or separate with commas. Replaces `names` in the query YAML.
    #[arg(long = "name", value_name = "CLASS", value_delimiter = ',')]
    names: Vec<String>,

    /// Attribute filter, e.g. `genome_size<3G`; repeat the flag for several. Replaces `attributes` in the query YAML.
    #[arg(long = "filter", value_name = "EXPRESSION")]
    filters: Vec<String>,

    /// Taxonomy backbone, e.g. `ncbi`. Overrides `taxonomy` in the params YAML.
    #[arg(long, value_name = "NAME")]
    taxonomy: Option<String>,
}

impl QueryArgs {
    /// Build the v3 query documents from the parsed flags, then apply flag overrides.
    fn into_input(self, runtime: &RuntimeConfig) -> anyhow::Result<SplitQueryDocument> {
        let taxon_filter = self
            .taxon_filter
            .as_deref()
            .map(parse_taxon_filter)
            .transpose()?;
        let overrides = QueryOverrides {
            size: self.size,
            page: self.page,
            sort_by: self.sort_by,
            sort_order: self.sort_order,
            tidy: self.tidy,
            include_estimates: self.include_estimates,
            taxonomy: self.taxonomy,
            taxa: self.taxa,
            taxon_rank: self.taxon_rank,
            taxon_filter,
            assemblies: self.assemblies,
            samples: self.samples,
            fields: self.fields.clone(),
            ranks: self.ranks,
            names: self.names,
            filters: self.filters,
        };
        let documents = match self.query {
            Some(expression) => v3_query_input_from_v2(&QueryRequest {
                result: resolve_result_index(self.result.as_deref(), runtime),
                query: Some(expression),
                fields: self.fields,
                size: None,
            })?,
            None => {
                let default_index = resolve_result_index(self.result.as_deref(), runtime);
                v3_query_input(
                    self.query_yaml.as_deref(),
                    self.params_yaml.as_deref(),
                    &default_index,
                )?
            }
        };
        apply_overrides(documents, &overrides)
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

/// Subcommands of `metadata`; with none, the aggregated hub metadata is printed.
#[derive(Subcommand)]
enum MetadataCommand {
    /// List the available indices.
    Indices,

    /// List the available taxonomies.
    Taxonomies,

    /// List the taxonomic ranks.
    Ranks,

    /// List field definitions for one result index.
    Fields {
        /// Result index to describe (defaults to the preset's index, then `taxon`).
        #[arg(long)]
        index: Option<String>,
    },
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
        #[command(flatten)]
        query: QueryArgs,
    },

    /// Count matching records on a GenomeHubs instance and print the JSON response.
    Count {
        #[command(flatten)]
        query: QueryArgs,
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

    /// Fetch records by ID and print the JSON response.
    Record {
        /// Record ID to fetch; repeat the flag or separate with commas, e.g. `9606,9605`.
        #[arg(
            long = "record-id",
            value_name = "ID",
            value_delimiter = ',',
            required = true
        )]
        record_ids: Vec<String>,

        /// Result index of the records (defaults to the preset's index, then `taxon`).
        #[arg(long)]
        result: Option<String>,
    },

    /// Resolve PhyloPic silhouettes for one or more taxa, or for a batch file of taxon IDs.
    Phylopic {
        /// NCBI taxon ID(s) to resolve, e.g. `9606` or `9606,10090`; several IDs use the batch route (omit when using `--batch`).
        #[arg(
            long = "taxon-id",
            value_name = "ID",
            value_delimiter = ',',
            required_unless_present = "batch",
            conflicts_with = "batch"
        )]
        taxon_id: Vec<String>,

        /// File of taxon IDs (a JSON array, or one numeric ID per line), resolved in one request (max 200).
        #[arg(long, value_name = "FILE", conflicts_with = "taxon_id")]
        batch: Option<PathBuf>,

        /// Taxonomy the taxon IDs belong to.
        #[arg(long, default_value = "ncbi")]
        taxonomy: String,

        /// Accept only images with these SPDX licence IDs; repeat or separate with commas.
        #[arg(long, value_name = "SPDX", value_delimiter = ',')]
        license: Vec<String>,

        /// Accept only public-domain images.
        #[arg(long)]
        public_domain: bool,
    },

    /// Print hub metadata; without a subcommand, print the aggregated metadata.
    Metadata {
        #[command(subcommand)]
        command: Option<MetadataCommand>,
    },

    /// Print API health and readiness as JSON.
    Status,

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

    /// Build a report from a GenomeHubs instance and print the JSON response.
    Report {
        #[command(flatten)]
        query: QueryArgs,

        /// Report config YAML, e.g. `report: histogram`. File path or inline YAML.
        /// May be omitted when `--query-yaml` is a combined document with a `report` key.
        #[arg(
            long = "report-yaml",
            value_name = "YAML",
            required_unless_present = "query_yaml"
        )]
        report_yaml: Option<String>,

        #[command(flatten)]
        plot: PlotArgs,
    },

    /// Build a positional report (oxford, ribbon, painting) and print the JSON response.
    Positional {
        #[command(flatten)]
        query: QueryArgs,

        /// Positional config YAML, e.g. `report: oxford`. File path or inline YAML.
        #[arg(long = "positional-yaml", value_name = "YAML")]
        positional_yaml: String,

        #[command(flatten)]
        plot: PlotArgs,
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

/// Plot options shared by `report` and `positional`.
#[derive(Args)]
struct PlotArgs {
    /// Include the PlotSpec in the response.
    #[arg(long)]
    include_plot_spec: bool,

    /// Display options as a JSON string applied to the PlotSpec, e.g. `'{"title":"My plot"}'`.
    #[arg(long, value_name = "JSON")]
    display: Option<String>,
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

        Commands::Search { query } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime)?;
            commands::search::run(&context.runtime, &context.registry, &input)
        }

        Commands::Count { query } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime)?;
            commands::count::run(&context.runtime, &context.registry, &input)
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

        Commands::Record { record_ids, result } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let request = RecordRequest {
                record_ids,
                result: resolve_result_index(result.as_deref(), &context.runtime),
            };
            commands::record::run(&context.runtime, &context.registry, &request)
        }

        Commands::Phylopic {
            taxon_id,
            batch,
            taxonomy,
            license,
            public_domain,
        } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let filter = PhylopicFilter {
                licenses: license,
                public_domain,
            };
            let request = phylopic_request(taxon_id, batch.as_deref(), taxonomy, filter)?;
            commands::phylopic::run(&context.runtime, &context.registry, &request)
        }

        Commands::Metadata { command } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let request = match command {
                None => MetadataRequest::Aggregate,
                Some(MetadataCommand::Indices) => MetadataRequest::Indices,
                Some(MetadataCommand::Taxonomies) => MetadataRequest::Taxonomies,
                Some(MetadataCommand::Ranks) => MetadataRequest::Ranks,
                Some(MetadataCommand::Fields { index }) => MetadataRequest::Fields {
                    index: resolve_result_index(index.as_deref(), &context.runtime),
                },
            };
            commands::metadata::run(&context.runtime, &context.registry, &request)
        }

        Commands::Status => {
            let context = QueryContext::from_args(&cli.runtime)?;
            commands::status::run(&context.runtime, &context.registry)
        }

        Commands::Describe { query, verbose } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime)?;
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
            let input = query.into_input(&context.runtime)?;
            commands::validate::run(
                &context.runtime,
                &context.registry,
                &input,
                &context.metadata,
            )
        }

        Commands::Check { repo } => commands::check::run(&repo),

        Commands::Report {
            mut query,
            report_yaml,
            plot,
        } => {
            let resolved =
                resolve_report_request(query.query_yaml.as_deref(), report_yaml.as_deref())?;
            query.query_yaml = resolved.query_yaml;
            if query.taxon_filter.is_none() && !query.taxa.is_empty() {
                // A name match only hits the taxon itself, so reports default to descendants.
                query.taxon_filter = Some("tree".to_string());
            }
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime)?;
            let mut options = PlotOptions::from_flags(
                plot.include_plot_spec || resolved.include_plot_spec,
                plot.display.as_deref(),
            )?;
            options.display = options.display.or(resolved.display);
            commands::report::run(
                &context.runtime,
                &context.registry,
                &input,
                &resolved.report_yaml,
                &options,
            )
        }

        Commands::Positional {
            query,
            positional_yaml,
            plot,
        } => {
            let context = QueryContext::from_args(&cli.runtime)?;
            let input = query.into_input(&context.runtime)?;
            let options = PlotOptions::from_flags(plot.include_plot_spec, plot.display.as_deref())?;
            commands::positional::run(
                &context.runtime,
                &context.registry,
                &input,
                &positional_yaml,
                &options,
            )
        }

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
