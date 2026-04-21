# Extension Guide

Customize and extend cli-generator for your project's needs using the proven pattern: **Rust-first, then expose to Python, R, JavaScript, and documentation**.

**Status**: 📋 PLANNING (comprehensive v1 created 2026-04-21)
**Audience**: SDK developers, template maintainers, and LLM agents
**Effort per task**: 2–4 hours depending on complexity
**Key principle**: Keep logic in Rust core; language templates are wiring only

---

## Overview: The Extension Pattern

All extensions follow the same workflow:

```
1. Rust Core      → Add logic to src/core/ + unit tests
2. Python FFI     → Expose via PyO3 in src/lib.rs + .pyi stub
3. R/JS Templates → Add method to templates/ (R/JS wiring, not logic)
4. Documentation  → Add docstring + example
5. Integration    → Cross-language tests verify parity
```

**Why this order?**

- **Rust core first**: Single source of truth; tested in isolation
- **Python next**: PyO3 boundary is the narrowest; catches type issues early
- **R/JS templates**: Simple wiring; logic reuses Rust via templates
- **Docs + tests last**: Verify behavior across all languages simultaneously

**Avoid bloat**: Don't duplicate logic in templates. If you're adding the same logic to Python, R, and JS, it belongs in Rust.

---

## The Five Extension Tasks

### Task 1: Add a New Query Parameter

**Example**: Add `set_field_range(min, max)` to filter numeric fields

#### 1.1 Add Rust core method (src/core/query/mod.rs)

```rust
/// Filter a numeric field by range [min, max].
pub fn set_field_range(&mut self, field: String, min: f64, max: f64) -> &mut Self {
    self.field_ranges.insert(field, (min, max));
    self
}
```

**Test it immediately** (same file):

```rust
#[test]
fn field_range_serializes() {
    let mut q = SearchQuery::default();
    q.set_field_range("genome_size".to_string(), 1_000_000.0, 5_000_000_000.0);

    let yaml = q.to_yaml().unwrap();
    assert!(yaml.contains("genome_size") && yaml.contains("1000000"));
}
```

**Verify Rust compiles**:

```bash
cargo test --lib  # Should pass
cargo clippy -- -D warnings  # Should pass
```

#### 1.2 Expose via PyO3 (src/lib.rs)

```rust
#[pymethods]
impl QueryBuilder {
    /// Filter a numeric field by range [min, max].
    pub fn set_field_range(&mut self, field: String, min: f64, max: f64) -> PyResult<PyObject> {
        self.query.set_field_range(field, min, max);
        Ok(self.into())  // Return self for chaining
    }
}
```

Add to `python/cli_generator/cli_generator.pyi` stub:

```python
class QueryBuilder:
    def set_field_range(self, field: str, min: float, max: float) -> QueryBuilder: ...
```

**Verify PyO3 compiles + Python import works**:

```bash
maturin develop --features extension-module
python3 -c "from cli_generator import QueryBuilder; print('✓ Import works')"
```

#### 1.3 Wire in Python template (templates/python/query.py.tera)

```python
def set_field_range(self, field: str, min: float, max: float) -> "QueryBuilder":
    """Filter a numeric field by range [min, max]."""
    # Delegate to Rust core
    self._ffi_qb.set_field_range(field, min, max)
    return self
```

#### 1.4 Wire in R template (templates/r/query.R.tera)

```r
#' @param field Field name (string)
#' @param min Minimum value (numeric)
#' @param max Maximum value (numeric)
set_field_range = function(field, min, max) {
  private$query$set_field_range(field, min, max)
  invisible(self)
}
```

#### 1.5 Wire in JavaScript template (templates/js/query.js.tera)

```javascript
/**
 * Filter a numeric field by range [min, max].
 * @param {string} field - Field name
 * @param {number} min - Minimum value
 * @param {number} max - Maximum value
 * @returns {QueryBuilder} this (for chaining)
 */
setFieldRange(field, min, max) {
  this.query.setFieldRange(field, min, max);
  return this;
}
```

#### 1.6 Add cross-language test (tests/python/test_sdk_parity.py)

```python
def test_field_range_parameter_parity(self):
    """Verify set_field_range works identically across Python, R, JS."""
    # Python
    py_qb = QueryBuilder("taxon")
    py_qb.set_field_range("genome_size", 1_000_000, 5_000_000_000)
    py_url = py_qb.to_url()

    # R (skip if R not available)
    pytest.skip("R testing deferred; verified manually")

    # JS (via fixtures or integration test)
    assert "genome_size" in py_url
    assert "1000000" in py_url
```

