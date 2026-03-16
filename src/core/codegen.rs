//! Code generation via Tera templates.
//!
//! [`CodeGenerator`] accepts a [`SiteConfig`], a [`CliOptionsConfig`], and
//! per-index [`FieldDef`] lists, then renders each Tera template in
//! `templates/` into a `String`.  The caller is responsible for writing the
//! rendered strings to disk (see `commands::new` and `commands::update`).
//!
//! Templates live in the `templates/` directory bundled with the generator
//! binary via [`include_str!`].

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Serialize;
use tera::{Context as TeraContext, Tera};

use crate::core::{
    config::{CliOptionsConfig, FieldGroup, SiteConfig},
    fetch::FieldDef,
};

// ── Bundled templates ─────────────────────────────────────────────────────────

/// All Tera templates are compiled into the binary at build time.
fn make_tera() -> Result<Tera> {
    let mut tera = Tera::default();
    tera.add_raw_template(
        "cli_meta.rs",
        include_str!("../../templates/cli_meta.rs.tera"),
    )
    .context("loading cli_meta.rs template")?;
    tera.add_raw_template(
        "indexes.rs",
        include_str!("../../templates/indexes.rs.tera"),
    )
    .context("loading indexes.rs template")?;
    tera.add_raw_template("fields.rs", include_str!("../../templates/fields.rs.tera"))
        .context("loading fields.rs template")?;
    tera.add_raw_template("groups.rs", include_str!("../../templates/groups.rs.tera"))
        .context("loading groups.rs template")?;
    tera.add_raw_template(
        "cli_flags.rs",
        include_str!("../../templates/cli_flags.rs.tera"),
    )
    .context("loading cli_flags.rs template")?;
    tera.add_raw_template("client.rs", include_str!("../../templates/client.rs.tera"))
        .context("loading client.rs template")?;
    tera.add_raw_template("output.rs", include_str!("../../templates/output.rs.tera"))
        .context("loading output.rs template")?;
    tera.add_raw_template(
        "generated_mod.rs",
        include_str!("../../templates/generated_mod.rs.tera"),
    )
    .context("loading generated_mod.rs template")?;
    tera.add_raw_template("main.rs", include_str!("../../templates/main.rs.tera"))
        .context("loading main.rs template")?;
    tera.add_raw_template(
        "GETTING_STARTED.md",
        include_str!("../../templates/GETTING_STARTED.md.tera"),
    )
    .context("loading GETTING_STARTED.md template")?;
    Ok(tera)
}

// ── Template context types ────────────────────────────────────────────────────

/// Serialisable view of a [`FieldDef`] passed into templates.
#[derive(Debug, Serialize)]
struct TemplateField {
    name: String,
    display_group: String,
    display_name: String,
    description: String,
    field_type: String,
    enum_values: Vec<String>,
    display_level: u8,
}

impl From<&FieldDef> for TemplateField {
    fn from(f: &FieldDef) -> Self {
        Self {
            name: f.name.clone(),
            display_group: f.display_group.clone().unwrap_or_default(),
            display_name: f.display_name.clone().unwrap_or_else(|| f.name.clone()),
            description: f.description.clone().unwrap_or_default(),
            field_type: f
                .field_type
                .clone()
                .unwrap_or_else(|| "keyword".to_string()),
            enum_values: f
                .constraint
                .as_ref()
                .map(|c| c.enum_values.clone())
                .unwrap_or_default(),
            display_level: f.display_level.unwrap_or(2),
        }
    }
}

/// Serialisable view of a display group and the fields belonging to it.
#[derive(Debug, Serialize)]
struct TemplateGroup {
    name: String,
    fields: Vec<String>,
}

/// Serialisable view of a [`FieldGroup`] flag for template rendering.
#[derive(Debug, Serialize)]
struct TemplateFlag {
    flag: String,
    flag_snake: String,
    description: String,
    display_groups: Vec<String>,
    compat_aliases: Vec<String>,
}

