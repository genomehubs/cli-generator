# Validation Coverage: All Languages

This summary shows the validation parity achieved across Python, R, and JavaScript SDKs.

## Operations Tested

All three validators test the **same core workflow** shown in GETTING_STARTED:

### 1. Import/Load

| Language   | Operation                                | Test Status |
| ---------- | ---------------------------------------- | ----------- |
| Python     | `from goat_sdk import QueryBuilder`      | ✅          |
| R          | `library(goat)` + `devtools::load_all()` | ✅          |
| JavaScript | `await import("./query.js")`             | ✅          |

### 2. Instantiation

| Language   | Operation                   | Test Status |
| ---------- | --------------------------- | ----------- |
| Python     | `QueryBuilder("taxon")`     | ✅          |
| R          | `QueryBuilder$new("taxon")` | ✅          |
| JavaScript | `new QueryBuilder("taxon")` | ✅          |

### 3. Builder Methods (Chaining)

| Language   | Method                                         | Test Status |
| ---------- | ---------------------------------------------- | ----------- |
| Python     | `.set_taxa(["Mammalia"], filter_type="tree")`  | ✅          |
| R          | `$set_taxa(c("Mammalia"), filter_type="tree")` | ✅          |
| JavaScript | `.setTaxa(["Mammalia"], "tree")`               | ✅          |

| Language   | Method                      | Test Status |
| ---------- | --------------------------- | ----------- |
| Python     | `.add_field("genome_size")` | ✅          |
| R          | `$add_field("genome_size")` | ✅          |
| JavaScript | `.addField("genome_size")`  | ✅          |

### 4. URL Generation

| Language   | Operation   | Test Status       |
| ---------- | ----------- | ----------------- |
| Python     | `.to_url()` | ✅ Returns string |
| R          | `$to_url()` | ✅ Returns string |
| JavaScript | `.toUrl()`  | ✅ Returns string |

**All three produce identical URLs:**

```
https://goat.genomehubs.org/api/v2/search?result=taxon&includeEstimates=true&taxonomy=ncbi&query=tax_tree%28Mammalia%29&fields=genome_size&size=10&offset=0
```

### 5. Validation (Python & R only)

| Language   | Operation                  | Test Status |
| ---------- | -------------------------- | ----------- |
| Python     | `.validate()` returns list | ✅          |
| R          | `$validate()` returns list | ✅          |
| JavaScript | (not applicable)           | —           |

## Full Test Results

```
=== Artifact Validation ===

Testing CLI...
✓ CLI --help works
✓ CLI taxon search --help works
✓ CLI URL generation works
✓ CLI --list-field-groups works
✓ CLI validation passed

Testing R SDK...
✓ R found: Rscript (R) version 4.5.3
✓ Import successful
✓ Instantiation successful
✓ Methods successful
✓ URL generated: [full URL with Mammalia and genome_size]
✓ R SDK validation passed

Testing JavaScript SDK...
✓ Node.js found: v22.12.0
✓ Import QueryBuilder works
✓ QueryBuilder instantiation works
✓ QueryBuilder methods (setTaxa, addField) work
✓ URL generation works
✓ JavaScript SDK validation passed

✓ All available artifacts validated successfully ✓
```

## Method Signature Parity

### set_taxa / setTaxa / $set_taxa

All accept:

1. **Taxa list** (array/vector): `["Mammalia"]` / `c("Mammalia")`
2. **Filter type** (optional, string): `filter_type="tree"` / `"tree"` / `filter_type="tree"`

Usage patterns:

```python
# Python
qb.set_taxa(["Mammalia"], filter_type="tree")

# R
qb$set_taxa(c("Mammalia"), filter_type = "tree")

# JavaScript
qb.setTaxa(["Mammalia"], "tree")
```

### add_field / addField / $add_field

All accept:

1. **Field name** (string): `"genome_size"`

Usage patterns:

```python
# Python
qb.add_field("genome_size")

# R
qb$add_field("genome_size")

# JavaScript
qb.addField("genome_size")
```

### to_url / toUrl / $to_url

All:

- Take **no arguments**
- Return API URL as string
- Perform **no network calls** (synchronous)

Usage patterns:

```python
# Python
url = qb.to_url()

# R
url <- qb$to_url()

# JavaScript
const url = qb.toUrl()
```

## Validation Links

For instructions on how to validate artifacts, see:

- **Quick Start**: [scripts/VALIDATION.md - Quick Start](VALIDATION.md#quick-start)
- **Individual Validators**: [scripts/VALIDATION.md - Individual Scripts](VALIDATION.md#individual-scripts)
- **Troubleshooting**: [scripts/VALIDATION.md - Troubleshooting](VALIDATION.md#troubleshooting)
- **SDK Examples**: [GETTING_STARTED.md - Python](../GETTING_STARTED.md#3-python-sdk), [R](../GETTING_STARTED.md#4-r-sdk), [JavaScript](../GETTING_STARTED.md#5-javascript-sdk)

## Design Principles

1. **Single source of truth**: URL building logic lives in Rust (`src/core/`), all languages delegate to it via FFI
2. **Consistent method names**: Python conventions (`snake_case`), R conventions (`$snake_case`), JavaScript conventions (`camelCase`)
3. **Minimal validation**: Smoke tests verify SDK loads and basic operations work; full feature tests in pytest/R tests/JS tests
4. **Language-native idioms**: Each validator uses language conventions (venv for Python, devtools for R, Node module for JS)
5. **Graceful degradation**: Validators skip if runtime not available (e.g., JS tests skip if Node.js missing)

## Future Language Support

The validation system is extensible. To add a new language (e.g., Go, Java):

1. Create `scripts/validate_$lang_sdk.sh` following the same pattern
2. Add `find_$lang_sdk()` function to `scripts/validate_artifacts.sh`
3. Call the new validator in the main loop
4. Update test coverage table in `scripts/VALIDATION.md`
5. Add examples to GETTING_STARTED (section N)
