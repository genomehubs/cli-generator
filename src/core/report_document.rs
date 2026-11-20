//! Combined query, params, display, and report YAML for the `report` command.
//!
//! A combined document holds `query:` and `params:` sections, the `display:`
//! and `include_plot_spec:` request settings, and report config keys at the top
//! level. Either `--query-yaml` or `--report-yaml` can carry the document.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use serde_yaml::{Mapping, Value as YamlValue};

use crate::core::runtime_query::read_yaml_argument;

/// Top-level keys that hold the query and params sections.
const SECTION_KEYS: [&str; 2] = ["query", "params"];

/// Top-level key that marks a document as a combined report document.
const REPORT_KEY: &str = "report";

/// Report request resolved from the `--query-yaml` and `--report-yaml` arguments.
#[derive(Debug, Default, PartialEq)]
pub struct ReportRequest {
    /// Query YAML for the query flags, or `None` to use the default index.
    pub query_yaml: Option<String>,
    /// Report config YAML sent as `report_yaml`.
    pub report_yaml: String,
    /// Display options set in a document; the `--display` flag takes precedence.
    pub display: Option<Value>,
    /// True when a document sets `include_plot_spec: true`.
    pub include_plot_spec: bool,
}

/// One YAML argument split into its sections, settings, and remaining keys.
#[derive(Debug, Default)]
struct DocumentParts {
    /// True when the document has a `report` key or a `query`/`params` section.
    combined: bool,
    /// The `query` and `params` sections of a combined document.
    sections: Mapping,
    /// Remaining keys: report config for a combined document, flat query otherwise.
    rest: Mapping,
    display: Option<Value>,
    include_plot_spec: Option<bool>,
}

/// Resolve the query and report YAML from the values of the two flags.
///
/// # Errors
///
/// Returns an error when a key is set in both flags, when no report config is
/// found, or when a YAML argument cannot be read or parsed.
pub fn resolve_report_request(
    query_arg: Option<&str>,
    report_arg: Option<&str>,
) -> Result<ReportRequest> {
    let query_parts = query_arg.map(parse_argument).transpose()?;
    let report_parts = report_arg.map(parse_argument).transpose()?;
    let (query, report) = (query_parts.as_ref(), report_parts.as_ref());
    let include_plot_spec = [query, report]
        .into_iter()
        .flatten()
        .any(|parts| parts.include_plot_spec == Some(true));
    Ok(ReportRequest {
        query_yaml: resolve_query_yaml(query, report)?,
        report_yaml: resolve_report_yaml(query, report)?,
        display: resolve_display(query, report)?,
        include_plot_spec,
    })
}

/// Read one YAML argument and split it into its parts.
fn parse_argument(value: &str) -> Result<DocumentParts> {
    split_document(&read_yaml_argument(value)?)
}

/// Split a YAML document into sections, settings, and the remaining keys.
fn split_document(text: &str) -> Result<DocumentParts> {
    let mut mapping = parse_mapping(text)?;
    let combined = mapping.contains_key(REPORT_KEY)
        || SECTION_KEYS.iter().any(|key| mapping.contains_key(*key));
    let display = mapping
        .remove("display")
        .map(serde_json::to_value)
        .transpose()
        .context("display must be expressible as JSON")?;
    let include_plot_spec = mapping
        .remove("include_plot_spec")
        .map(|value| {
            value
                .as_bool()
                .context("include_plot_spec must be true or false")
        })
        .transpose()?;
    let sections = take_sections(&mut mapping);
    Ok(DocumentParts {
        combined,
        sections,
        rest: mapping,
        display,
        include_plot_spec,
    })
}

/// Parse YAML text as a mapping; an empty document is an empty mapping.
fn parse_mapping(text: &str) -> Result<Mapping> {
    match serde_yaml::from_str::<YamlValue>(text).context("parsing YAML document")? {
        YamlValue::Mapping(mapping) => Ok(mapping),
        YamlValue::Null => Ok(Mapping::new()),
        _ => bail!("YAML document must be a mapping of keys to values"),
    }
}