impl TemplateFlag {
    fn from_group(fg: &FieldGroup) -> Self {
        Self {
            flag_snake: fg.flag.replace('-', "_"),
            flag: fg.flag.clone(),
            description: fg.description.clone(),
            display_groups: fg.display_groups.clone(),
            compat_aliases: fg.compat_aliases.clone(),
        }
    }
}

/// Per-index render payload passed to templates that iterate over indexes.
#[derive(Debug, Serialize)]
struct TemplateIndex {
    name: String,
    fields: Vec<TemplateField>,
    groups: Vec<TemplateGroup>,
    flags: Vec<TemplateFlag>,
}

// ── CodeGenerator ─────────────────────────────────────────────────────────────

/// Renders Tera templates into a map of `filename → rendered_source`.
pub struct CodeGenerator {
    tera: Tera,
}

impl CodeGenerator {
    /// Create a new generator by loading the bundled templates.
    pub fn new() -> Result<Self> {
        Ok(Self { tera: make_tera()? })
    }

    /// Render all templates for a site and return a map of file names to
    /// rendered source strings.
    ///
    /// The map keys are the target paths **relative to the generated repo
    /// root**, e.g. `"src/generated/fields.rs"`.
    pub fn render_all(
        &self,
        site: &SiteConfig,
        options: &CliOptionsConfig,
        fields_by_index: &HashMap<String, Vec<FieldDef>>,
    ) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();

        let ctx = self.build_context(site, options, fields_by_index);

        for template_name in &[
            "cli_meta.rs",
            "indexes.rs",
            "fields.rs",
            "groups.rs",
            "cli_flags.rs",
            "client.rs",
            "output.rs",
            "generated_mod.rs",
            "main.rs",
            "GETTING_STARTED.md",
        ] {
            let rendered = self
                .tera
                .render(template_name, &ctx)
                .with_context(|| format!("rendering template '{template_name}'"))?;

            let dest_path = template_name_to_dest(template_name);
            out.insert(dest_path, rendered);
        }

        Ok(out)
    }

    /// Build the Tera rendering context from the inputs.
    fn build_context(
        &self,
        site: &SiteConfig,
        options: &CliOptionsConfig,
        fields_by_index: &HashMap<String, Vec<FieldDef>>,
    ) -> TeraContext {
        let indexes: Vec<TemplateIndex> = site
            .indexes
            .iter()
            .map(|idx| build_template_index(idx, options, fields_by_index))
            .collect();

        let mut ctx = TeraContext::new();
        ctx.insert("site_name", &site.name);
        ctx.insert("site_display_name", &site.display_name);
        ctx.insert("api_base", &site.api_base);
        ctx.insert("api_version", &site.api_version);
        ctx.insert("goat_cli_compat", &site.compat.goat_cli);
        ctx.insert("indexes", &indexes);
        ctx
    }
}

/// Build the per-index template payload.
fn build_template_index(
    index: &crate::core::config::IndexDef,
    options: &CliOptionsConfig,
    fields_by_index: &HashMap<String, Vec<FieldDef>>,
) -> TemplateIndex {
    let raw_fields = fields_by_index
        .get(&index.name)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let fields: Vec<TemplateField> = raw_fields.iter().map(TemplateField::from).collect();

    let groups = build_groups(raw_fields);

    let flags = options
        .indexes
        .get(&index.name)
        .map(|opts| {
            opts.field_groups
                .iter()
                .map(TemplateFlag::from_group)
                .collect()
        })
        .unwrap_or_default();

    TemplateIndex {
        name: index.name.clone(),
        fields,
        groups,
        flags,
    }
}

