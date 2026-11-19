//! `report` command: build a report from a GenomeHubs API and print the JSON response.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::{run_v3_plot, PlotOptions};
use genomehubs_query::query::SplitQueryDocument;

/// Run a report and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
    report_yaml: &str,
    options: &PlotOptions,
) -> Result<()> {
    let response = run_v3_plot(runtime, registry, "report", documents, report_yaml, options)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
