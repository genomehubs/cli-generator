//! `search` command: query a GenomeHubs API and print the JSON response.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::metadata::MetadataOptions;
use crate::core::runtime_query::{run_query, QueryRequest};

/// Run a search request and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &QueryRequest,
    metadata: &MetadataOptions,
) -> Result<()> {
    let response = run_query(runtime, registry, "search", request, metadata)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
