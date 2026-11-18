//! `cli-generator validate` — check a query against the index metadata.
//!
//! Accepts the same inputs as `describe` (v2 flags or a v3 `--query-yaml`
//! body) and runs the portable validator that also backs the SDK `validate()`
//! methods.

use anyhow::{bail, Result};

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::metadata::MetadataOptions;
use crate::core::runtime_query::{validate_input, QueryInput};

/// Validate a query and print the error list as JSON to stdout.
///
/// Prints `[]` and returns `Ok(())` when the query is valid. Returns an error
/// (non-zero exit status) when the query has any validation errors.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    input: &QueryInput,
    metadata: &MetadataOptions,
) -> Result<()> {
    let errors = validate_input(runtime, registry, input, metadata)?;
    println!("{}", serde_json::to_string_pretty(&errors)?);
    if errors.is_empty() {
        return Ok(());
    }
    bail!("query is invalid: {} error(s)", errors.len())
}