/// Aggregate fields by `display_group`, preserving insertion order of groups.
fn build_groups(fields: &[FieldDef]) -> Vec<TemplateGroup> {
    let mut group_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut group_order: Vec<String> = Vec::new();

    for field in fields {
        let group = field.display_group.clone().unwrap_or_default();
        if !group_order.contains(&group) {
            group_order.push(group.clone());
        }
        group_map.entry(group).or_default().push(field.name.clone());
    }

    group_order
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|name| {
            let mut field_names = group_map.remove(&name).unwrap_or_default();
            field_names.sort();
            TemplateGroup {
                name,
                fields: field_names,
            }
        })
        .collect()
}

/// Map a template name to its destination path in the generated repo.
fn template_name_to_dest(template_name: &str) -> String {
    match template_name {
        "cli_meta.rs" => "src/cli_meta.rs".to_string(),
        "main.rs" => "src/main.rs".to_string(),
        "GETTING_STARTED.md" => "GETTING_STARTED.md".to_string(),
        "generated_mod.rs" => "src/generated/mod.rs".to_string(),
        other => format!("src/generated/{other}"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{CompatConfig, FieldGroup, IndexDef, IndexOptions, SiteConfig};
    use crate::core::fetch::FieldDef;

    fn sample_site() -> SiteConfig {
        SiteConfig {
            name: "testsite".to_string(),
            display_name: "Test Site".to_string(),
            api_base: "https://example.com/api".to_string(),
            api_version: "v2".to_string(),
            indexes: vec![IndexDef {
                name: "taxon".to_string(),
                result_fields_endpoint: None,
            }],
            compat: CompatConfig::default(),
        }
    }

    fn sample_fields() -> Vec<FieldDef> {
        vec![
            FieldDef {
                name: "genome_size".to_string(),
                display_group: Some("genome_size".to_string()),
                display_name: Some("Genome size".to_string()),
                description: None,
                field_type: Some("long".to_string()),
                constraint: None,
                display_level: Some(1),
            },
            FieldDef {
                name: "assembly_level".to_string(),
                display_group: Some("assembly".to_string()),
                display_name: Some("Assembly level".to_string()),
                description: None,
                field_type: Some("keyword".to_string()),
                constraint: None,
                display_level: Some(1),
            },
        ]
    }

    fn sample_options() -> CliOptionsConfig {
        CliOptionsConfig {
            indexes: {
                let mut m = HashMap::new();
                m.insert(
                    "taxon".to_string(),
                    IndexOptions {
                        field_groups: vec![FieldGroup {
                            flag: "genome-size".to_string(),
                            description: "Genome size fields".to_string(),
                            display_groups: vec!["genome_size".to_string()],
                            compat_aliases: vec![],
                        }],
                    },
                );
                m
            },
        }
    }

    #[test]
    fn build_groups_aggregates_by_display_group() {
        let fields = sample_fields();
        let groups = build_groups(&fields);
        assert_eq!(groups.len(), 2);
        let gs_group = groups.iter().find(|g| g.name == "genome_size").unwrap();
        assert_eq!(gs_group.fields, ["genome_size"]);
    }

    #[test]
    fn template_name_to_dest_maps_cli_meta() {
        assert_eq!(template_name_to_dest("cli_meta.rs"), "src/cli_meta.rs");
    }

    #[test]
    fn template_name_to_dest_maps_generated_files() {
        assert_eq!(
            template_name_to_dest("fields.rs"),
            "src/generated/fields.rs"
        );
    }

    #[test]
    fn codegen_renders_all_templates_without_error() {
        let gen = CodeGenerator::new().unwrap();
        let site = sample_site();
        let options = sample_options();
        let mut fields_by_index = HashMap::new();
        fields_by_index.insert("taxon".to_string(), sample_fields());

        let rendered = gen.render_all(&site, &options, &fields_by_index).unwrap();
        assert!(rendered.contains_key("src/cli_meta.rs"));
        assert!(rendered.contains_key("src/generated/fields.rs"));
        assert!(rendered.contains_key("src/generated/mod.rs"));
    }
}
