# Test Coverage Strategy

**Status:** Planning
**Date:** 19 March 2026
**Scope:** Multi-level testing for cli-generator and generated sites before adding R, JS, Go SDKs

---

## Overview

Testing occurs at three levels:

1. **Generator tests** — Does cli-generator itself work? (Rust unit + Python integration)
2. **Generated code tests** — Does the generated SDK/CLI work? (Integration tests on GOAT/BOAT)
3. **Multi-language tests** — Do all languages generate correctly? (Phase 2+)

Current state: ✅ Decent unit/integration, ⚠️ **No property tests**, ⚠️ **No command tests**, ⚠️ **No error tests**.

This document establishes **coverage targets**, **testing strategy**, and **enforcement via CI**.

---

## Level 1: Generator Tests (Rust + Python)

### 1.1 Rust Testing

#### Current Coverage

- **33 unit tests** across 7 modules (config, fetch, query, codegen, commands)
- **2 integration tests** (generated_goat_cli.rs)
- **0 property tests** (framework installed but unused)
- **0 error scenario tests** (invalid configs, missing files, etc.)

#### Targets

| Category        | Target         | Current              | Gap                                                              |
| --------------- | -------------- | -------------------- | ---------------------------------------------------------------- |
| Unit tests      | 1 per function | ~15 functions tested | ~10 functions                                                    |
| Line coverage   | 85%+           | **Unknown**          | Measure first                                                    |
| Branch coverage | 80%+           | **Unknown**          | Measure first                                                    |
| Property tests  | 3+ invariants  | 0                    | Add QueryBuilder round-trip, config validation, field resolution |
| Error tests     | 10+ scenarios  | 0                    | Add malformed YAML, missing files, invalid operators             |

#### Testing Strategy

**A. Activate Code Coverage Measurement**

Add `tarpaulin` to measure line coverage:

```toml
# Cargo.toml
[dev-dependencies]
cargo-tarpaulin = "0.20"  # or use via: cargo install cargo-tarpaulin
```

**Command:**

```bash
cargo tarpaulin --out Html --output-dir ./coverage --exclude-files tests/ --timeout 300
```

Configuration file `tarpaulin.toml`:

```toml
[coverage]
timeout = 300
exclude-files = ["tests/"]
exclude-lines = ["unreachable!", "panic!", "unimplemented!"]
```

**CI Integration** (update `.github/workflows/ci.yml`):

```yaml
- name: Measure Rust coverage
  if: matrix.os == 'ubuntu-latest' # Only measure on one platform
  run: |
    cargo install cargo-tarpaulin
    cargo tarpaulin --out Xml --output-dir ./coverage

- name: Upload coverage to Codecov
  uses: codecov/codecov-action@v3
  with:
    files: ./coverage/cobertura.xml
    flags: rust
```

**B. Write Property Tests (proptest)**

Create `src/core/query/tests.rs` with property-based tests:

```rust
#[cfg(test)]
mod proptest_tests {
    use proptest::prelude::*;
    use crate::core::query::{QuerySnapshot, QueryBuilder};

    // Property: URL round-trip idempotency
    // If you encode a query to URL and decode it back, you get the same query
    proptest! {
        #[test]
        fn prop_query_url_roundtrip(
            filters in "(\\w+|\\W+)*",  // Field names
            operator in "(=|>=|<=|>|<|!=)",
            value in "(\\w+|\\W+)*"
        ) {
            let qb = QueryBuilder::new("taxon");
            let url = qb.add_filter(&filters, &operator, &value).build();

            // Verify URL contains the encoded filter
            assert!(url.contains("%3D") || url.contains("="));  // encoded or literal =
        }
    }

    // Property: Field resolution consistency
    // Pattern matching should always resolve to the same canonical fields
    proptest! {
        #[test]
        fn prop_field_pattern_consistency(
            pattern in "(\\w+)\\*?",  // glob pattern
        ) {
            let fields = vec![
                FieldDef { name: "genome_size".to_string(), ..default() },
                FieldDef { name: "genome_size_draft".to_string(), ..default() },
                FieldDef { name: "sequence_count".to_string(), ..default() },
            ];

            // Run multiple times; result should be deterministic
            let result1 = resolve_pattern(&pattern, &fields);
            let result2 = resolve_pattern(&pattern, &fields);
            assert_eq!(result1, result2);
        }
    }

    // Property: Config parsing stability
    // Valid YAML should always parse consistently
    proptest! {
        #[test]
        fn prop_config_roundtrip(yaml_str in ".*") {
            if let Ok(config) = serde_yaml::from_str::<SiteConfig>(&yaml_str) {
                let re_serialized = serde_yaml::to_string(&config).unwrap();
                let reparsed = serde_yaml::from_str::<SiteConfig>(&re_serialized).unwrap();
                assert_eq!(config, reparsed);
            }
        }
    }
}
```

