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
        include_str!("../../templates/rust/cli_meta.rs.tera"),
    )
    .context("loading cli_meta.rs template")?;
    tera.add_raw_template(
        "indexes.rs",
        include_str!("../../templates/rust/indexes.rs.tera"),
    )
    .context("loading indexes.rs template")?;
    tera.add_raw_template(
        "fields.rs",
        include_str!("../../templates/rust/fields.rs.tera"),
    )
    .context("loading fields.rs template")?;
    tera.add_raw_template(
        "groups.rs",
        include_str!("../../templates/rust/groups.rs.tera"),
    )
    .context("loading groups.rs template")?;
    tera.add_raw_template(
        "cli_flags.rs",
        include_str!("../../templates/rust/cli_flags.rs.tera"),
    )
    .context("loading cli_flags.rs template")?;
    tera.add_raw_template(
        "client.rs",
        include_str!("../../templates/rust/client.rs.tera"),
    )
    .context("loading client.rs template")?;
    tera.add_raw_template(
        "output.rs",
        include_str!("../../templates/rust/output.rs.tera"),
    )
    .context("loading output.rs template")?;
    tera.add_raw_template(
        "generated_mod.rs",
        include_str!("../../templates/rust/generated_mod.rs.tera"),
    )
    .context("loading generated_mod.rs template")?;
    tera.add_raw_template("main.rs", include_str!("../../templates/rust/main.rs.tera"))
        .context("loading main.rs template")?;
    tera.add_raw_template(
        "GETTING_STARTED.md",
        include_str!("../../templates/shared/GETTING_STARTED.md.tera"),
    )
    .context("loading GETTING_STARTED.md template")?;
    tera.add_raw_template(
        "PREVIEW.md",
        include_str!("../../templates/shared/PREVIEW.md.tera"),
    )
    .context("loading PREVIEW.md template")?;
    tera.add_raw_template(
        "autoupdate.yml",
        include_str!("../../templates/shared/autoupdate.yml.tera"),
    )
    .context("loading autoupdate.yml template")?;
    tera.add_raw_template("ci.yml", include_str!("../../templates/shared/ci.yml.tera"))
        .context("loading ci.yml template")?;
    tera.add_raw_template(
        "field_meta.rs",
        include_str!("../../templates/rust/field_meta.rs.tera"),
    )
    .context("loading field_meta.rs template")?;
    tera.add_raw_template("sdk.rs", include_str!("../../templates/rust/sdk.rs.tera"))
        .context("loading sdk.rs template")?;
    tera.add_raw_template("lib.rs", include_str!("../../templates/rust/lib.rs.tera"))
        .context("loading lib.rs template")?;
    tera.add_raw_template(
        "query.py",
        include_str!("../../templates/python/query.py.tera"),
    )
    .context("loading query.py template")?;
    tera.add_raw_template(
        "site_cli.pyi",
        include_str!("../../templates/python/site_cli.pyi.tera"),
    )
    .context("loading site_cli.pyi template")?;
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
    /// Deprecated aliases for this field used in synonym lookup maps.
    synonyms: Vec<String>,
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
            synonyms: f.synonyms.clone(),
        }
    }
}

/// Compile-time metadata for a field emitted into the `field_meta.rs` template.
#[derive(Debug, Serialize)]
struct TemplateFieldMeta {
    /// Canonical field name.
    name: String,
    /// Processed type for operator validation, e.g. `"long"`, `"keyword"`.
    processed_type: String,
    /// Direction of taxonomy tree traversal (`"up"`, `"down"`, `"both"`), if any.
    traverse_direction: Option<String>,
    /// Valid summary modifiers accepted by the API for this field.
    summary: Vec<String>,
    /// Allowed enum values for constrained keyword fields.
    constraint_enum: Option<Vec<String>>,
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
    /// Optional single-character short code accepted within `--field-groups`.
    short: Option<String>,
    description: String,
    compat_aliases: Vec<String>,
    /// Field names resolved at code-generation time from `display_groups`,
    /// `fields`, and `patterns`.  Baked into the generated `expand()` method.
    resolved_fields: Vec<String>,
}