/// Move the non-null `query` and `params` keys out of `mapping` into a new mapping.
fn take_sections(mapping: &mut Mapping) -> Mapping {
    let mut sections = Mapping::new();
    for key in SECTION_KEYS {
        match mapping.remove(key) {
            Some(YamlValue::Null) | None => {}
            Some(value) => {
                sections.insert(YamlValue::from(key), value);
            }
        }
    }
    sections
}

/// Resolve the query YAML from the query flag and the report flag.
fn resolve_query_yaml(
    query: Option<&DocumentParts>,
    report: Option<&DocumentParts>,
) -> Result<Option<String>> {
    let from_query = query
        .map(|parts| query_content(parts, false))
        .transpose()?
        .flatten();
    let from_report = report
        .map(|parts| query_content(parts, true))
        .transpose()?
        .flatten();
    single_source(from_query, from_report, "query or params")
}

/// Query YAML a document supplies: its sections when combined, or its flat query when passed as `--query-yaml`.
fn query_content(parts: &DocumentParts, from_report_flag: bool) -> Result<Option<String>> {
    if parts.combined {
        return mapping_yaml(&parts.sections);
    }
    if from_report_flag {
        return Ok(None);
    }
    mapping_yaml(&parts.rest)
}

/// Resolve the report config from `--report-yaml`, or from a combined `--query-yaml`.
fn resolve_report_yaml(
    query: Option<&DocumentParts>,
    report: Option<&DocumentParts>,
) -> Result<String> {
    let from_report = report
        .map(|parts| &parts.rest)
        .filter(|rest| !rest.is_empty());
    let from_query = query
        .filter(|parts| parts.combined)
        .map(|parts| &parts.rest)
        .filter(|rest| !rest.is_empty());
    match (from_report, from_query) {
        (Some(_), Some(_)) => bail!("report config given in both --query-yaml and --report-yaml"),
        (Some(config), None) | (None, Some(config)) => {
            serde_yaml::to_string(config).context("serialising report config")
        }
        (None, None) => bail!(
            "no report config: pass --report-yaml, or put report keys in a combined --query-yaml"
        ),
    }
}

/// Resolve the display settings from whichever flag set them.
fn resolve_display(
    query: Option<&DocumentParts>,
    report: Option<&DocumentParts>,
) -> Result<Option<Value>> {
    let from_query = query.and_then(|parts| parts.display.clone());
    let from_report = report.and_then(|parts| parts.display.clone());
    single_source(from_query, from_report, "display")
}

/// Pick the value set by one flag; setting it in both flags is an error.
fn single_source<T>(
    from_query: Option<T>,
    from_report: Option<T>,
    what: &str,
) -> Result<Option<T>> {
    match (from_query, from_report) {
        (Some(_), Some(_)) => bail!("{what} given in both --query-yaml and --report-yaml"),
        (found, None) | (None, found) => Ok(found),
    }
}