#### 1.7 Update docstring (GETTING_STARTED.md or API reference)

````markdown
### Numeric Range Filtering

Filter fields by numeric range:

```python
from cli_generator import QueryBuilder

qb = QueryBuilder("taxon")
qb.set_field_range("genome_size", 1_000_000, 5_000_000_000)  # 1 MB to 5 GB
qb.add_field("genome_size", "sequencing_status")
print(qb.to_url())
```
````

````

---

### Task 2: Add Support for a New Language (e.g., Go, Perl)

**Example**: Generate Go SDK

#### 2.1 Understand the current structure

Go SDK generation requires:
- `templates/go/query.go.tera` — QueryBuilder struct + methods
- `templates/go/client.go.tera` — HTTP client wrapper
- `templates/go/go.mod.tera` — Module definition
- Logic reuses Rust via cgo (C FFI) or WASM

**Decision**: Use WASM (like JavaScript) or cgo (like Python)?
- **WASM**: Works everywhere, good for data science notebooks
- **cgo**: Direct Rust call, better performance, more setup required

→ Assume **WASM** for Go (simpler, consistent with JS approach)

#### 2.2 Create Go template files

**File**: `templates/go/query.go.tera`

```go
package {{project_snake_case}}

import (
  "fmt"
)

// QueryBuilder constructs search queries for the {{api_base}} API.
type QueryBuilder struct {
  index string
  query map[string]interface{}
}

// NewQueryBuilder creates a new query builder for the given index.
func NewQueryBuilder(index string) *QueryBuilder {
  return &QueryBuilder{
    index: index,
    query: make(map[string]interface{}),
  }
}

// SetTaxa sets the taxonomic constraint.
func (qb *QueryBuilder) SetTaxa(taxa []string, filterType string) *QueryBuilder {
  qb.query["taxon"] = map[string]interface{}{
    "name": taxa,
    "filter": filterType,
  }
  return qb
}

// ToURL generates a URL for this query (calls Rust via WASM).
func (qb *QueryBuilder) ToURL() (string, error) {
  // Call WASM module: wasm.ToURL(qb.query)
  return wasmModule.ToURL(qb.query)
}
````

#### 2.3 Update Rust WASM export (crates/genomehubs-query/src/lib.rs)

Ensure the WASM module exports `to_url`:

```rust
#[wasm_bindgen]
pub fn to_url(query: JsValue) -> Result<String, JsValue> {
    // Deserialize JS object → Rust SearchQuery
    // Call query.to_url()
    // Serialize result → JS string
}
```

#### 2.4 Add Go tests (tests/go/)

```go
package {{project_snake_case}}_test

import (
  "testing"
  pkg "{{package_path}}"
)

func TestSetTaxaBuildsCorrectly(t *testing.T) {
  qb := pkg.NewQueryBuilder("taxon")
  qb.SetTaxa([]string{"Mammalia"}, "tree")

  url, err := qb.ToURL()
  if err != nil {
    t.Fatalf("ToURL failed: %v", err)
  }

  if !strings.Contains(url, "tax_tree") {
    t.Errorf("Expected tax_tree in URL, got: %s", url)
  }
}
```

#### 2.5 Update documentation

Add Go installation + quick-start to GETTING_STARTED.md:

```markdown
### Go SDK

Install from GitHub releases:

\`\`\`bash
go get github.com/genomehubs/{{project}}-go@latest
\`\`\`

Quick start:

\`\`\`go
package main

import "github.com/genomehubs/{{project}}-go"

func main() {
qb := {{project}}.NewQueryBuilder("taxon")
qb.SetTaxa([]string{"Homo sapiens"}, "exact")
url, \_ := qb.ToURL()
println(url)
}
\`\`\`
```

---

### Task 3: Extend the Validator with Custom Rules

**Example**: Add a rule that prevents combining `taxon` + `assembly` filters

#### 3.1 Add validation logic in Rust (src/core/validation.rs)

```rust
/// Enforce that taxon and assembly filters cannot be combined.
pub fn validate_no_mixed_filters(query: &SearchQuery) -> Result<(), ValidationError> {
    let has_taxon = query.taxa.is_some();
    let has_assembly = query.assembly.is_some();

    if has_taxon && has_assembly {
        return Err(ValidationError {
            field: "query".to_string(),
            message: "Cannot combine taxon and assembly filters".to_string(),
            code: "MIXED_FILTERS".to_string(),
        });
    }
    Ok(())
}
```

