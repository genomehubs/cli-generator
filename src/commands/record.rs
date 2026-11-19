//! `record` command: fetch records by ID from a GenomeHubs API and print the JSON response.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::{run_record, RecordRequest};

/// Fetch records by ID and print the pretty-printed JSON response to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &RecordRequest,
) -> Result<()> {
    let response = run_record(runtime, registry, request)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