**Execution:**

```bash
cargo test --lib proptest_tests -- --nocapture
```

**C. Write Error Scenario Tests**

Create `src/core/tests/errors.rs`:

```rust
#[cfg(test)]
mod error_tests {
    #[test]
    fn config_missing_required_field() {
        let yaml = "name: testsite\n";  // Missing display_name
        let result = serde_yaml::from_str::<SiteConfig>(&yaml);
        assert!(result.is_err());
    }

    #[test]
    fn config_invalid_yaml_syntax() {
        let yaml = "name: [unclosed array";
        let result = serde_yaml::from_str::<SiteConfig>(&yaml);
        assert!(result.is_err());
    }

    #[test]
    fn querybuilder_invalid_operator() {
        let qb = QueryBuilder::new("taxon");
        let result = qb.add_filter("field", "INVALID_OP", "value").build();
        // Should either error or sanitize; verify consistent behavior
        assert!(!result.is_empty());
    }

    #[test]
    fn field_synonym_unknown_alias() {
        let fields = vec![
            FieldDef { name: "canonical_name".to_string(), synonyms: vec!["old_name".to_string()], ..default() },
        ];
        let result = resolve_field("nonexistent", &fields);
        // Should gracefully handle unknown field
        assert!(result.is_none());
    }
}
```

#### Coverage Target Enforcement

Create `scripts/check_coverage.sh`:

```bash
#!/bin/bash
set -e

echo "Measuring Rust code coverage..."
cargo tarpaulin --out Xml --output-dir ./coverage --timeout 300

# Parse coverage from XML and check thresholds
LINE_COVERAGE=$(grep -oP 'line-rate="\K[0-9.]+' ./coverage/cobertura.xml | head -1)
LINE_THRESHOLD=0.85

if (( $(echo "$LINE_COVERAGE < $LINE_THRESHOLD" | bc -l) )); then
    echo "❌ Line coverage $LINE_COVERAGE is below threshold $LINE_THRESHOLD"
    exit 1
fi

echo "✅ Line coverage: $LINE_COVERAGE (threshold: $LINE_THRESHOLD)"
```

**Integrate into CI:**

```yaml
- name: Check coverage thresholds
  run: bash scripts/check_coverage.sh
```

---

### 1.2 Python Testing

#### Current Coverage

- **18+ tests** in test_core.py
- **Hypothesis configured** (profiles in conftest.py)
- **0 @given property tests**
- **0 error scenario tests**
- **Unknown coverage %**

#### Targets

| Category                | Target        | Current     | Gap                                   |
| ----------------------- | ------------- | ----------- | ------------------------------------- |
| Line coverage           | 85%+          | **Unknown** | Measure with coverage.py              |
| Branch coverage         | 80%+          | **Unknown** | Measure with coverage.py              |
| Property tests (@given) | 5+ invariants | 0           | Add to test_core.py                   |
| Error tests             | 10+ scenarios | 0           | Add query validation, invalid configs |

#### Testing Strategy

**A. Measure Python Coverage**

Install `coverage.py`:

```bash
pip install coverage pytest-cov
```

**Run tests with coverage:**

```bash
coverage run -m pytest tests/python/ --cov=python/cli_generator --cov-report=html --cov-report=term
```

**Configuration** (`pyproject.toml`):

```toml
[tool.coverage.run]
source = ["python/cli_generator"]
omit = ["*/__pycache__/*", "*/site-packages/*"]

[tool.coverage.report]
exclude_lines = [
    "pragma: no cover",
    "def __repr__",
    "raise AssertionError",
    "raise NotImplementedError",
    "if __name__ == .__main__.:",
]
fail_under = 85  # Fail if coverage drops below 85%

[tool.pytest.ini_options]
testpaths = ["tests/python"]
```

**B. Write Property Tests with Hypothesis**

Extend `tests/python/test_core.py`:

```python
import pytest
from hypothesis import given, strategies as st, settings, HealthCheck
from cli_generator import QueryBuilder, Validator

# Property: URL round-trip
@given(
    filters=st.lists(
        st.tuples(
            st.text(alphabet="abcdefghijklmnopqrstuvwxyz_", min_size=1, max_size=20),
            st.sampled_from(["=", ">", "<", ">=", "<=", "!="]),
            st.text(alphabet="abcdefghijklmnopqrstuvwxyz0123456789", min_size=1, max_size=20),
        ),
        min_size=0,
        max_size=5
    )
)
@settings(max_examples=200)  # Use CI profile
def test_querybuilder_roundtrip(filters):
    """QueryBuilder should encode/decode consistently."""
    qb = QueryBuilder("taxon")
    for field, op, value in filters:
        qb.add_filter(field, op, value)

    url = qb.build()
    assert url is not None
    assert "taxon" in url.lower()

# Property: Field expansion idempotency
@given(
    patterns=st.lists(
        st.text(alphabet="abcdefghijklmnopqrstuvwxyz_*", min_size=1, max_size=15),
        min_size=1,
        max_size=3
    )
)
def test_field_resolution_deterministic(patterns):
    """Field pattern resolution should be deterministic."""
    validator = Validator("goat")

    result1 = [validator.resolve_field(p) for p in patterns]
    result2 = [validator.resolve_field(p) for p in patterns]

    assert result1 == result2

# Property: Validator never rejects valid canonical names
@given(
    canonical_names=st.lists(
        st.just("genome_size") | st.just("organism_name"),  # Known valid fields
        min_size=1,
        max_size=5
    )
)
def test_validator_accepts_canonical(canonical_names):
    """Validator should accept all canonical field names."""
    validator = Validator("goat")

    for name in canonical_names:
        result = validator.validate_field(name)
        assert result is not None, f"Validator rejected canonical field: {name}"
```

**C. Write Error Scenario Tests**

Extend `tests/python/test_core.py`:

```python
def test_querybuilder_invalid_index():
    """QueryBuilder should handle unknown indexes gracefully."""
    with pytest.raises(ValueError):
        QueryBuilder("unknown_index")

def test_validator_unknown_field():
    """Validator should return None for unknown fields."""
    validator = Validator("goat")
    result = validator.validate_field("nonexistent_field")
    assert result is None

def test_querybuilder_invalid_operator():
    """QueryBuilder should reject invalid operators."""
    qb = QueryBuilder("taxon")
    with pytest.raises(ValueError):
        qb.add_filter("field", "INVALID", "value")

def test_querybuilder_circular_restriction():
    """QueryBuilder.restrict() should reject circular restrictions."""
    qb = QueryBuilder("taxon")
    with pytest.raises(ValueError):
        qb.restrict("field", restriction_field="field")  # Self-reference

def test_validator_enum_constraint_validation():
    """Validator should validate enum values."""
    validator = Validator("goat")
    # assembly_level should only accept: ["Contig", "Scaffold", "Chromosome"]
    assert validator.validate_enum("assembly_level", "Contig")
    assert not validator.validate_enum("assembly_level", "InvalidValue")
```

#### Coverage Target Enforcement

```yaml
# .github/workflows/ci.yml
- name: Measure Python coverage
  run: coverage run -m pytest tests/python/ --cov=python/cli_generator --cov-report=term

- name: Check coverage thresholds
  run: coverage report --fail-under=85
```

---

## Level 2: Generated Code Tests (GOAT/BOAT CLIs)

### 2.1 Current Coverage

- ✅ `goat-cli` is generated and compiled
- ❌ `boat-cli` is **not tested**
- ⚠️ Only content validation and URL smoke tests; limited E2E

### 2.2 Targets

| Category          | Target                         | Current            | Gap                 |
| ----------------- | ------------------------------ | ------------------ | ------------------- |
| CLI generation    | Both sites (`goat`, `boat`)    | goat only          | Add boat-cli to CI  |
| CLI compilation   | Both platforms (Linux, macOS)  | ✅ Both            | Complete            |
| Index coverage    | All indexes in `goat` + `boat` | goat 2/2, boat 0/2 | Add boat testing    |
| Flag combinations | 5+ combos per index            | ~3 per index       | Expand smoke tests  |
| Error handling    | Invalid query strings          | Not tested         | Add CLI error tests |

### 2.3 Testing Strategy

**A. Expand CI Matrix to include BOAT-CLI**

Update `.github/workflows/ci.yml`:

```yaml
strategy:
  matrix:
    site: [goat, boat]
    os: [ubuntu-latest, macos-latest]

jobs:
  generated-cli-tests:
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Test ${{ matrix.site }}-cli generation
        run: |
          cargo run -- new ${{ matrix.site }} --output-dir ./generated/${{ matrix.site }}-cli

      - name: Verify generated structure
        run: |
          test -f ./generated/${{ matrix.site }}-cli/src/cli_meta.rs
          test -f ./generated/${{ matrix.site }}-cli/Cargo.toml

      - name: Compile ${{ matrix.site }}-cli
        run: cd ./generated/${{ matrix.site }}-cli && cargo build --release

      - name: Run ${{ matrix.site }} smoke tests
        run: bash scripts/test_generated_site.sh ${{ matrix.site }}
```

**B. Create Comprehensive Smoke Test Script**

Create `scripts/test_generated_site.sh`:

```bash
#!/bin/bash
set -e

SITE=$1
CLI="./generated/$SITE-cli/target/release/$SITE-cli"

echo "Testing $SITE-cli..."

# Test 1: Help output
$CLI --help | grep -q "Search"

# Test 2: URL generation (no network call)
URL=$($CLI taxon search --field-groups genome-size --url)
echo "Generated URL: $URL"
[[ "$URL" == *"genome_size"* ]] || exit 1

# Test 3: Field group expansion
$CLI --help | grep -q "genome-size"
$CLI taxon search --help | grep -q "genome-size"

# Test 4: Sorting
URL=$($CLI taxon search --sort genome_size --url)
[[ "$URL" == *"sort="* ]] || exit 1

# Test 5: Invalid index should fail gracefully
! $CLI invalid_index search --url 2>/dev/null || exit 1

# Test 6: Multiple filters
URL=$($CLI taxon search \
  --field-groups genome-size \
  --sort genome_size \
  --limit 10 \
  --url)
[[ "$URL" == *"limit=10"* ]] || exit 1

echo "✅ All smoke tests passed for $SITE-cli"
```

**C. Add Python SDK Round-Trip Tests**

Extend `tests/python/test_core.py`:

```python
def test_goat_cli_python_sdk_parity():
    """Generated goat-cli and goat_sdk should produce identical URLs."""
    # Simulate what goat-cli does
    cli_qb = cli_generator.QueryBuilder("taxon")
    cli_qb.add_filter("organism_name", "=", "Escherichia coli")
    cli_url = cli_qb.build()

    # Compare with Python SDK
    sdk_qb = QueryBuilder("taxon")
    sdk_qb.add_filter("organism_name", "=", "Escherichia coli")
    sdk_url = sdk_qb.build()

    assert cli_url == sdk_url, f"Mismatch: CLI {cli_url} vs SDK {sdk_url}"

def test_boat_cli_generation():
    """Verify boat-cli generates without errors."""
    result = subprocess.run(
        ["cargo", "run", "--", "new", "boat", "--output-dir", "./test_boat"],
        capture_output=True,
        text=True
    )
    assert result.returncode == 0, f"boat-cli generation failed: {result.stderr}"
    assert Path("./test_boat/Cargo.toml").exists()
```

---

## Level 3: Multi-Language Coverage (Phase 2+)

### 3.1 When to Test Each Language

| Language   | Phase    | Start testing                   |
| ---------- | -------- | ------------------------------- |
| Rust       | Current  | Now (Level 2)                   |
| Python     | Current  | Now (Level 1.2)                 |
| R          | Phase 2  | After Phase 1 infrastructure ✅ |
| JavaScript | Phase 3  | After Phase 2 ✅                |
| Go         | Phase 3+ | After Phase 2 ✅                |

### 3.2 Multi-Language Coverage Strategy

Once Phase 1 infrastructure is complete, each new language automatically inherits:

- **Unit tests** for `SnippetGenerator::render_snippet()` (tests/python/test_core.py)
- **Template rendering tests** — Verify generated code is syntactically valid
- **Snippet generation tests** — Same query → same snippet across languages
- **Integration tests** — Generated package compiles/installs

Example for Phase 2 (R):

```python
# tests/python/test_core.py
def test_r_sdk_generation():
    """Verify R SDK generates without errors."""
    result = generator.render_all(goat_config, goat_options, fields_by_index)

    assert "r" in result
    assert "r/goat_sdk/R/query.R" in result["r"]
    assert "r/goat_sdk/DESCRIPTION" in result["r"]

def test_r_snippet_generation():
    """Verify R snippet is syntactically valid."""
    query = QuerySnapshot(
        filters=[("genome_size", ">=", "1000000000")],
        sorts=[],
        flags=["genome-size"],
        selections=[],
        traversal=None,
        summaries=[]
    )

    snippet = SnippetGenerator().render_snippet(query, "r", goat_config)

    assert "library(goat_sdk)" in snippet
    assert "add_filter" in snippet
    assert "1000000000" in snippet
```