**Add test**:

```rust
#[test]
fn mixed_filters_rejected() {
    let mut q = SearchQuery::default();
    q.set_taxa(vec!["Mammalia".to_string()], "tree".to_string());
    q.set_assembly(vec!["GCF_000001405.40".to_string()]);

    let result = validate_no_mixed_filters(&q);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "MIXED_FILTERS");
}
```

#### 3.2 Call validator from Rust CLI (src/main.rs)

```rust
fn validate_query(query: &SearchQuery) -> Result<(), Box<dyn Error>> {
    validation::validate_no_mixed_filters(query)?;
    validation::validate_required_fields(query)?;
    // ... other checks
    Ok(())
}

// In command handler
let query = /* build from args */;
validate_query(&query)?;  // Fails fast if invalid
```

**Verify on CLI**:

```bash
cargo run -- taxon search --taxon Mammalia --assembly GCF_000001405.40
# Error: Cannot combine taxon and assembly filters
```

#### 3.3 Expose via PyO3 (src/lib.rs)

```rust
#[pymethods]
impl QueryBuilder {
    /// Validate the query. Raises ValidationError if invalid.
    pub fn validate(&self, validation_level: Option<String>) -> PyResult<()> {
        let level = validation_level.unwrap_or_else(|| self.validation_level.clone());

        match level.as_str() {
            "full" => validation::validate_no_mixed_filters(&self.query)
                .map_err(|e| PyValueError::new_err(e.message))?,
            "basic" => { /* skip some checks */ },
            _ => {},
        }
        Ok(())
    }
}
```

Add to `.pyi`:

```python
class QueryBuilder:
    def validate(self, validation_level: str | None = None) -> None: ...
```

#### 3.4 Wire in templates (R, JS — Python already has PyO3)

**R** (`templates/r/query.R.tera`):

```r
validate = function(validation_level = NULL) {
  level <- validation_level %||% private$validation_level
  private$query$validate(level)
  invisible(self)
}
```

**JavaScript** (`templates/js/query.js.tera`):

```javascript
validate(validationLevel = null) {
  const level = validationLevel || this.validationLevel;
  this.query.validate(level);
  return this;
}
```

#### 3.5 Add cross-language tests (tests/python/test_sdk_parity.py)

```python
@pytest.mark.parametrize("lang", ["python", "r"])
def test_mixed_filters_validation(self, lang):
    """Verify mixed filter validation works across languages."""
    if lang == "python":
        qb = QueryBuilder("taxon")
        qb.set_taxa(["Mammalia"], "tree")
        qb.set_assembly(["GCF_000001405.40"])

        with pytest.raises(ValueError, match="Cannot combine"):
            qb.validate()

    elif lang == "r":
        pytest.skip("R validation tests deferred; verified manually")
```

---

### Task 4: Add a New Snippet Language

**Example**: Generate Julia query code snippets

#### 4.1 Add snippet template (templates/snippets/julia_snippet.tera)

```julia
# Julia snippet for {{project}} {{api_base}}

using HTTP, JSON3

function search_{{search_type}}(; kwargs...)
    """Build and execute a {{search_type}} search."""
    query = Dict(
        "query" => "{{search_type}}",
        {{#each fields}}
        "{{this}}" => nothing,
        {{/each}}
    )

    url = "{{api_base}}/{{url_path | safe}}"
    response = HTTP.get(url, query=query)
    return JSON3.read(String(response.body))
end

# Example:
results = search_{{search_type}}()
println(results)
```

#### 4.2 Register in Rust (src/core/snippet.rs)

```rust
impl SnippetGenerator {
    pub fn new() -> Self {
        let mut tera = Tera::default();

        // ... existing languages ...

        tera.add_raw_template(
            "julia_snippet",
            include_str!("../../templates/snippets/julia_snippet.tera"),
        ).expect("Failed to load Julia template");

        SnippetGenerator { tera }
    }
}
```

**Add test**:

```rust
#[test]
fn julia_snippet_generates() {
    let gen = SnippetGenerator::new();
    let snippet = gen.generate(
        "julia",
        "taxon",
        &["genome_size", "genome_url"],
    ).unwrap();

    assert!(snippet.contains("function search_taxon"));
    assert!(snippet.contains("genome_size"));
}
```

#### 4.3 Expose via Python (src/lib.rs already has snippet() function)

If not already exposed, add:

