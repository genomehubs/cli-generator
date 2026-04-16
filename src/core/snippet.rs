//! Code snippet generation for all languages.
//!
//! Generates runnable code examples in Python, R, JavaScript, etc. suitable for
//! embedding in UIs or documentation.

use crate::core::config::SiteConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Context as TeraContext, Tera};

/// Represents a single query as built by an SDK or UI.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuerySnapshot {
    /// Filters: (field_name, operator, value)
    pub filters: Vec<(String, String, String)>,
    /// Sorts: (field_name, direction)
    pub sorts: Vec<(String, String)>,
    /// CLI flags, e.g., ["genome-size", "assembly"]
    pub flags: Vec<String>,
    /// Selected output fields
    pub selections: Vec<String>,
    /// Traversal context: (field_name, direction)
    pub traversal: Option<(String, String)>,
    /// Summaries: (field_name, modifier)
    pub summaries: Vec<(String, String)>,
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

        self.tera
            .render(&template_name, &ctx)
            .with_context(|| format!("rendering {} snippet", language))
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
        ctx.insert("filters", &query.filters);
        ctx.insert("sorts", &query.sorts);
        ctx.insert("flags", &query.flags);
        ctx.insert("selections", &query.selections);
        ctx.insert("traversal", &query.traversal);
        ctx.insert("summaries", &query.summaries);
        ctx.insert("site_name", &site.name);
        ctx.insert("sdk_name", &site.resolved_sdk_name());
        ctx.insert("api_base", &site.api_base);
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_generator_renders_python() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "testsite".to_string(),
            display_name: "Test Site".to_string(),
            ..Default::default()
        };
        let query = QuerySnapshot {
            filters: vec![(
                "genome_size".to_string(),
                ">=".to_string(),
                "1000000000".to_string(),
            )],
            sorts: vec![],
            flags: vec![],
            selections: vec![],
            traversal: None,
            summaries: vec![],
        };

        let snippet = gen.render_snippet(&query, "python", &site).unwrap();

        assert!(snippet.contains("QueryBuilder"));
        assert!(snippet.contains("genome_size"));
        assert!(snippet.contains(">="));
    }
}
