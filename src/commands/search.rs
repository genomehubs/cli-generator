//! `search` command: query a GenomeHubs API and print the JSON response.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::run_v3_query;
use genomehubs_query::query::SplitQueryDocument;

/// Run a v3 search and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
) -> Result<()> {
    let response = run_v3_query(runtime, registry, "search", documents)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