```rust
#[pymethods]
impl QueryBuilder {
    pub fn snippet(&self, language: &str) -> PyResult<String> {
        let gen = SnippetGenerator::new();
        gen.generate(language, &self.index, &self.fields)
            .map_err(|e| PyValueError::new_err(e.message))
    }
}
```

#### 4.4 Add to documentation (GETTING_STARTED.md)

````markdown
### Code Snippet Generation

Generate boilerplate code for your language:

```python
qb = QueryBuilder("taxon")
qb.add_field("genome_size")

julia_code = qb.snippet("julia")
print(julia_code)
# Output: Complete Julia function for this search
```
````

Supported languages: python, javascript, r, julia (new!)

````

---

### Task 5: Customize Generated Code Structure

**Example**: Add a per-site configuration that changes how SDKs organize modules

#### 5.1 Define per-site config (config/custom-site.yaml)

```yaml
name: custom-site
api_base: https://custom.example.com/api

customization:
  python:
    # Organize by data type instead of query type
    module_structure: "by_type"  # vs. default "by_query"

  javascript:
    # Include bundled CLI in package
    include_cli: true
````

#### 5.2 Update template logic (templates/python/query.py.tera)

```jinja2
{# If customization.python.module_structure == "by_type" #}
{% if module_structure == "by_type" %}
# Organization: modules ordered by data type (taxon, assembly, etc.)
from . import taxon, assembly, literature  # vs. ".search, .filter, .get"
{% else %}
# Organization: modules ordered by query type (search, filter, get)
from . import search, filter, get
{% endif %}
```

#### 5.3 Update Rust generation logic (src/commands/new.rs)

Read customization from config:

```rust
fn generate_python_package(config: &SiteConfig, output_dir: &Path) -> Result<()> {
    let module_structure = config
        .customization
        .as_ref()
        .and_then(|c| c.python.as_ref())
        .and_then(|p| p.module_structure.as_ref())
        .unwrap_or(&"by_query".to_string());

    let context = TemplateContext {
        module_structure: module_structure.clone(),
        // ...
    };

    // Render with customized context
}
```

#### 5.4 Test customization (tests/generated_goat_cli.rs)

```rust
#[test]
fn custom_site_python_structure_respects_module_organization() {
    // Generate with module_structure = "by_type"
    // Verify __init__.py imports taxon, assembly, literature
}
```

---

## Anti-Patterns: What NOT to Do

| ❌ Don't                                                       | ✅ Do                                                |
| -------------------------------------------------------------- | ---------------------------------------------------- |
| Add logic to Python template that duplicates Rust              | Keep logic in Rust; template calls Rust function     |
| Add separate validation in R and JavaScript                    | Add validation in Rust once; expose to all languages |
| Create language-specific snippet templates for identical logic | Create one Rust template; register for all languages |
| Commit generated code with custom hand-edits                   | Customize via config; regenerate cleanly             |
| Skip tests for R/JS "because they use Rust anyway"             | Test all languages; parity matters                   |
| Add TODO comments instead of implementing                      | See AGENTS.md: no speculative code                   |

---

## Checklist: New Extension Task

Before submitting, verify:

- [ ] **Rust code written + tested** (unit tests pass, clippy clean)
- [ ] **PyO3 exposures added** (`src/lib.rs` + `.pyi` stub)
- [ ] **All templates updated** (Python, R, JavaScript)
- [ ] **Logic stays in Rust** (templates are wiring only)
- [ ] **Cross-language tests added** (`tests/python/test_sdk_parity.py`)
- [ ] **Documentation updated** (GETTING_STARTED.md or API reference)
- [ ] **Agent log created** (if significant change; see AGENTS.md)
- [ ] **All language tests passing** (`pytest`, `npm test`, manual R check)
- [ ] **Coverage maintained** (Rust ≥90%, Python ≥65%)

### Verification Commands

```bash
# Full validation (from project root)
bash scripts/verify_code.sh

# Then regenerate a test site to ensure generation still works
cargo run -- new goat --output-dir /tmp/test-gen --config config/

# Test generated Python + R + JS (in generated project)
cd /tmp/test-gen/goat-cli
maturin develop --features extension-module
pytest tests/python/ -v
npm test  # (if JavaScript included)
# Manual R testing in R console
```

---

## Related Documents

- [.github/copilot-instructions.md](../../.github/copilot-instructions.md) — Updated with extension guidelines
- [AGENTS.md](../../AGENTS.md) — Agent-specific best practices (references this guide)
- [integration-runbook.md](./integration-runbook.md) — For integration-specific extensions
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — General coding standards

### 1.3 Update JavaScript template

**File**: `templates/js/query.js`

```javascript
setCustomField(value) {
  this.customField = value;
  return this;
}
```

Update `toQueryYaml()` similar to Python.

### 1.4 Update R template

**File**: `templates/r/query.R`

```r
set_custom_field = function(value) {
  private$custom_field <- value
  self
}
```

### 1.5 Add tests for parity

**File**: `tests/python/test_sdk_parity.py`

```python
CANONICAL_METHODS["set_custom_field"] = {
    "params": ["value"],
    "python_name": "set_custom_field",
    "js_name": "setCustomField",
    "r_name": "set_custom_field",
}
```

### 1.6 Regenerate and test

```bash
# Test generation
cargo run -- new boat --output-dir /tmp/test-boat --config sites/

# Test SDK
cd /tmp/test-boat/goat-cli/python
maturin develop --features extension-module
python -c "from goat_sdk import QueryBuilder; qb = QueryBuilder('taxon').set_custom_field('test'); print(qb.to_query_yaml())"
```

**Checklist**:

- [ ] Rust core method + tests
- [ ] Python template updated + method works
- [ ] JavaScript template updated + method works
- [ ] R template updated + method works
- [ ] Parity test added (`CANONICAL_METHODS`)
- [ ] All SDKs regenerate cleanly
- [ ] Manual smoke test passes

---

## Task 2: Add Support for a New Language (e.g., Go)

**Goal**: Generate Go SDK with query builder

### 2.1 Create Go template

**File**: `templates/go/query.go.tera`

```go
package sdk

type QueryBuilder struct {
    Index string
    Taxa []string
    // ... other fields
}

func NewQueryBuilder(index string) *QueryBuilder {
    return &QueryBuilder{
        Index: index,
    }
}

func (q *QueryBuilder) SetTaxa(taxa []string) *QueryBuilder {
    q.Taxa = taxa
    return q
}

func (q *QueryBuilder) ToQueryYAML() (string, error) {
    // Marshal to YAML
    return "", nil
}
```

### 2.2 Register template in code generator

**File**: `src/commands/new.rs` (in `copy_templates()` function)

Add Go copy logic:

```rust
let go_src = format!("{}/generated/goat-cli/go", output_path);

// Copy Go template
fs::create_dir_all(&go_src)?;
fs::write(
    format!("{}/query.go", go_src),
    include_str!("../../templates/go/query.go.tera")
        .replace("{{ package }}", "goat")
)?;
```

### 2.3 Add parity test for Go

**File**: `tests/python/test_sdk_parity.py`

```python
def get_go_methods():
    """Extract methods from generated Go SDK."""
    query_go = PROJECT_ROOT / "workdir/goat-cli/go/query.go"
    # Parse with regex similar to JavaScript
    ...

def test_go_methods_match_canonical():
    go_methods = get_go_methods()
    for concept, spec in CANONICAL_METHODS.items():
        go_name = spec.get("go_name")
        assert go_name in go_methods, f"Go SDK missing {go_name}"
```

### 2.4 Test generation

```bash
cargo run -- new boat --output-dir /tmp/test-go --config sites/

# Check Go code was generated
ls /tmp/test-go/boat-cli/go/
```

**Checklist**:

- [ ] Go template created (query.go.tera)
- [ ] Template registered in code generator
- [ ] Generator produces valid Go code
- [ ] Go code compiles (`go build ./...`)
- [ ] Parity test added
- [ ] All other languages still work

---

## Task 3: Add Custom Validation Rule

**Goal**: Add a rule "genome_size must include a unit (G, M, K, B)"

### 3.1 Add validation function

**File**: `src/core/query/validation.rs`

```rust
pub fn validate_genome_size(value: &str) -> Result<(), String> {
    let valid_units = ["G", "M", "K", "B"];
    if valid_units.iter().any(|u| value.ends_with(u)) {
        Ok(())
    } else {
        Err("genome_size must end with G, M, K, or B".to_string())
    }
}
```

Add test:

```rust
#[test]
fn genome_size_unit_validation() {
    assert!(validate_genome_size("1G").is_ok());
    assert!(validate_genome_size("1000").is_err());
}
```

### 3.2 Wire into validator

**File**: `src/core/query/validation.rs` (in `validate_query()`)

```rust
if let Some(size_attr) = query.attributes.find("genome_size") {
    if let Some(value_str) = &size_attr.value {
        validate_genome_size(value_str)?;
    }
}
```

### 3.3 Test end-to-end

```bash
cd goat-cli/python
python -c "
from goat_sdk import QueryBuilder
qb = QueryBuilder('taxon').add_attribute('genome_size', operator='>', value='1000')
errors = qb.validate()  # Should error
print(errors)
"
```

**Checklist**:

- [ ] Validation function added + tested
- [ ] Wired into validator path
- [ ] Python SDK picks up validation error
- [ ] JavaScript/R also see validation errors (if FFI exposed)

---

## Task 4: Add a New Snippet Language

**Goal**: Generate Bash curl command snippets for API calls

### 4.1 Create Bash template

**File**: `templates/snippets/bash_snippet.tera`

```bash
#!/bin/bash
# Query: {{ query_name }}
# Generated: {{ generated_date }}

API_BASE="{{ api_base }}"
ENDPOINT="{{ endpoint }}"

curl -X POST "$API_BASE/$ENDPOINT" \
  -H "Content-Type: application/json" \
  -d '{
    "index": "{{ index }}",
    "taxa": {{ taxa | json }},
    "size": {{ size }}
  }'
