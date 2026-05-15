//! Code snippet generation for all languages.
//!
//! Generates runnable code examples in Python, R, JavaScript, and CLI suitable for
//! embedding in UIs or documentation.  This module is embedded verbatim into
//! generated site repos, so it must be self-contained — no `genomehubs_query`
//! dependency.

use crate::core::config::SiteConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Context as TeraContext, Tera};

/// Snapshot of a `ReportBuilder` configuration.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ReportSnapshot {
    /// Report type, e.g. `"histogram"`, `"scatter"`, `"map"`, `"tree"`.
    pub report_type: String,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub cat: Option<String>,
    #[serde(default)]
    pub rank: Option<String>,
}

/// Snapshot of a positional request.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PositionalSnapshot {
    /// Report type, e.g. `"oxford"`, `"painting"`, `"busco"`.
    pub report: String,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub assemblies: Vec<String>,
}

/// Represents a single query as built by an SDK or UI.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct QuerySnapshot {
    /// Index name, e.g. `"taxon"` or `"assembly"`.
    #[serde(default)]
    pub index: String,
    /// Taxon names to filter by.
    #[serde(default)]
    pub taxa: Vec<String>,
    /// How the taxon filter is applied: `"name"`, `"tree"`, or `"lineage"`.
    #[serde(default)]
    pub taxon_filter: String,
    /// Restrict results to this taxonomic rank, e.g. `"species"`.
    #[serde(default)]
    pub rank: Option<String>,
    /// Filters: (field_name, operator, value)
    #[serde(default)]
    pub filters: Vec<(String, String, String)>,
    /// Sorts: (field_name, direction)
    #[serde(default)]
    pub sorts: Vec<(String, String)>,
    /// CLI flags, e.g., ["genome-size", "assembly"]
    #[serde(default)]
    pub flags: Vec<String>,
    /// Selected output fields
    #[serde(default)]
    pub selections: Vec<String>,
    /// Traversal context: (field_name, direction)
    #[serde(default)]
    pub traversal: Option<(String, String)>,
    /// Summaries: (field_name, modifier)
    #[serde(default)]
    pub summaries: Vec<(String, String)>,
    /// Which API call to show in the snippet: `"search"` (default), `"count"`,
    /// `"report"`, `"positional"`, `"search_batch"`, `"count_batch"`.
    #[serde(default)]
    pub call_type: String,
    /// Report configuration, used when `call_type = "report"`.
    #[serde(default)]
    pub report: Option<ReportSnapshot>,
    /// Batch queries, used when `call_type = "search_batch"` or `"count_batch"`.
    #[serde(default)]
    pub batch_queries: Vec<QuerySnapshot>,
    /// Positional configuration, used when `call_type = "positional"`.
    #[serde(default)]
    pub positional: Option<PositionalSnapshot>,
}

/// Generates runnable code snippets in multiple languages.
pub struct SnippetGenerator {
    tera: Tera,
}

impl SnippetGenerator {
    /// Create a new snippet generator with bundled templates.
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        tera.add_raw_template(
            "python_snippet",
            include_str!("../../templates/snippets/python_snippet.tera"),
        )
        .context("loading python_snippet template")?;

        tera.add_raw_template(
            "r_snippet",
            include_str!("../../templates/snippets/r_snippet.tera"),
        )
        .context("loading r_snippet template")?;

        tera.add_raw_template(
            "javascript_snippet",
            include_str!("../../templates/snippets/js_snippet.tera"),
        )
        .context("loading javascript_snippet template")?;

        tera.add_raw_template(
            "cli_snippet",
            include_str!("../../templates/snippets/cli_snippet.tera"),
        )
        .context("loading cli_snippet template")?;

