//! `genomehubs local-report` — build a PlotSpec from a local data file.
//!
//! Reads a TSV/CSV file (or stdin), calls `local_plot_spec_json` from the
//! `genomehubs-query` crate, and writes the resulting PlotSpec JSON to a
//! file or stdout.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;

use anyhow::{Context, Result};
use genomehubs_query::local_report::{detect_delimiter, local_plot_spec_json};

/// Run the `local-report` subcommand.
pub fn run(
    input: Option<&Path>,
    report_type: &str,
    x_col: Option<&str>,
    y_col: Option<&str>,
    display_json: Option<&str>,
    delimiter_override: Option<char>,
    output: Option<&Path>,
) -> Result<()> {
    let content = read_input(input)?;

    let inferred_delimiter = delimiter_override.unwrap_or_else(|| detect_delimiter(input));
    let delimiter_str = if inferred_delimiter == ',' { "," } else { "\t" };

    let column_map = build_column_map(x_col, y_col);
    let column_map_json = serde_json::to_string(&column_map).context("serialising column map")?;

    let display = display_json.unwrap_or("{}");

    let result_json = local_plot_spec_json(
        &content,
        report_type,
        &column_map_json,
        display,
        delimiter_str,
    );

    // Check for error response
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&result_json) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            anyhow::bail!("local-report error: {err}");
        }
    }

    write_output(output, &result_json)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Read the full content of the input file, or stdin if no path is given.
fn read_input(path: Option<&Path>) -> Result<String> {
    match path {
        Some(p) => std::fs::read_to_string(p)
            .with_context(|| format!("reading input file '{}'", p.display())),
        None => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .context("reading from stdin")?;
            Ok(buf)
        }
    }
}

/// Build the axis column map from optional x/y overrides.
fn build_column_map<'a>(x: Option<&'a str>, y: Option<&'a str>) -> HashMap<&'a str, &'a str> {
    let mut map = HashMap::new();
    if let Some(xc) = x {
        map.insert("x", xc);
    }
    if let Some(yc) = y {
        map.insert("y", yc);
    }
    map
}

/// Write `content` to `path`, or to stdout if `path` is `None`.
fn write_output(path: Option<&Path>, content: &str) -> Result<()> {
    match path {
        Some(p) => std::fs::write(p, content)
            .with_context(|| format!("writing output to '{}'", p.display())),
        None => {
            println!("{content}");
            Ok(())
        }
    }
}
