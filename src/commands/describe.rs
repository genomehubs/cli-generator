//! `describe` command: print a plain-English description of a query.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::metadata::MetadataOptions;
use crate::core::runtime_query::{describe_input, QueryInput};

/// Describe a query (v2 or v3 input) and print the text to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    input: &QueryInput,
    metadata: &MetadataOptions,
    verbose: bool,
) -> Result<()> {
    let text = describe_input(runtime, registry, input, metadata, verbose)?;
    println!("{text}");
    Ok(())
}
