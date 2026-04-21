# Extension Guide

Customize and extend cli-generator for your project's needs.

**Status**: 📋 PLANNING (stub created 2026-04-21)
**Owner**: [Your team]
**Effort**: 4–5 hours to write with worked examples

---

## Overview

This guide explains how to:
- Add new query parameters to the SDK
- Add support for a new language (e.g., Go)
- Extend the validator with custom rules
- Add new snippet languages or custom snippets
- Customize generated code structure

**Audience**: SDK developers and CLI maintainers
**Outcome**: Custom SDKs tailored to your project

---

## Template Lookup Reference

All customizable components are in cli-generator:

```
templates/
├── python/
│   ├── query.py.tera          ← Python SDK class methods
│   ├── __init__.py.tera       ← Package exports
│   └── ...
├── js/
│   ├── query.js               ← JavaScript SDK class methods (plain JS, not Tera)
│   └── ...
├── r/
│   ├── query.R                ← R SDK class methods
│   └── ...
├── rust/
│   └── client.rs.tera         ← Rust FFI wrapper
└── snippets/
    ├── python_snippet.tera    ← Code generation examples
    └── ...

src/core/
├── query/
│   └── validation.rs          ← Validation rules live here
└── ...
```

---

## Task 1: Add a New Query Parameter

**Goal**: Add `set_custom_field(value)` method to all SDKs

### 1.1 Add Rust core method

**File**: `src/core/query/mod.rs`

```rust
impl SearchQuery {
    /// Set a custom field constraint.
    pub fn set_custom_field(&mut self, value: String) -> &mut Self {
        self.custom_field = Some(value);
        self
    }
}
```

Add tests in same file:
```rust
#[test]
fn custom_field_serializes() {
    let mut q = SearchQuery::default();
    q.set_custom_field("test_value".to_string());
    let yaml = q.to_yaml().unwrap();
    assert!(yaml.contains("test_value"));
}
```

### 1.2 Update Python template

**File**: `templates/python/query.py.tera`

Add method:
```python
def set_custom_field(self, value: str) -> "QueryBuilder":
    """Set a custom field constraint."""
    self._custom_field = value
    return self
```

Add to `to_query_yaml()`:
```python
if self._custom_field:
    query["custom_field"] = self._custom_field
```

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

| Issue | Cause | Fix |
|-------|-------|-----|
| "Template not found" error | Tera file missing or misnamed | Check `templates/` directory spelling |
| Python import fails | Module not registered in `__init__.py.tera` | Add export to template |
| Parity test fails | New language method missing check | Add to `CANONICAL_METHODS` |
| JS WASM won't build | Template syntax error in build-script | Check `build-wasm.sh.tera` generates valid bash |

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
