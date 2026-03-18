# Multi-Language SDK Support Plan

**Status:** Planned (not yet implemented)
**Date:** 18 March 2026
**Target Languages:** R (Phase 2), JavaScript (Phase 3), Go (Phase 3 optional)

---

## Overview

This document outlines a phased approach to extend the `cli-generator` beyond Rust/Python to support R, JavaScript, Go, and other languages. The architecture is language-agnostic at the core level, making expansion feasible within a single monorepo.

### Key Principles

- **Core is reusable:** Query building, config system, field fetching, validation rules all live in pure Rust with no language-specific dependencies.
- **Templates + paths vary:** Only Tera templates and output directory structures differ per language.
- **Single version:** All SDKs version-lock to the generator (e.g., goat-cli-generator 0.1.0 generates goat_sdk 0.1.0 for all languages).
- **Monorepo:** Keep everything together. Build complexity is manageable with parallel CI jobs per language.
- **Centrally maintained:** Generated packages are version-controlled and published from this repo.

---

## Architecture Assessment

### Language-Agnostic Core

These components require **no changes** and can serve any language:

- **Query building** (`src/core/query/`): `SearchQuery`, `QueryParams`, URL encoding
- **Field fetching** (`src/core/fetch.rs`): `FieldDef` structs, API caching (24h)
- **Config system** (`src/core/config.rs`): `SiteConfig`, `CliOptionsConfig` (pure data, uses only serde/anyhow/chrono)
- **Validation rules** (emitted into `field_meta.rs.tera`): Operator validation, enum constraints, summary modifiers
- **Template context design** (`CodeGenerator::build_context()`): Generic structure (site metadata, field defs, CLI flags)

### Language-Specific Components

Only these parts vary per language:

- **Template set:** Currently 18 templates split between Rust (11), Python (2), CI (3), shared (2)
- **Output file paths:** E.g., Python → `python/{sdk_name}/`, R → `r/{sdk_name}/`
- **Post-processing:** Python uses `black`/`isort`, R uses `styler`, JS uses `prettier`
- **Compatibility flags:** Rust-only (`goat_cli_compat` for clap aliases)

### Tera Templating Capability

- **Features used:** Loops, filters, conditionals, filters
- **Strengths:** Unidirectional data flow, compile-time safety, outputs any text syntax
- **Limitation:** No runtime variation beyond what's in config

---

## Phased Implementation Plan

### Phase 1: Infrastructure Foundation

**Goal:** Establish language-agnostic template loading and rendering pipeline.

#### 1.1 Reorganize Templates by Language Family

Create subdirectories in `templates/`:

```
templates/
  rust/
    cli_meta.rs.tera
    main.rs.tera
    generated_mod.rs.tera
    fields.rs.tera
    groups.rs.tera
    cli_flags.rs.tera
    client.rs.tera
    output.rs.tera
    sdk.rs.tera
    lib.rs.tera
    field_meta.rs.tera
    indexes.rs.tera
  python/
    query.py.tera
    site_cli.pyi.tera
  shared/
    GETTING_STARTED.md.tera
    PREVIEW.md.tera
    autoupdate.yml.tera
    ci.yml.tera
  r/              # Created in Phase 2
    (new templates)
  js/             # Created in Phase 3
    (new templates)
```

#### 1.2 Update `make_tera()` in [src/core/codegen.rs](src/core/codegen.rs#L21)

- Group template registration by language family
- Maintain same `add_raw_template()` calls but organize with comments
- Add language constants for future CLI/config options:
  ```rust
  const SUPPORTED_LANGUAGES: &[&str] = &["rust", "python"];  // Extend later
  ```

#### 1.3 Extend Config Schema — Add `enabled_sdks` to `SiteConfig`

**File:** [src/core/config.rs](src/core/config.rs)

Add optional field to `SiteConfig`:

```rust
/// Which SDK languages to generate (defaults to ["python"] for backward compat).
#[serde(default = "default_enabled_sdks")]
pub enabled_sdks: Vec<String>,

fn default_enabled_sdks() -> Vec<String> {
    vec!["python".to_string()]
}
```