```

### 4.2 Register in code generator

**File**: `src/core/snippet.rs`

```rust
pub fn generate_bash(query: &SearchQuery) -> String {
    let mut tera = Tera::new("templates/snippets/bash_snippet.tera")?;
    // Fill context with query data
    ...
}
```

### 4.3 Test snippet generation

```bash
cargo run -- snippet --language bash --query-file query.yaml
```

**Checklist**:

- [ ] Bash template created
- [ ] Template renders valid bash syntax
- [ ] Snippet generation CLI works
- [ ] Generated snippets execute correctly

---

## Task 5: Customize Generated Project Structure

**Goal**: Change generated Python project layout (e.g., move query.py to sdk/)

### 5.1 Update code generator

**File**: `src/commands/new.rs` (in file copy logic)

Change:

```rust
fs::write(format!("{}/python/cli_generator/query.py", output_path), ...);
```

To:

```rust
fs::create_dir_all(format!("{}/python/cli_generator/sdk", output_path))?;
fs::write(format!("{}/python/cli_generator/sdk/query.py", output_path), ...);
```

### 5.2 Update template imports

**File**: `templates/python/__init__.py.tera`

```python
from .sdk.query import QueryBuilder  # Changed path
```

### 5.3 Test generation

```bash
cargo run -- new boat --output-dir /tmp/test-layout --config sites/
ls /tmp/test-layout/boat-cli/python/cli_generator/sdk/
```

**Checklist**:

- [ ] Directory structure changed as intended
- [ ] All imports updated
- [ ] Generated code still importable from user code
- [ ] Tests pass

---

## Troubleshooting

| Issue                      | Cause                                       | Fix                                             |
| -------------------------- | ------------------------------------------- | ----------------------------------------------- |
| "Template not found" error | Tera file missing or misnamed               | Check `templates/` directory spelling           |
| Python import fails        | Module not registered in `__init__.py.tera` | Add export to template                          |
| Parity test fails          | New language method missing check           | Add to `CANONICAL_METHODS`                      |
| JS WASM won't build        | Template syntax error in build-script       | Check `build-wasm.sh.tera` generates valid bash |

---

## Advanced: Custom Template Variables

Add new variables available to all templates:

**File**: `src/commands/new.rs` (in context building)

```rust
context.insert("custom_org", json!("your-org"));
context.insert("custom_license", json!("MIT"));
```

Use in templates:

```python
# templates/python/__init__.py.tera
"""
Organization: {{ custom_org }}
License: {{ custom_license }}
"""
```

---

## Related Docs

- [Integration Runbook](integration-runbook.md) — Onboard a new repo
- [API Refactoring Plan](api-aggregation-refactoring-plan.md) — Long-term architecture
- [Query Builder Design](../reference/query-builder-design.md) — Core SDK design
- [Testing Strategy](../testing/sdk-parity-testing.md) — How to test your extensions

---

## Checklist: Complete Custom Extension

- [ ] Rust core logic works (tests pass)
- [ ] Python SDK updated + smoke test works
- [ ] JavaScript SDK updated + smoke test works
- [ ] R SDK updated + smoke test works (if language added)
- [ ] Parity tests added/updated
- [ ] All existing tests still pass
- [ ] Documentation updated
- [ ] Commit with clear message: "feat: add {feature} to SDKs"
