//! `lookup` command: resolve a search term on a GenomeHubs API and print the JSON response.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::{run_lookup, LookupRequest};

/// Run a lookup request and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &LookupRequest,
) -> Result<()> {
    let response = run_lookup(runtime, registry, request)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