Update YAML config example (e.g., `sites/goat.yaml`):

```yaml
enabled_sdks:
  - python
  - r # Enable when Phase 2 complete
```

#### 1.4 Make `template_name_to_dest()` Language-Aware

**File:** [src/core/codegen.rs](src/core/codegen.rs#L436)

Refactor from hardcoded matching to language-parameterized:

```rust
/// Map a template name to its destination path, per language.
fn template_name_to_dest(
    template_name: &str,
    language: &str,
    sdk_name: &str,
) -> String {
    match language {
        "rust" => {
            match template_name {
                "cli_meta.rs" => "src/cli_meta.rs".to_string(),
                // ... rest of rust routing
                _ => format!("src/generated/{template_name}"),
            }
        }
        "python" => {
            match template_name {
                "query.py" => format!("python/{sdk_name}/query.py"),
                "site_cli.pyi" => format!("python/{sdk_name}/{sdk_name}.pyi"),
                _ => format!("python/{sdk_name}/{template_name}"),
            }
        }
        "r" => {
            match template_name {
                "query_builder.R" => format!("r/{sdk_name}/R/query.R"),
                "validation.R" => format!("r/{sdk_name}/R/validation.R"),
                "DESCRIPTION" => format!("r/{sdk_name}/DESCRIPTION"),
                "NAMESPACE" => format!("r/{sdk_name}/NAMESPACE"),
                _ => format!("r/{sdk_name}/{template_name}"),
            }
        }
        _ => format!("generated/{language}/{sdk_name}/{template_name}"),
    }
}
```

#### 1.5 Refactor `CodeGenerator::render_all()` to Loop Over Languages

**File:** [src/core/codegen.rs](src/core/codegen.rs#L217)

Change return type and rendering loop:

```rust
/// Render all templates for all enabled languages, returning nested HashMap.
pub fn render_all(
    &self,
    site: &SiteConfig,
    options: &CliOptionsConfig,
    fields_by_index: &HashMap<String, Vec<FieldDef>>,
) -> Result<HashMap<String, HashMap<String, String>>> {
    let mut all_langs = HashMap::new();

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
    // Determine which templates to render per language
    let template_names = match language {
        "rust" => vec![
            "cli_meta.rs", "indexes.rs", "fields.rs", "groups.rs", "cli_flags.rs",
            "client.rs", "output.rs", "field_meta.rs", "sdk.rs", "lib.rs", "generated_mod.rs",
            "main.rs",
        ],
        "python" => vec!["query.py", "site_cli.pyi"],
        "r" => vec![ /* Phase 2 */ ],
        _ => vec![],
    };

    let ctx = self.build_context(site, options, fields_by_index);
    let mut out = HashMap::new();

    for template_name in template_names {
        let rendered = self.tera.render(template_name, &ctx)?;
        let dest_path = template_name_to_dest(template_name, language, &site.resolved_sdk_name());
        out.insert(dest_path, rendered);
    }

    Ok(out)
}
```

**Note:** For backward compatibility, update callers in `commands/new.rs` and `commands/update.rs` to iterate over returned languages.

#### 1.6 Add Shared Template Files

Move shared templates (not language-specific) to `templates/shared/`:

- `GETTING_STARTED.md.tera`
- `PREVIEW.md.tera`
- `autoupdate.yml.tera`
- `ci.yml.tera`

These are rendered **once per site** (not per language). Update `render_all()` to include them in all language maps, or render them separately.

#### 1.7 Unit Tests for Phase 1

**File:** [src/core/codegen.rs](src/core/codegen.rs#L600)

Add tests:

```rust
#[test]
fn render_all_returns_nested_map_per_language() {
    let gen = CodeGenerator::new().unwrap();
    let site = sample_site();  // with enabled_sdks: ["python"]
    let options = sample_options();
    let mut fields_by_index = HashMap::new();
    fields_by_index.insert("taxon".to_string(), sample_fields());

    let all_langs = gen.render_all(&site, &options, &fields_by_index).unwrap();

    assert!(all_langs.contains_key("python"));
    assert!(all_langs.get("python").unwrap().contains_key("python/testsite_sdk/query.py"));
}

#[test]
fn template_name_to_dest_routes_per_language() {
    assert_eq!(
        template_name_to_dest("query.py", "python", "goat_sdk"),
        "python/goat_sdk/query.py"
    );

    // Placeholder for R (Phase 2):
    // assert_eq!(
    //     template_name_to_dest("query_builder.R", "r", "goat_sdk"),
    //     "r/goat_sdk/R/query.R"
    // );
}

#[test]
fn config_parses_enabled_sdks() {
    let yaml = r#"
name: testsite
display_name: Test Site
api_base: https://example.com/api
api_version: v2
indexes:
  - name: taxon
enabled_sdks:
  - python
"#;
    let config: SiteConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.enabled_sdks, vec!["python"]);
}
```

#### 1.8 Update Command Callers

**Files:** [src/commands/new.rs](src/commands/new.rs), [src/commands/update.rs](src/commands/update.rs)

Update `write_generated_files()` to iterate over languages:

```rust
fn write_generated_files(
    rendered_by_lang: &HashMap<String, HashMap<String, String>>,
    output_dir: &Path,
) -> Result<()> {
    for (language, rendered) in rendered_by_lang {
        // Apply language-specific post-processing
        let processed = postprocess(rendered, language)?;
        for (file_path, content) in processed {
            let full_path = output_dir.join(&file_path);
            fs::create_dir_all(full_path.parent().unwrap())?;
            fs::write(&full_path, content)?;
        }
    }
    Ok(())
}
```

#### 1.9 Extend CI to Report Language-Specific Status

**File:** `.github/workflows/ci.yml`

Update Rust tests to verify Phase 1 refactoring:

```yaml
- name: Test Phase 1 infrastructure (multi-lang rendering)
  run: cargo test codegen::tests::render_all_returns_nested_map_per_language
```

---

### Phase 2: R SDK Implementation

**Goal:** Prove the multi-language pattern works. Deliver R SDK with full feature parity to Python.

#### 2.1 Design R Template Set

Create templates in `templates/r/`:

1. **query_builder.R.tera** — S4 class or R6 reference class matching Python QueryBuilder
   - Methods: `add_filter()`, `add_sort()`, `restrict()`, `add_summary()`, `build()`
   - Properties: filters, sorts, flags, selections, traversal
   - Docstrings via roxygen2 comments

2. **validation.R.tera** — Field metadata and validation logic
   - Data frame of canonical field names, types, synonyms, enum values
   - Functions: `validate_field()`, `get_enum_values()`, `get_summary_modifiers()`
   - Synonym resolution (deprecated aliases → canonical names)

3. **DESCRIPTION.tera** — R package metadata
   - Package name: `{sdk_name}` (e.g., `goat_sdk`)
   - Version: match generator version (e.g., `0.1.0`)
   - Title: "QueryBuilder for {site_display_name}"
   - Authors: auto-populate from config or defaults
   - Imports: `httr2` (HTTP), `jsonlite` (JSON)

4. **NAMESPACE.tera** — R package exports
   - Export S4 class `QueryBuilder`
   - Export helper functions (if any)
   - Import namespace declarations

5. **README.md.tera** — Basic usage guide (similar to Python's query.py docstring)

#### 2.2 Implement R Query Builder Class

**Design notes:**

- Use **R6 reference class** over S4 for simplicity and Python parity
- Methods return `self` for method chaining: `qb$add_filter()$add_sort()$build()`
- `build()` method returns URL string (via `paste0()`) matching Rust `build_url()`
- Field validation at runtime (prevent typos, unsupported operators)
- Support field synonyms (deprecated aliases resolve to canonical)

#### 2.3 Add R to `make_tera()` and `render_for_language()`

**File:** [src/core/codegen.rs](src/core/codegen.rs#L21)

```rust
fn make_tera() -> Result<Tera> {
    // ... existing Rust/Python templates ...

    // Phase 2: R templates
    tera.add_raw_template(
        "query_builder.R",
        include_str!("../../templates/r/query_builder.R.tera"),
    )
    .context("loading query_builder.R template")?;
    tera.add_raw_template(
        "validation.R",
        include_str!("../../templates/r/validation.R.tera"),
    )
    .context("loading validation.R template")?;
    tera.add_raw_template(
        "DESCRIPTION",
        include_str!("../../templates/r/DESCRIPTION.tera"),
    )
    .context("loading DESCRIPTION template")?;
    // ... etc
    Ok(tera)
}
```

Update `render_for_language()`:

```rust
"r" => vec![
    "query_builder.R", "validation.R", "DESCRIPTION", "NAMESPACE", "README.md",
],
```

#### 2.4 Implement R Code Generation Tests

**File:** [tests/python/test_core.py](tests/python/test_core.py) (or `tests/r/test_codegen.r`)

1. **Template rendering test:** Render R templates for test site, verify output files exist
2. **Syntax validation:** Parse generated R code (language-agnostic: check for balanced parens/quotes, roxygen markers)
3. **QueryBuilder interface:** Verify generated R class has required methods (`add_filter`, `build`, etc.)
4. **Field metadata:** Verify synonym resolution works in generated validation.R
5. **Version parity:** Assert DESCRIPTION version matches config metadata

Example (Python-based):

```python
def test_render_r_templates():
    gen = CodeGenerator()
    site = sample_site()
    site.enabled_sdks = ["r"]
    rendered = gen.render_all(site, sample_options(), sample_fields_by_index())

    assert "r" in rendered
    assert "r/testsite_sdk/R/query.R" in rendered["r"]
    assert "r/testsite_sdk/DESCRIPTION" in rendered["r"]

    r_code = rendered["r"]["r/testsite_sdk/R/query.R"]
    assert "setRefClass" in r_code or "R6Class" in r_code
    assert "add_filter" in r_code
    assert "build" in r_code
```

#### 2.5 Extend CI to Test R

**File:** `.github/workflows/ci.yml`

Add parallel job:

```yaml
test-r:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: r-lib/actions/setup-r@v2
      with:
        r-version: "latest"
    - name: Install dependencies
      run: |
        install.packages(c("devtools", "lintr"))
        devtools::install_dev_deps()
      shell: Rscript {0}
    - name: Lint R code
      run: lintr::lint_package()
      shell: Rscript {0}
    - name: Run R tests
      run: devtools::test()
      shell: Rscript {0}
```

#### 2.6 Document R SDK in PREVIEW

Update [templates/PREVIEW.md.tera](templates/PREVIEW.md.tera) to include:

- Quick-start code snippet for R QueryBuilder
- Feature parity matrix vs Python
- Link to generated R package README

#### 2.7 Verify Backward Compatibility

Run full test suite:

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
pytest tests/python/ -v
```

Ensure Python SDK generation unaffected. Regenerate GOAT/BOAT CLIs; output should match pre-Phase-1 commits.

---

### Phase 3: Additional Languages (Future)

Once R is complete and tested, the pattern is established. Subsequent languages follow the same steps:

#### 3.1 JavaScript/TypeScript SDK

**Templates:** `query_builder.ts`, `validation.ts`, `package.json`
**Formatter:** `prettier`
**Test framework:** Jest
**Effort:** ~80% of Phase 2 (templates + tests)

#### 3.2 Go SDK

**Templates:** `query_builder.go`, `validation.go`, `go.mod`
**Formatter:** `gofmt`
**Test framework:** `go test`

---

## Implementation Checklist

### Phase 1: Infrastructure

- [ ] Reorganize templates into language subdirectories
- [ ] Update `make_tera()` with new template paths
- [ ] Add `enabled_sdks` field to `SiteConfig`
- [ ] Refactor `template_name_to_dest()` to be language-aware
- [ ] Refactor `CodeGenerator::render_all()` to loop over languages
- [ ] Handle shared templates (GETTING_STARTED, etc.)
- [ ] Update `commands/new.rs` and `commands/update.rs` to consume nested HashMap
- [ ] Add Phase 1 unit tests
- [ ] Run full test suite; verify backward compat with Python

### Phase 2: R SDK

- [ ] Write R template files (5 templates)
- [ ] Add R templates to `make_tera()`
- [ ] Add R branch to `render_for_language()`
- [ ] Add R tests (template rendering, syntax, interface)
- [ ] Extend CI with R test job
- [ ] Update PREVIEW.md with R quick-start
- [ ] Manually test: `goat cli-generator new goat --output-dir ../goat-r --enabled-sdks r`
- [ ] Verify generated R package installs and QueryBuilder works

### Phase 3: JavaScript (whenever)

- [ ] Repeat Phase 2 steps for JS

### Phase 3: Go (whenever)

- [ ] Repeat Phase 2 steps for Go

---

## Key Decisions

1. **Versioning:** All SDKs locked to generator version. Single changelog.
2. **Post-processing:** Mandatory in both `new` and `update` commands (Python: `black`/`isort`, R: `styler`, JS: `prettier`).
3. **Field metadata:** Serialized to JSON alongside generated files (Option A). R and JS templates read same JSON.
4. **Monorepo:** Expand `cli-generator` to include R, JS, Go. CI runs language tests in parallel.
5. **Backward compatibility:** Phase 1 refactoring maintains Python SDK generation without changes to `sites/` configs.

---

## Risk Mitigation

1. **Phase 1 refactoring is breaking:** Mitigate with extensive backward-compat tests. Regenerate GOAT/BOAT CLIs; diffs should be empty.
2. **R test infrastructure unfamiliar:** Evaluate R CI action (`r-lib/actions/setup-r`) early. Fallback: run `R CMD check` if devtools is overkill.
3. **Field metadata drift:** Enforce schema via tests. Python, R, JS must all read and validate same `TemplateFieldMeta` structure.
4. **Build time bloat:** Monitor CI time. If >10min for all languages, investigate parallelization.

---

## Future Considerations

- **Runtime metadata API:** Expose field definitions at SDK runtime (R `list()`, JS object) for advanced use cases?
- **SDK package managers:** CRAN (R), PyPI (Python), npm (JS) registration and publishing automation?
- **Constraint solver:** Advanced query validation (e.g., "field X cannot be combined with field Y")?
- **Performance:** Benchmark URL building across languages; optimize if needed.

---

## Files Modified (Phase 1 & 2)

**Core:**

- `src/core/codegen.rs` — Template loading, rendering loop, destination routing
- `src/core/config.rs` — Add `enabled_sdks` to `SiteConfig`

**Commands:**

- `src/commands/new.rs` — Update to handle language-nested rendered output
- `src/commands/update.rs` — Same

**Templates:**

- Move existing to `templates/rust/` and `templates/python/`
- Add `templates/r/` (Phase 2)
- Add `templates/shared/` for GETTING_STARTED, PREVIEW, etc.

**Tests:**

- `src/core/codegen.rs` — Add Phase 1 tests
- `tests/python/test_core.py` — Add R rendering tests (Phase 2)
- `.github/workflows/ci.yml` — Add R test job (Phase 2)

**Documentation:**

- `docs/multi-language-sdk-plan.md` — This file
- `templates/PREVIEW.md.tera` — R quick-start (Phase 2)

---

## References

- [src/core/codegen.rs](src/core/codegen.rs) — Current template system
- [src/core/config.rs](src/core/config.rs) — Config schema
- [src/commands/new.rs](src/commands/new.rs) — File writing logic
- [tests/python/test_core.py](tests/python/test_core.py) — Test patterns
- [.github/workflows/ci.yml](.github/workflows/ci.yml) — CI structure
