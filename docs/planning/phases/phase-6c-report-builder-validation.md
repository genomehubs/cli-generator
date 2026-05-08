# Phase 6c: ReportBuilder and Report Validation

**Depends on:** Phase 6b (v3 migration — Python SDK complete)
**Blocks:** Phase 6d (CLI report subcommand), Phase 6f (R/JS SDK report), Phase 6g (docs), Phase XX (describe/snippet extensions)
**Follows Rust-first pattern:** logic in `crates/genomehubs-query/src/core/`, PyO3 in `src/lib.rs`, templates wired to Rust bindings

---

## Motivation

Phase 6b added a `_post_json` transport helper and `search_all` cursor loop to `QueryBuilder`. A `ReportBuilder` class — mirroring `QueryBuilder`'s chainable design — is the planned entry point for all report calls. `QueryBuilder.report()` will accept only a `ReportBuilder` instance; there is no flat-kwargs form.

---

## Work Items

### 1. Report type enum in `genomehubs-query`

**Files:** `crates/genomehubs-query/src/report.rs` (new)

Move the report type knowledge from `crates/genomehubs-api/src/routes/report.rs` into the shared query crate so it can be used by both the API and the validator.

```rust
/// Supported v3 report types.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportType {
    Histogram,
    Scatter,
    Map,
    Tree,
    CountPerRank,
    Sources,
    Arc,
}

impl ReportType {
    /// Fields that must be present in the report YAML for this type.
    pub fn required_axes(&self) -> &'static [&'static str] {
        match self {
            Self::Histogram    => &["x"],
            Self::Scatter      => &["x", "y"],
            Self::Map          => &[],
            Self::Tree         => &["rank"],
            Self::CountPerRank => &["query"],
            Self::Sources      => &[],
            Self::Arc          => &["x"],
        }
    }

    /// Fields that may be present for this type (used by validator to warn on unknowns).
    pub fn valid_axes(&self) -> &'static [&'static str] {
        match self {
            Self::Histogram    => &["x", "y", "cat", "rank", "fields", "status_filter", "cat_rank", "cat_opts", "x_opts", "y_opts"],
            Self::Scatter      => &["x", "y", "cat", "rank", "fields", "status_filter", "scatter_threshold", "cat_opts", "x_opts", "y_opts"],
            Self::Map          => &["location_field", "hex_resolution", "map_threshold", "rank", "status_filter"],
            Self::Tree         => &["rank", "collapse_monotypic", "preserve_rank", "count_rank", "status_filter", "cat", "cat_rank"],
            Self::CountPerRank => &["query", "ranks", "cat", "cat_opts"],
            Self::Sources      => &["rank", "fields", "status_filter"],
            Self::Arc          => &["x", "y", "cat", "x_opts", "y_opts", "cat_opts"],
        }
    }
}
```

---

### 2. `validate_report_yaml` function

**Files:** `crates/genomehubs-query/src/validation.rs`

```rust
/// Validate a report YAML string against the report type rules and optionally
/// the field metadata for the query's index.
///
/// Returns a JSON array of error strings (empty array if valid).
pub fn validate_report_yaml(
    report_yaml: &str,
    field_meta_json: &str,  // "{}" if no field metadata available
) -> String { ... }
```

Checks:

1. `report` key is present and is a known `ReportType` variant
2. All `required_axes()` for the report type are present
3. Axis field names (x, y, cat) are known fields (when `field_meta_json` is non-empty)
4. Numeric config values are in range: `hex_resolution` 1–12, `map_threshold` > 0, `scatter_threshold` > 0

Returns a JSON array of error strings, matching the signature of `validate_query_json`.

Unit tests: one test per report type for the happy path; one test per required-axis violation; one test for unknown field name with metadata; one for range checks.

---

### 3. PyO3 exposure

**Files:** `src/lib.rs`, `python/cli_generator.pyi`

```rust
#[pyfunction]
fn validate_report_yaml(report_yaml: &str, field_meta_json: &str) -> String {
    genomehubs_query::validation::validate_report_yaml(report_yaml, field_meta_json)
}
```

Register in `#[pymodule]`. Add stub to `.pyi`:

```python
def validate_report_yaml(report_yaml: str, field_meta_json: str) -> str: ...
```

---

### 4. `ReportBuilder` — Python library

**File:** `python/cli_generator/query.py`

