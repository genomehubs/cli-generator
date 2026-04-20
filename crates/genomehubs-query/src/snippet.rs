//! Code snippet generation for all languages in WASM.
//!
//! Generates runnable code examples in Python, R, JavaScript, and CLI suitable for
//! embedding in UIs or documentation. Templates are embedded and compiled into WASM.

use crate::types::{QuerySnapshot, SiteConfig};
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use tera::{Context as TeraContext, Tera};

/// Generates runnable code snippets in multiple languages.
pub struct SnippetGenerator {
    tera: Tera,
}

impl SnippetGenerator {
    /// Create a new snippet generator with bundled templates.
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        // Embed all snippet templates
        tera.add_raw_template(
            "python_snippet",
            include_str!("../../../templates/snippets/python_snippet.tera"),
        )?;

        tera.add_raw_template(
            "r_snippet",
            include_str!("../../../templates/snippets/r_snippet.tera"),
        )?;

        tera.add_raw_template(
            "javascript_snippet",
            include_str!("../../../templates/snippets/js_snippet.tera"),
        )?;

        tera.add_raw_template(
            "cli_snippet",
            include_str!("../../../templates/snippets/cli_snippet.tera"),
        )?;

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
            .map_err(|e| anyhow::anyhow!("rendering {} snippet: {}", language, e))
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
        ctx
    }
}

/// Serialize snippet results to JSON for WASM FFI.
pub fn snippets_to_json(snippets: &HashMap<String, String>) -> String {
    let result = snippets
        .iter()
        .map(|(lang, code)| (lang.clone(), json!(code)))
        .collect::<HashMap<_, _>>();
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_generator_renders_python() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "testsite".to_string(),
            ..Default::default()
        };
        let query = QuerySnapshot {
            index: "taxon".to_string(),
            ..Default::default()
        };
        let snippet = gen.render_snippet(&query, "python", &site).unwrap();
        assert!(snippet.contains("QueryBuilder"));
        assert!(snippet.contains("testsite"));
    }

    #[test]
    fn snippet_generator_renders_js() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "goat".to_string(),
            sdk_name: Some("goat_sdk".to_string()),
            ..Default::default()
        };
        let query = QuerySnapshot {
            index: "assembly".to_string(),
            taxa: vec!["Homo sapiens".to_string()],
            ..Default::default()
        };
        let snippet = gen.render_snippet(&query, "javascript", &site).unwrap();
        assert!(snippet.contains("QueryBuilder"));
        assert!(snippet.contains("assembly"));
    }
}
