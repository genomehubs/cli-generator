//! `cli-generator validate` — check a query against the index metadata.
//!
//! Accepts the same inputs as `describe` (a v3 `--query-yaml` body) and runs
//! the portable validator that also backs the SDK `validate()` methods.

use anyhow::{bail, Result};

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::metadata::MetadataOptions;
use crate::core::runtime_query::validate_input;
use genomehubs_query::query::SplitQueryDocument;

/// Validate a query and print the error list as JSON to stdout.
///
/// Prints `[]` and returns `Ok(())` when the query is valid. Returns an error
/// (non-zero exit status) when the query has any validation errors.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
    metadata: &MetadataOptions,
) -> Result<()> {
    let errors = validate_input(runtime, registry, documents, metadata)?;
    println!("{}", serde_json::to_string_pretty(&errors)?);
    if errors.is_empty() {
        return Ok(());
    }
    bail!("query is invalid: {} error(s)", errors.len())
}
