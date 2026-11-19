//! `status` command: print API health and readiness from the `/v3/status` route.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::runtime_query::run_status;

/// Fetch the API status and print the pretty-printed JSON response to stdout.
pub fn run(runtime: &RuntimeConfig, registry: &SitePresetRegistry) -> Result<()> {
    let response = run_status(runtime, registry)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
