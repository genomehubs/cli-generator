//! `describe` command: print a plain-English description of a query.

use anyhow::Result;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::metadata::MetadataOptions;
use crate::core::runtime_query::describe_input;
use genomehubs_query::query::SplitQueryDocument;

/// Describe a v3 query and print the text to stdout.
pub fn run(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
    metadata: &MetadataOptions,
    verbose: bool,
) -> Result<()> {
    let text = describe_input(runtime, registry, documents, metadata, verbose)?;
    println!("{text}");
    Ok(())
}