impl TemplateFlag {
    fn from_group(fg: &FieldGroup, all_fields: &[FieldDef]) -> Self {
        Self {
            flag_snake: fg.flag.replace('-', "_"),
            flag: fg.flag.clone(),
            short: fg.short.clone(),
            description: fg.description.clone(),
            compat_aliases: fg.compat_aliases.clone(),
            resolved_fields: resolve_fields(fg, all_fields),
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
    /// Field metadata for the `field_meta.rs` template.
    meta_fields: Vec<TemplateFieldMeta>,
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
    ) -> Result<HashMap<String, HashMap<String, String>>> {
        let mut all_langs: HashMap<String, HashMap<String, String>> = HashMap::new();

        let default_langs = vec!["rust".to_string(), "shared".to_string()];
        for language in default_langs {
            let rendered = self.render_for_language(&language, site, options, fields_by_index)?;
            all_langs.insert(language.clone(), rendered);
        }
        for language in &site.enabled_sdks {
            let rendered = self.render_for_language(language, site, options, fields_by_index)?;
            all_langs.insert(language.clone(), rendered);
        }

        Ok(all_langs)
    }

    /// Render templates for a single language.
    fn render_for_language(
        &self,
        language: &str,
        site: &SiteConfig,
        options: &CliOptionsConfig,
        fields_by_index: &HashMap<String, Vec<FieldDef>>,
    ) -> Result<HashMap<String, String>> {
        let template_names = match language {
            "rust" => vec![
                "cli_meta.rs",
                "indexes.rs",
                "fields.rs",
                "groups.rs",
                "cli_flags.rs",
                "client.rs",
                "output.rs",
                "field_meta.rs",
                "sdk.rs",
                "lib.rs",
                "generated_mod.rs",
                "main.rs",
            ],
            "shared" => vec![
                "GETTING_STARTED.md",
                "PREVIEW.md",
                "autoupdate.yml",
                "ci.yml",
            ],
            "python" => vec!["query.py", "site_cli.pyi"],
            "r" => vec![], // Empty for Phase 2
            _ => vec![],
        };

        let ctx = self.build_context(site, options, fields_by_index);
        let mut out = HashMap::new();

        for template_name in template_names {
            let rendered = self.tera.render(template_name, &ctx)?;
            let dest_path =
                template_name_to_dest(template_name, language, &site.resolved_sdk_name());
            out.insert(dest_path, rendered);
        }

        // Generate field_meta.json and validation_config.json for JavaScript/Python/R SDKs (shared)
        if language == "shared" {
            // Generate field_meta.json: per-index structure {"taxon": {...}, "assembly": {...}}
            // This allows validate() in all SDKs to extract the correct subset by index.
            let mut field_meta_by_index = serde_json::Map::new();
            for (index, fields) in fields_by_index {
                let mut index_meta = serde_json::Map::new();
                for field in fields {
                    let processed_type = field
                        .processed_type
                        .clone()
                        .or_else(|| field.field_type.clone())
                        .unwrap_or_else(|| "keyword".to_string());
                    let constraint_enum = field
                        .constraint
                        .as_ref()
                        .map(|c| c.enum_values.clone())
                        .filter(|v| !v.is_empty());

                    let meta = serde_json::json!({
                        "processed_type": processed_type,
                        "traverse_direction": field.traverse_direction,
                        "summary": field.summary.clone(),
                        "constraint_enum": constraint_enum
                    });

                    index_meta.insert(field.name.clone(), meta);
                }
                field_meta_by_index.insert(index.clone(), serde_json::Value::Object(index_meta));
            }
            let field_meta_content = serde_json::to_string_pretty(&field_meta_by_index)
                .unwrap_or_else(|_| "{}".to_string());
            // Written to src/generated/ for the Rust build
            out.insert(
                "src/generated/field_meta.json".to_string(),
                field_meta_content.clone(),
            );
            // Also written into the Python package so validate() can find it at runtime
            let sdk_name = site.resolved_sdk_name();
            out.insert(
                format!("python/{sdk_name}/generated/field_meta.json"),
                field_meta_content.clone(),
            );
            // Also written into the R package inst/ dir so system.file() can find it
            let r_package_name = site.name.replace('-', "_").to_lowercase();
            out.insert(
                format!("r/{r_package_name}/inst/generated/field_meta.json"),
                field_meta_content,
            );

            // Generate validation_config.json with defaults
            let validation_config = serde_json::json!({
                "assembly_accession_prefixes": ["gca_", "gcf_", "gcs_", "gcn_", "gcp_", "gcr_", "wgs", "asm"],
                "sample_accession_prefixes": ["srs", "srr", "srx", "sam", "ers", "erp", "erx", "drr", "drx", "samea", "sameg"],
                "taxon_name_classes": ["scientific_name", "common_name", "synonym", "tolid_prefix", "authority"],
                "taxon_filter_types": ["name", "tree", "lineage"]
            });
            let validation_config_content = serde_json::to_string_pretty(&validation_config)
                .unwrap_or_else(|_| "{}".to_string());
            out.insert(
                "src/generated/validation_config.json".to_string(),
                validation_config_content.clone(),
            );
            out.insert(
                format!("python/{sdk_name}/generated/validation_config.json"),
                validation_config_content.clone(),
            );
            out.insert(
                format!("r/{r_package_name}/inst/generated/validation_config.json"),
                validation_config_content,
            );
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
        ctx.insert("ui_base", &site.resolved_ui_base());
        ctx.insert("archive", &site.archive);
        ctx.insert("goat_cli_compat", &site.compat.goat_cli);
        ctx.insert("sdk_name", &site.resolved_sdk_name());
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
                .map(|fg| TemplateFlag::from_group(fg, raw_fields))
                .collect()
        })
        .unwrap_or_default();

    let meta_fields = raw_fields
        .iter()
        .map(|f| {
            let processed_type = f
                .processed_type
                .clone()
                .or_else(|| f.field_type.clone())
                .unwrap_or_else(|| "keyword".to_string());
            let constraint_enum = f
                .constraint
                .as_ref()
                .map(|c| c.enum_values.clone())
                .filter(|v| !v.is_empty());
            TemplateFieldMeta {
                name: f.name.clone(),
                processed_type,
                traverse_direction: f.traverse_direction.clone(),
                summary: f.summary.clone(),
                constraint_enum,
            }
        })
        .collect();

    TemplateIndex {
        name: index.name.clone(),
        fields,
        groups,
        flags,
        meta_fields,
    }
}

/// Resolve all field names for a flag from its `display_groups`, explicit
/// `fields`, and glob `patterns`, deduplicating while preserving order.
fn resolve_fields(fg: &FieldGroup, all_fields: &[FieldDef]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<String> = Vec::new();

    let mut add = |name: String| {
        if seen.insert(name.clone()) {
            result.push(name);
        }
    };

    // 1. Fields whose display_group matches one of the listed groups.
    for group in &fg.display_groups {
        for field in all_fields {
            if field.display_group.as_deref() == Some(group.as_str()) {
                add(field.name.clone());
            }
        }
    }

    // 2. Explicit field names — also match against synonyms so that the
    //    deprecated alias resolves to the canonical field name.
    for name in &fg.fields {
        // Try exact canonical match first.
        let canonical_exact = all_fields.iter().find(|f| &f.name == name);
        if canonical_exact.is_some() {
            add(name.clone());
            continue;
        }
        // Fall back to synonym lookup.
        if let Some(field) = all_fields.iter().find(|f| f.synonyms.contains(name)) {
            add(field.name.clone());
        } else {
            // Unknown name — include as-is so the user gets a clear compile
            // error rather than a silent omission.
            add(name.clone());
        }
    }

    // 3. Glob patterns: `prefix*`, `*suffix`, `*contains*`.
    //    Also match against synonym names so patterns like `ebp_metric_*`
    //    resolve to the canonical `ebp_standard_*` field.
    for pattern in &fg.patterns {
        for field in all_fields {
            if matches_pattern(&field.name, pattern)
                || field.synonyms.iter().any(|s| matches_pattern(s, pattern))
            {
                add(field.name.clone());
            }
        }
    }

    result
}

/// Return `true` when `name` matches the glob `pattern`.
///
/// Supported forms: `prefix*`, `*suffix`, `*contains*`.  A pattern with no
/// wildcard matches exactly.
fn matches_pattern(name: &str, pattern: &str) -> bool {
    match (pattern.starts_with('*'), pattern.ends_with('*')) {
        (false, true) => name.starts_with(pattern.trim_end_matches('*')),
        (true, false) => name.ends_with(pattern.trim_start_matches('*')),
        (true, true) => {
            let inner = pattern.trim_matches('*');
            inner.is_empty() || name.contains(inner)
        }
        (false, false) => name == pattern,
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
fn template_name_to_dest(template_name: &str, language: &str, sdk_name: &str) -> String {
    match language {
        "rust" => match template_name {
            "cli_meta.rs" => "src/cli_meta.rs".to_string(),
            "indexes.rs" => "src/generated/indexes.rs".to_string(),
            "fields.rs" => "src/generated/fields.rs".to_string(),
            "groups.rs" => "src/generated/groups.rs".to_string(),
            "cli_flags.rs" => "src/generated/cli_flags.rs".to_string(),
            "client.rs" => "src/generated/client.rs".to_string(),
            "output.rs" => "src/generated/output.rs".to_string(),
            "field_meta.rs" => "src/generated/field_meta.rs".to_string(),
            "sdk.rs" => "src/generated/sdk.rs".to_string(),
            "lib.rs" => "src/lib.rs".to_string(),
            "generated_mod.rs" => "src/generated/mod.rs".to_string(),
            "main.rs" => "src/main.rs".to_string(),
            other => format!("src/generated/{}", other),
        },
        "python" => match template_name {
            "query.py" => format!("python/{sdk_name}/query.py"),
            "site_cli.pyi" => format!("python/{sdk_name}/{sdk_name}.pyi"),
            other => format!("python/{sdk_name}/{}", other),
        },
        "shared" => match template_name {
            "autoupdate.yml" => ".github/workflows/autoupdate.yml".to_string(),
            other => other.to_string(),
        },
        // "r" => {
        //     // Phase 2
        //     format!("r/{sdk_name}/{template_name}")
        // }
        _ => template_name.to_string(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{
        CompatConfig, FieldGroup, IndexDef, IndexOptions, SiteConfig, ValidationConfig,
    };
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
            archive: false,
            validation: ValidationConfig::default(),
            sdk_name: None,
            enabled_sdks: vec!["python".to_string()],
            ui_base: None,
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
                synonyms: vec![],
                processed_type: Some("long".to_string()),
                traverse_direction: Some("down".to_string()),
                summary: vec!["min".to_string(), "max".to_string()],
            },
            FieldDef {
                name: "assembly_level".to_string(),
                display_group: Some("assembly".to_string()),
                display_name: Some("Assembly level".to_string()),
                description: None,
                field_type: Some("keyword".to_string()),
                constraint: None,
                display_level: Some(1),
                synonyms: vec![],
                processed_type: None,
                traverse_direction: None,
                summary: vec![],
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
                            short: None,
                            display_groups: vec!["genome_size".to_string()],
                            fields: vec![],
                            patterns: vec![],
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
        assert_eq!(
            template_name_to_dest("cli_meta.rs", "rust", "goat_sdk"),
            "src/cli_meta.rs"
        );
    }

    #[test]
    fn template_name_to_dest_maps_generated_files() {
        assert_eq!(
            template_name_to_dest("fields.rs", "rust", "goat_sdk"),
            "src/generated/fields.rs"
        );
    }

    #[test]
    fn template_name_to_dest_maps_autoupdate_workflow() {
        assert_eq!(
            template_name_to_dest("autoupdate.yml", "shared", "goat_sdk"),
            ".github/workflows/autoupdate.yml"
        );
    }

    #[test]
    fn template_name_to_dest_maps_preview_md() {
        assert_eq!(
            template_name_to_dest("PREVIEW.md", "shared", "goat_sdk"),
            "PREVIEW.md"
        );
    }

    #[test]
    fn template_name_to_dest_maps_query_py() {
        assert_eq!(
            template_name_to_dest("query.py", "python", "goat_sdk"),
            "python/goat_sdk/query.py"
        );
    }

    #[test]
    fn template_name_to_dest_maps_lib_rs() {
        assert_eq!(
            template_name_to_dest("lib.rs", "rust", "goat_sdk"),
            "src/lib.rs"
        );
    }

    #[test]
    fn template_name_to_dest_maps_sdk_rs() {
        assert_eq!(
            template_name_to_dest("sdk.rs", "rust", "goat_sdk"),
            "src/generated/sdk.rs"
        );
    }

    #[test]
    fn template_name_to_dest_maps_site_cli_pyi() {
        assert_eq!(
            template_name_to_dest("site_cli.pyi", "python", "goat_sdk"),
            "python/goat_sdk/goat_sdk.pyi"
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

        // Verify all languages are present.
        assert!(rendered.contains_key("rust"), "rust language missing");
        assert!(rendered.contains_key("shared"), "shared language missing");
        assert!(rendered.contains_key("python"), "python language missing");

        // Check rust templates.
        let rust_files = &rendered["rust"];
        assert!(
            rust_files.contains_key("src/cli_meta.rs"),
            "src/cli_meta.rs missing"
        );
        assert!(
            rust_files.contains_key("src/generated/fields.rs"),
            "src/generated/fields.rs missing"
        );
        assert!(
            rust_files.contains_key("src/generated/mod.rs"),
            "src/generated/mod.rs missing"
        );
        assert!(rust_files.contains_key("src/lib.rs"), "src/lib.rs missing");
        assert!(
            rust_files.contains_key("src/generated/sdk.rs"),
            "src/generated/sdk.rs missing"
        );
        assert!(
            rust_files.contains_key("src/generated/field_meta.rs"),
            "src/generated/field_meta.rs missing"
        );

        // Check shared templates.
        let shared_files = &rendered["shared"];
        assert!(
            shared_files.contains_key(".github/workflows/autoupdate.yml"),
            ".github/workflows/autoupdate.yml missing"
        );

        // Check python templates.
        let python_files = &rendered["python"];
        assert!(
            python_files.contains_key("python/testsite_sdk/query.py"),
            "python/testsite_sdk/query.py missing"
        );
        assert!(
            python_files.contains_key("python/testsite_sdk/testsite_sdk.pyi"),
            "python/testsite_sdk/testsite_sdk.pyi missing"
        );

        // Spot-check rendered content of the new templates.
        let lib_rs = rust_files.get("src/lib.rs").unwrap();
        assert!(
            lib_rs.contains("testsite_sdk"),
            "lib.rs missing PyO3 module name"
        );
        assert!(
            lib_rs.contains("sdk::build_url"),
            "lib.rs missing build_url registration"
        );

        let sdk_rs = rust_files.get("src/generated/sdk.rs").unwrap();
        assert!(
            sdk_rs.contains("API_BASE_URL"),
            "sdk.rs missing API_BASE_URL"
        );
        assert!(
            sdk_rs.contains("\"taxon\""),
            "sdk.rs missing taxon index arm"
        );

        let query_py = python_files.get("python/testsite_sdk/query.py").unwrap();
        assert!(
            query_py.contains("import testsite_sdk as _ext"),
            "query.py missing extension import"
        );

        let pyi = python_files
            .get("python/testsite_sdk/testsite_sdk.pyi")
            .unwrap();
        assert!(
            pyi.contains("class Validator"),
            "pyi stub missing Validator class"
        );
    }

    #[test]
    fn matches_pattern_prefix_wildcard() {
        assert!(matches_pattern("busco_completeness", "busco_*"));
        assert!(!matches_pattern("genome_size", "busco_*"));
    }

    #[test]
    fn matches_pattern_suffix_wildcard() {
        assert!(matches_pattern("genome_size_kmer", "*_kmer"));
        assert!(!matches_pattern("genome_size", "*_kmer"));
    }

    #[test]
    fn matches_pattern_contains_wildcard() {
        assert!(matches_pattern("c_value_method", "*value*"));
        assert!(!matches_pattern("genome_size", "*value*"));
    }

    #[test]
    fn matches_pattern_exact() {
        assert!(matches_pattern("genome_size", "genome_size"));
        assert!(!matches_pattern("genome_size_draft", "genome_size"));
    }

    #[test]
    fn resolve_fields_deduplicates_across_sources() {
        use crate::core::config::FieldGroup;
        let fields = sample_fields(); // genome_size (group: genome_size), assembly_level (group: assembly)
        let fg = FieldGroup {
            flag: "test".to_string(),
            description: "test".to_string(),
            short: None,
            display_groups: vec!["genome_size".to_string()],
            fields: vec!["genome_size".to_string()], // duplicate of display_group result
            patterns: vec!["genome_*".to_string()],  // also matches genome_size
            compat_aliases: vec![],
        };
        let resolved = resolve_fields(&fg, &fields);
        assert_eq!(resolved, vec!["genome_size"]);
    }

    #[test]
    fn resolve_fields_resolves_synonym_in_fields_list() {
        use crate::core::config::FieldGroup;
        let fields = vec![FieldDef {
            name: "ebp_standard_date".to_string(),
            display_group: Some("assembly".to_string()),
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec!["ebp_metric_date".to_string()],
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        }];
        let fg = FieldGroup {
            flag: "ebp".to_string(),
            description: "EBP fields".to_string(),
            short: None,
            display_groups: vec![],
            fields: vec!["ebp_metric_date".to_string()], // deprecated alias
            patterns: vec![],
            compat_aliases: vec![],
        };
        let resolved = resolve_fields(&fg, &fields);
        assert_eq!(resolved, vec!["ebp_standard_date"]);
    }

    #[test]
    fn resolve_fields_resolves_synonym_via_pattern() {
        use crate::core::config::FieldGroup;
        let fields = vec![FieldDef {
            name: "ebp_standard_date".to_string(),
            display_group: Some("assembly".to_string()),
            display_name: None,
            description: None,
            field_type: None,
            constraint: None,
            display_level: None,
            synonyms: vec!["ebp_metric_date".to_string()],
            processed_type: None,
            traverse_direction: None,
            summary: vec![],
        }];
        let fg = FieldGroup {
            flag: "ebp".to_string(),
            description: "EBP fields".to_string(),
            short: None,
            display_groups: vec![],
            fields: vec![],
            patterns: vec!["ebp_metric_*".to_string()], // matches the synonym
            compat_aliases: vec![],
        };
        let resolved = resolve_fields(&fg, &fields);
        assert_eq!(resolved, vec!["ebp_standard_date"]);
    }
}