        Ok(Self { tera })
    }

    /// Render a single-language snippet.
    pub fn render_snippet(
        &self,
        query: &QuerySnapshot,
        language: &str,
        site: &SiteConfig,
    ) -> Result<String> {
        let template_name = format!("{}_snippet", language);
        let ctx = self.build_context(query, site);

        let raw = self
            .tera
            .render(&template_name, &ctx)
            .with_context(|| format!("rendering {} snippet", language))?;
        Ok(normalize_blank_lines(&raw))
    }

    /// Render snippets in multiple languages.
    pub fn render_all_snippets(
        &self,
        query: &QuerySnapshot,
        site: &SiteConfig,
        languages: &[&str],
    ) -> Result<HashMap<String, String>> {
        let mut snippets = HashMap::new();
        for lang in languages {
            let snippet = self.render_snippet(query, lang, site)?;
            snippets.insert(lang.to_string(), snippet);
        }
        Ok(snippets)
    }

    fn build_context(&self, query: &QuerySnapshot, site: &SiteConfig) -> TeraContext {
        let mut ctx = TeraContext::new();
        ctx.insert("index", &query.index);
        ctx.insert("taxa", &query.taxa);
        ctx.insert("taxon_filter", &query.taxon_filter);
        ctx.insert("rank", &query.rank);
        ctx.insert("filters", &query.filters);
        ctx.insert("sorts", &query.sorts);
        ctx.insert("flags", &query.flags);
        ctx.insert("selections", &query.selections);
        ctx.insert("traversal", &query.traversal);
        ctx.insert("summaries", &query.summaries);
        ctx.insert("site_name", &site.name);
        ctx.insert("sdk_name", &site.resolved_sdk_name());
        ctx.insert("api_base", &site.api_base);

        let call_type = if query.call_type.is_empty() {
            "search"
        } else {
            query.call_type.as_str()
        };
        ctx.insert("call_type", call_type);

        if let Some(report) = &query.report {
            ctx.insert("report_type", &report.report_type);
            ctx.insert("report_x", &report.x);
            ctx.insert("report_y", &report.y);
            ctx.insert("report_cat", &report.cat);
            ctx.insert("report_rank", &report.rank);
        } else {
            ctx.insert("report_type", &Option::<String>::None);
            ctx.insert("report_x", &Option::<String>::None);
            ctx.insert("report_y", &Option::<String>::None);
            ctx.insert("report_cat", &Option::<String>::None);
            ctx.insert("report_rank", &Option::<String>::None);
        }

        let batch_indices: Vec<&str> = query
            .batch_queries
            .iter()
            .map(|q| q.index.as_str())
            .collect();
        ctx.insert("batch_query_count", &query.batch_queries.len());
        ctx.insert("batch_indices", &batch_indices);

        if let Some(pos) = &query.positional {
            ctx.insert("positional_report", &pos.report);
            ctx.insert("positional_group_by", &pos.group_by);
            ctx.insert("positional_assemblies", &pos.assemblies);
        } else {
            ctx.insert("positional_report", &Option::<String>::None);
            ctx.insert("positional_group_by", &Option::<String>::None);
            ctx.insert("positional_assemblies", &Vec::<String>::new());
        }

        ctx
    }
}

/// Collapse runs of 3+ consecutive newlines down to 2 (one blank line).
///
/// Template conditionals that evaluate to empty leave behind extra blank lines;
/// this normalises the output to at most one blank line between sections.
fn normalize_blank_lines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut consecutive_newlines: usize = 0;
    for ch in s.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::SiteConfig;

    #[test]
    fn snippet_generator_renders_python() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "testsite".to_string(),
            display_name: "Test Site".to_string(),
            ..Default::default()
        };
        let query = QuerySnapshot {
            index: "taxon".to_string(),
            filters: vec![(
                "genome_size".to_string(),
                ">=".to_string(),
                "1000000000".to_string(),
            )],
            ..Default::default()
        };
        let snippet = gen.render_snippet(&query, "python", &site).unwrap();
        assert!(snippet.contains("QueryBuilder"));
        assert!(snippet.contains("genome_size"));
    }

    #[test]
    fn snippet_generator_count_call_type() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "testsite".to_string(),
            ..Default::default()
        };
        let query = QuerySnapshot {
            index: "taxon".to_string(),
            call_type: "count".to_string(),
            ..Default::default()
        };
        let snippet = gen.render_snippet(&query, "python", &site).unwrap();
        assert!(snippet.to_lowercase().contains("count"));
    }
}