/// Serialise a non-empty mapping as YAML text; an empty mapping yields `None`.
fn mapping_yaml(mapping: &Mapping) -> Result<Option<String>> {
    if mapping.is_empty() {
        return Ok(None);
    }
    serde_yaml::to_string(mapping)
        .map(Some)
        .context("serialising YAML section")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn as_json(yaml: &str) -> Value {
        let parsed: YamlValue = serde_yaml::from_str(yaml).unwrap();
        serde_json::to_value(parsed).unwrap()
    }

    const COMBINED_QUERY: &str = "query:\n  index: taxon\n  taxa: [Canidae]\nparams:\n  taxonomy: ncbi\ndisplay:\n  width: 800\n  tree:\n    tree_style: circ\n";

    #[test]
    fn combined_query_file_supplies_query_display_and_report_config() {
        let request =
            resolve_report_request(Some(COMBINED_QUERY), Some("report: tree\nrank: genus\n"))
                .unwrap();
        assert_eq!(
            as_json(request.query_yaml.as_deref().unwrap()),
            json!({"query": {"index": "taxon", "taxa": ["Canidae"]}, "params": {"taxonomy": "ncbi"}})
        );
        assert_eq!(
            as_json(&request.report_yaml),
            json!({"report": "tree", "rank": "genus"})
        );
        assert_eq!(
            request.display,
            Some(json!({"width": 800, "tree": {"tree_style": "circ"}}))
        );
        assert!(!request.include_plot_spec);
    }

    #[test]
    fn combined_report_file_supplies_query_sections_and_report_config() {
        let text = "query:\n  index: taxon\nreport: tree\nrank: genus\ninclude_plot_spec: true\n";
        let request = resolve_report_request(None, Some(text)).unwrap();
        assert_eq!(
            as_json(request.query_yaml.as_deref().unwrap()),
            json!({"query": {"index": "taxon"}})
        );
        assert_eq!(
            as_json(&request.report_yaml),
            json!({"report": "tree", "rank": "genus"})
        );
        assert!(request.include_plot_spec);
    }

    #[test]
    fn combined_query_file_alone_supplies_report_config() {
        let text = "query:\n  index: taxon\nreport: tree\nrank: genus\n";
        let request = resolve_report_request(Some(text), None).unwrap();
        assert_eq!(
            as_json(&request.report_yaml),
            json!({"report": "tree", "rank": "genus"})
        );
        assert_eq!(
            as_json(request.query_yaml.as_deref().unwrap()),
            json!({"query": {"index": "taxon"}})
        );
    }

    #[test]
    fn flat_query_with_report_file_keeps_flat_query() {
        let request = resolve_report_request(
            Some("index: taxon\nsize: 5\ndisplay:\n  width: 400\n"),
            Some("report: tree\nrank: genus\n"),
        )
        .unwrap();
        assert_eq!(
            as_json(request.query_yaml.as_deref().unwrap()),
            json!({"index": "taxon", "size": 5})
        );
        assert_eq!(
            as_json(&request.report_yaml),
            json!({"report": "tree", "rank": "genus"})
        );
        assert_eq!(request.display, Some(json!({"width": 400})));
    }

    #[test]
    fn report_file_without_sections_uses_default_query() {
        let request = resolve_report_request(None, Some("report: tree\n")).unwrap();
        assert_eq!(request.query_yaml, None);
        assert_eq!(as_json(&request.report_yaml), json!({"report": "tree"}));
    }

    #[test]
    fn flat_query_alone_has_no_report_config() {
        let err = resolve_report_request(Some("index: taxon\n"), None).unwrap_err();
        assert!(err.to_string().contains("no report config"), "{err}");
    }

    #[test]
    fn report_config_in_both_flags_is_an_error() {
        let query = "query:\n  index: taxon\nreport: tree\n";
        let err = resolve_report_request(Some(query), Some("report: tree\n")).unwrap_err();
        assert!(
            err.to_string().contains("report config given in both"),
            "{err}"
        );
    }

    #[test]
    fn display_in_both_flags_is_an_error() {
        let report = "report: tree\ndisplay:\n  width: 400\n";
        let err = resolve_report_request(Some(COMBINED_QUERY), Some(report)).unwrap_err();
        assert!(err.to_string().contains("display given in both"), "{err}");
    }

    #[test]
    fn query_sections_in_both_flags_is_an_error() {
        let report = "query:\n  index: assembly\nreport: tree\n";
        let err = resolve_report_request(Some("index: taxon\n"), Some(report)).unwrap_err();
        assert!(
            err.to_string().contains("query or params given in both"),
            "{err}"
        );
    }

    #[test]
    fn non_boolean_include_plot_spec_is_an_error() {
        let err = resolve_report_request(None, Some("report: tree\ninclude_plot_spec: maybe\n"))
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("include_plot_spec must be true or false"),
            "{err}"
        );
    }
}
