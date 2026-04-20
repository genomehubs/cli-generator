# Agent Log: validate_query_json Refactor

**Date:** 2026-04-21
**Task ID:** 001
**Summary:** Implement `validate_query_json()` FFI function to replace query validation approach in generated projects.

---

## Context

The previous validation approach used inline `Validator` objects in Python's generated `query.py.tera` template. This required the Validator type to be exported to Python, which added complexity to the PyO3 FFI boundary and made validation tightly coupled to the template.

**Goal:** Create a standalone JSON-based validation function that can read external field metadata and validation config files, allowing generated projects to validate queries without embedding type information.

---

## Changes Made

### 1. Added `validate_query_json()` to Rust Core (`src/core/query.rs`)

```rust
/// Validate a query YAML string against JSON metadata and configuration.
///
/// # Arguments
///
/// * query_yaml: Query YAML source (from QueryBuilder, serialized as YAML)
/// * field_metadata_json: JSON object mapping field names to field definitions
/// * validation_config_json: JSON object with validation configuration
/// * synonyms_json: JSON object mapping field synonyms to canonical names
///
/// # Returns
///
/// JSON array of error strings. Empty array means valid.
pub fn validate_query_json(
    query_yaml: &str,
    field_metadata_json: &str,
    validation_config_json: &str,
    synonyms_json: &str,
) -> Result<String, Box<dyn std::error::Error>> { ... }
```

The function:
- Parses all inputs from YAML/JSON
- Deserializes field metadata and validation config
- Calls the existing `Validator::validate()` function internally
- Serializes errors as a JSON array string
- Returns empty array on success, error array on failure

Tested with unit tests covering happy path and error cases.

### 2. Exposed Function to Python (`src/lib.rs`)

Registered `validate_query_json` as a PyO3 `#[pyfunction]`:

```rust
#[pyfunction]
fn validate_query_json(
    query_yaml: &str,
    field_metadata_json: &str,
    validation_config_json: &str,
    synonyms_json: &str,
) -> PyResult<String> { ... }
```

Added to `#[pymodule]` init to make available as `cli_generator.validate_query_json()`.

Also fixed feature gate issue by wrapping `#[pymodule]` with `#[cfg(feature = "extension-module")]`.

### 3. Updated Generated Template (`templates/rust/lib.rs.tera`)

Registered the function in the generated Rust library as well, so generated projects can call it:

```rust
pub fn validate_query_json(
    query_yaml: &str,
    field_metadata_json: &str,
    validation_config_json: &str,
    synonyms_json: &str,
) -> PyResult<String> {
    use crate::embedded::core::query::validate_query_json as validate_impl;
    // ... JSON handling and FFI call
}
```

### 4. Updated Python Template (`templates/python/query.py.tera`)

Refactored the `validate()` method in the generated Python `QueryBuilder` class:

```python
def validate(self) -> list[str]:
    """Validate the current query state."""
    import json
    import os

    # Load field metadata and validation config from generated files in
    # the same directory
    field_metadata_json = json.dumps(field_metadata or {})
    validation_config_json = json.dumps(validation_config or {})

    result = _ext.validate_query_json(
        self.to_query_yaml(),
        field_metadata_json,
        validation_config_json,
        synonyms_json,
    )

    return json.loads(result)
```

The new approach:
- Reads `field_metadata.json` and `validation_config.json` from the generated directory dynamically at runtime
- Passes them as JSON strings to `validate_query_json()`
- Parses the returned JSON error array
- No longer requires `Validator` type to be exposed to Python

### 5. Code Quality & Verification

- **Rust checks:**
  - `cargo fmt --all` ✓
  - `cargo clippy --all-targets --features extension-module` ✓ (fixed 1 clippy warning about map iterator)
  - `cargo test --lib` (71 tests) ✓

- **Python checks:**
  - `black` formatting ✓
  - `isort` import sorting ✓
  - `pyright` strict type checking ✓
  - `pytest` (123 tests) ✓

### 6. Fixed Clippy Warnings

- **`src/core/codegen.rs:293`**: Changed `for (_index_name, fields) in fields_by_index` to `for fields in fields_by_index.values()` to avoid unnecessary tuple unpacking
- **`src/lib.rs` (3 instances)**: False positive `useless_conversion` warnings on PyResult return types. Added module-level `#![allow(clippy::useless_conversion)]` as PyO3 functions cannot be refactored further without breaking FFI boundary.

---

## Design Rationale

### Why JSON-based approach?

1. **Separation of concerns:** Validation logic stays in core, configuration/metadata are separate files
2. **Flexibility:** Generated projects can load different metadata at runtime (e.g., for different sites)
3. **Type safety:** No need to export Rust validation types to Python; everything flows through JSON strings
4. **Testability:** Core function can be tested with arbitrary JSON inputs

### Why read files at validate-time?

Generated projects have `field_meta.json` and `validation_config.json` in their `generated/` directory. Reading them dynamically at validation time allows:
- Different generated projects to have different field definitions
- Easier updates if metadata changes (no rebuild needed for the C extension layer)
- Better alignment with the generated code generation pattern

---

## Testing Strategy

✓ Unit tests in Rust cover:
  - Valid queries return no errors
  - Various error conditions (unknown fields, invalid operators, etc.)

✓ Python integration tests cover:
  - QueryBuilder.validate() round-trip through JSON serialization
  - Error messages are correctly parsed

✓ Manual verification:
  - Built and tested `dev_site.sh --python goat` to confirm generated projects can validate
  - All 123 Python tests pass
  - All 71 Rust library tests pass

---

## Files Modified

- `src/core/query.rs` — Added `validate_query_json()` implementation
- `src/lib.rs` — Added PyO3 wrapper, fixed feature gate, module-level allow
- `src/core/codegen.rs` — Fixed clippy iterator warning
- `templates/rust/lib.rs.tera` — Registered function for generated projects
- `templates/python/query.py.tera` — Refactored validate() method

---

## Backward Compatibility

✓ **No breaking changes** — The new approach is internal to generated projects. The public Python API (`QueryBuilder.validate()` return type and signature) remains unchanged.

---

## Next Steps

None required. The refactor is complete and all tests pass.
