//! Local report generation from TSV/CSV data files.
//!
//! Provides [`builder::local_plot_spec`] (and its JSON wrapper
//! [`builder::local_plot_spec_json`]) for building a [`crate::report::plot_spec::PlotSpec`]
//! without an API call, and [`tsv::detect_delimiter`] / [`tsv::read_delimited`]
//! for reading delimited text.

pub mod builder;
pub mod tsv;

pub use builder::{local_plot_spec, local_plot_spec_json, LocalReportError};
pub use tsv::{detect_delimiter, read_delimited};
