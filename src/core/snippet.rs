//! Code snippet generation for all languages.
//!
//! Re-exports the canonical implementation from the `genomehubs-query` crate so
//! that both the PyO3 extension and the CLI binary share a single code path.

pub use genomehubs_query::snippet::SnippetGenerator;
pub use genomehubs_query::types::QuerySnapshot;