---

## Implementation Checklist

### Phase 0 (Now): Measurement & Baseline

- [ ] Add `cargo-tarpaulin` to Rust test infrastructure
- [ ] Add `coverage.py` to Python test infrastructure
- [ ] Run baseline coverage for Rust (measure, don't enforce yet)
- [ ] Run baseline coverage for Python (measure, don't enforce yet)
- [ ] Set initial targets (85% line coverage, 80% branch coverage)
- [ ] Document current coverage gaps

### Phase 1: Error & Property Testing

**Rust:**

- [ ] Add 5+ property tests (proptest)
- [ ] Add 10+ error scenario tests
- [ ] Enable coverage enforcement in CI (fail if <85%)

**Python:**

- [ ] Add 5+ property tests (@given with Hypothesis)
- [ ] Add 10+ error scenario tests
- [ ] Enable coverage enforcement in CI (fail if <85%)

### Phase 2: Expand Generated Code Testing

- [ ] Add boat-cli to CI matrix
- [ ] Create comprehensive smoke test script
- [ ] Add platform-specific tests (Linux, macOS, Windows)
- [ ] Add round-trip parity tests (CLI ↔ Python SDK)

### Phase 3: Multi-Language Testing (R)

- [ ] Add R SDK generation tests
- [ ] Add R snippet generation tests
- [ ] Verify R syntax (lintr + R CMD check)
- [ ] Test R↔Python SDK parity

### Phase 4: Continuous Coverage Monitoring

- [ ] Upload coverage reports to Codecov or similar
- [ ] Add coverage badges to README
- [ ] Set up trend tracking (coverage regression alerts)

---

## Testing Pyramid

```
                        ╱╲
                       ╱  ╲ E2E Tests (Multi-platform)
                      ╱    ╲ boat/goat on Linux + macOS
                     ╱──────╲
                    ╱        ╲
                   ╱          ╲ Integration Tests
                  ╱ Property   ╲ Generated code compiles
                 ╱ Tests #5+   ╲ SDK round-trip
                ╱────────────────╲
               ╱                  ╲
              ╱ Unit Tests (85%+)  ╲
             ╱ - Config parsing    ╲
            ╱ - Query building     ╲
           ╱ - Field resolution    ╲
          ╱ - Template rendering   ╲
         ╱──────────────────────────╲
```

---

## Files to Create/Modify

**New:**

- `scripts/check_coverage.sh` — Rust coverage threshold enforcement
- `src/core/query/proptest_tests.rs` — Property tests
- `src/core/tests/errors.rs` — Error scenario tests
- `scripts/test_generated_site.sh` — Multi-site smoke tests

**Modify:**

- `.github/workflows/ci.yml` — Add coverage measurement, boat-cli matrix, enforcement
- `Cargo.toml` — Add cargo-tarpaulin (dev dependency)
- `pyproject.toml` — Add pytest-cov, coverage configuration
- `tests/python/test_core.py` — Add @given property tests, error tests
- `tests/generated_goat_cli.rs` — Expand to boat-cli
- `proptest.toml` — Document property test configuration

---

## Success Criteria

By end of Phase 1:

- ✅ Rust: 85%+ line coverage, 80%+ branch coverage
- ✅ Python: 85%+ line coverage, 80%+ branch coverage
- ✅ Property tests: 5+ per language, all passing
- ✅ Error tests: 10+ per language, all passing
- ✅ CI: Coverage reports uploaded, thresholds enforced
- ✅ Boat-cli: Generated, compiled, and tested on Linux + macOS

By Phase 2 (R):

- ✅ All Phase 1 + R SDK generation tests
- ✅ R snippet generation tests
- ✅ R↔Python SDK parity confirmed

---

## References

### Tools & Docs

- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) — Rust coverage measurement
- [coverage.py](https://coverage.readthedocs.io/) — Python coverage measurement
- [proptest docs](https://docs.rs/proptest/) — Rust property testing
- [Hypothesis docs](https://hypothesis.readthedocs.io/) — Python property testing
- [Codecov](https://about.codecov.io/) — Coverage reporting (optional integration)

### Related Docs

- [.github/workflows/ci.yml](.github/workflows/ci.yml) — Current CI configuration
- [Cargo.toml](Cargo.toml) — Rust dependency management
- [pyproject.toml](pyproject.toml) — Python tool configuration
- [tests/python/conftest.py](tests/python/conftest.py) — Hypothesis configuration
- [proptest.toml](proptest.toml) — proptest configuration
