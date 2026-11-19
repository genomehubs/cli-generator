//! `metadata` command: print hub metadata from the `/v3/metadata` routes.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::{run_metadata, MetadataRequest};

/// Fetch hub metadata and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &MetadataRequest,
) -> Result<()> {
    let response = run_metadata(runtime, registry, request)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
