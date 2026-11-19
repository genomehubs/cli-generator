//! `phylopic` command: resolve PhyloPic silhouettes for a taxon or a batch of taxa.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::{run_phylopic, PhylopicRequest};

/// Resolve PhyloPic images and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &PhylopicRequest,
) -> Result<()> {
    let response = run_phylopic(runtime, registry, request)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