New class alongside `QueryBuilder`. Design mirrors `QueryBuilder`'s chainable pattern:

```python
class ReportBuilder:
    """Builder for v3 /report POST body configuration.

    Constructs the report_yaml that controls how a /report query is visualised.
    Designed to be paired with a QueryBuilder:

        rb = ReportBuilder("histogram").set_x("genome_size").set_rank("species")
        data = qb.report(rb)
    """

    def __init__(self, report_type: str) -> None: ...

    # Axis setters — each returns self for chaining
    def set_x(self, field: str, opts: str = "") -> "ReportBuilder": ...
    def set_y(self, field: str | list[str], opts: str = "") -> "ReportBuilder": ...
    def set_cat(self, field: str, opts: str = "") -> "ReportBuilder": ...

    # Common config
    def set_rank(self, rank: str) -> "ReportBuilder": ...
    def set_fields(self, fields: list[str]) -> "ReportBuilder": ...
    def set_status_filter(self, value: str) -> "ReportBuilder": ...
    def set_cat_rank(self, rank: str) -> "ReportBuilder": ...

    # Type-specific config
    def set_collapse_monotypic(self, value: bool = True) -> "ReportBuilder": ...
    def set_preserve_rank(self, rank: str) -> "ReportBuilder": ...
    def set_count_rank(self, rank: str) -> "ReportBuilder": ...
    def set_location_field(self, field: str) -> "ReportBuilder": ...
    def set_hex_resolution(self, resolution: int) -> "ReportBuilder": ...
    def set_map_threshold(self, threshold: int) -> "ReportBuilder": ...
    def set_scatter_threshold(self, threshold: int) -> "ReportBuilder": ...

    # Serialisation
    def to_report_yaml(self) -> str:
        """Return the report configuration as a YAML string."""
        ...

    # Validation
    def validate(self, field_meta: dict[str, Any] | None = None) -> list[str]:
        """Return list of validation errors. Empty = valid."""
        import json
        field_meta_json = json.dumps(field_meta or {})
        return json.loads(_ext.validate_report_yaml(self.to_report_yaml(), field_meta_json))

    # Execution (convenience — delegates to QueryBuilder.report)
    def run(self, query_builder: "QueryBuilder") -> Any:
        """Execute this report against the given QueryBuilder's query."""
        return query_builder.report(self)
```

**`QueryBuilder.report()` updated signature:**

```python
def report(self, report: "ReportBuilder") -> Any:
    """Run a /report query.

    Args:
        report: A :class:`ReportBuilder` instance.
    """
    data = self._post_json(
        f"{api_base}/{api_version}/report",
        {
            "query_yaml": self.to_query_yaml(),
            "params_yaml": self.to_params_yaml(),
            "report_yaml": report.to_report_yaml(),
        },
    )
    return data.get("report", data)
```

---

### 5. `ReportBuilder` — Python template

**File:** `templates/python/query.py.tera`

Mirror `python/cli_generator/query.py`'s `ReportBuilder` class exactly. The template version uses the same `_ext.validate_report_yaml` call. The `API_BASE` constant is used for the `run()` call.

Key differences from the library version:

- No `api_base` parameter on `run()` — baked into `API_BASE` at generation time
- `from . import build_url as _build_url` is not needed (reports don't build v2 URLs)

---

### 6. Update generated-project embedding

**File:** `src/commands/new.rs`

`validate_report_yaml` is called from the template (via `_ext`), so the Rust function must be available in generated projects. The function lives in `crates/genomehubs-query/src/validation.rs` — already copied to generated projects via `copy_embedded_modules()`. No new file copy required; just ensure `validate_report_yaml` is registered in `templates/rust/lib.rs.tera` and `extendr-wrappers.R.tera`.

---

### 7. `ReportBuilder` — R template

**File:** `templates/r/query.R`

New R6 class following the same naming and setter pattern as the Python version:

```r
#' @title ReportBuilder
#' @description Build report configurations for v3 /report POST calls.
ReportBuilder <- R6::R6Class("ReportBuilder",
  public = list(
    initialize = function(report_type) { ... },
    set_x = function(field, opts = "") { ... self },
    set_y = function(field, opts = "") { ... self },
    set_cat = function(field, opts = "") { ... self },
    set_rank = function(rank) { ... self },
    set_fields = function(fields) { ... self },
    set_status_filter = function(value) { ... self },
    set_cat_rank = function(rank) { ... self },
    set_collapse_monotypic = function(value = TRUE) { ... self },
    set_preserve_rank = function(rank) { ... self },
    set_count_rank = function(rank) { ... self },
    set_location_field = function(field) { ... self },
    set_hex_resolution = function(resolution) { ... self },
    set_map_threshold = function(threshold) { ... self },
    set_scatter_threshold = function(threshold) { ... self },
    to_report_yaml = function() { ... },
    validate = function(field_meta = NULL) {
      meta_json <- if (is.null(field_meta)) "{}" else jsonlite::toJSON(field_meta, auto_unbox = TRUE)
      jsonlite::fromJSON(validate_report_yaml(self$to_report_yaml(), meta_json))
    },
    run = function(query_builder) { query_builder$report(self) }
  ),
  private = list(...)
)
```

`QueryBuilder$report()` updated to accept either an `ReportBuilder` or a character string.

---

### 8. `ReportBuilder` — JS template

**File:** `templates/js/query.js`

New JS class:

```js
class ReportBuilder {
  constructor(reportType) { ... }
  setX(field, opts = "") { ...; return this; }
  setY(field, opts = "") { ...; return this; }
  setCat(field, opts = "") { ...; return this; }
  setRank(rank) { ...; return this; }
  setFields(fields) { ...; return this; }
  setStatusFilter(value) { ...; return this; }
  setCatRank(rank) { ...; return this; }
  setCollapseMonotypic(value = true) { ...; return this; }
  setPreserveRank(rank) { ...; return this; }
  setCountRank(rank) { ...; return this; }
  setLocationField(field) { ...; return this; }
  setHexResolution(resolution) { ...; return this; }
  setMapThreshold(threshold) { ...; return this; }
  setScatterThreshold(threshold) { ...; return this; }
  toReportYaml() { ... }
  validate(fieldMeta = {}) {
    const result = _validateReportYaml(this.toReportYaml(), JSON.stringify(fieldMeta));
    return JSON.parse(result);
  }
  async run(queryBuilder) { return queryBuilder.report(this); }
}
```

`QueryBuilder.report()` updated to accept a `ReportBuilder` or string.

---

## Verification

```bash
# Rust
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test

# Python (after maturin develop)
maturin develop --features extension-module
pyright python/ tests/python/
pytest tests/python/ -v -k "report"
```

---

## Tests to Add

| Test                                                                    | File                              |
| ----------------------------------------------------------------------- | --------------------------------- |
| `test_validate_report_yaml_histogram_valid`                             | `tests/python/test_core.py`       |
| `test_validate_report_yaml_histogram_missing_x`                         | `tests/python/test_core.py`       |
| `test_validate_report_yaml_scatter_missing_y`                           | `tests/python/test_core.py`       |
| `test_validate_report_yaml_map_valid`                                   | `tests/python/test_core.py`       |
| `test_validate_report_yaml_unknown_type`                                | `tests/python/test_core.py`       |
| `test_validate_report_yaml_hex_resolution_out_of_range`                 | `tests/python/test_core.py`       |
| `test_report_builder_constructs_valid_yaml`                             | `tests/python/test_core.py`       |
| `test_report_builder_validate_method`                                   | `tests/python/test_core.py`       |
| `test_report_builder_run_delegates_to_qb`                               | `tests/python/test_core.py`       |
| `test_qb_report_accepts_report_builder`                                 | `tests/python/test_core.py`       |
| `test_qb_report_flat_kwargs_still_works`                                | `tests/python/test_core.py`       |
| SDK parity: `ReportBuilder.to_report_yaml()` same output in Python/R/JS | `tests/python/test_sdk_parity.py` |

---

## Ordering

1. `report.rs` — `ReportType` enum and `required_axes()`/`valid_axes()` (Rust)
2. `validate_report_yaml()` in `validation.rs` + unit tests (Rust)
3. PyO3 exposure + `.pyi` stub
4. `ReportBuilder` in `query.py` (Python library) + unit tests
5. `ReportBuilder` in `query.py.tera` (Python template)
6. Verify `validate_report_yaml` available in generated project via lib.rs.tera
7. `ReportBuilder` in `query.R` (R template)
8. `ReportBuilder` in `query.js` (JS template)
9. SDK parity tests
