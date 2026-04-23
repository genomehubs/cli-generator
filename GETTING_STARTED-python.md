# Python SDK — Quick Reference

This is a quick reference for the Python SDK. **For comprehensive examples with all methods,
operators, and options, see the [QueryBuilder reference](docs/reference/query-builder.html) in the full documentation** or run `quarto preview docs/` in the generated project.

## Installation

```bash
# From a pre-built wheel
pip install goat_sdk-*.whl pyyaml

# Or, if you've generated a custom CLI
cd /path/to/my-site-cli
maturin develop --features extension-module
```

## Quick Start

```python
from goat_sdk.query import QueryBuilder

# Create a query builder
qb = QueryBuilder("taxon")

# Add filters and fields (methods chain)
qb = qb.set_taxa(["Mammalia"], filter_type="tree") \
        .add_field("genome_size")

# Generate the URL (no network call)
print(qb.to_url())

# Or fetch results
results = qb.search()
count = qb.count()
```

## Core Operations

### Building Queries

| Operation               | Example                                       |
| ----------------------- | --------------------------------------------- |
| **Create builder**      | `qb = QueryBuilder("taxon")`                  |
| **Set taxa**            | `.set_taxa(["Mammalia"], filter_type="tree")` |
| **Add field**           | `.add_field("genome_size")`                   |
| **Filter by attribute** | `.add_attribute("genome_size", "ge", "1G")`   |
| **Set result size**     | `.set_size(100)`                              |
| **Sort results**        | `.set_sort("genome_size", "desc")`            |

### Fetching & Parsing

| Operation               | Example              | Returns                        |
| ----------------------- | -------------------- | ------------------------------ |
| **Validate**            | `qb.validate()`      | List of errors (empty = valid) |
| **Count**               | `qb.count()`         | Integer                        |
| **Search**              | `qb.search()`        | List of dicts                  |
| **Search as DataFrame** | `qb.search_df()`     | pandas.DataFrame               |
| **Search as polars**    | `qb.search_polars()` | polars.DataFrame               |

### Response Parsing

```python
import json
from cli_generator import parse_response_status

# After fetching results
response = qb.search_raw()  # Get raw API response
status_json = json.loads(parse_response_status(json.dumps(response)))

# Access metadata
print(f"Total hits: {status_json['hits']}")
print(f"Took: {status_json['took']}ms")
```

## Examples

### Example 1: Simple Count

```python
from goat_sdk.query import QueryBuilder

qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree")
count = qb.count()
print(f"Mammals: {count} records")
```

### Example 2: Filter by Attribute

```python
# Find mammals with genome size >= 1 gigabase
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", "ge", "1G")
    .add_field("genome_size")
)

errors = qb.validate()
if not errors:
    results = qb.search()
    print(f"Found {len(results)} records")
```

### Example 3: Multiple Operators

```python
# Mammals with genome size between 1G and 3G, with specific fields
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", "ge", "1G")
    .add_attribute("genome_size", "le", "3G")
    .add_field("genome_size")
    .add_field("assembly_span")
    .set_size(100)
)

df = qb.search_df()
print(df.head())
```

### Example 4: Complex Query with Sorting

```python
# Insects with genome size info, sorted descending
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Insecta"], filter_type="tree")
    .add_attribute("genome_size", "exists")
    .add_field("genome_size")
    .add_field("assembly_span")
    .set_sort("genome_size", "desc")
    .set_size(50)
)

results = qb.search_df()
print(f"Returned {len(results)} Insects with genome_size info")
print(results[["taxon_name", "genome_size", "assembly_span"]])
```

## Attribute Operators

When using `.add_attribute()`, the available operators depend on the field type:

| Operator   | Meaning               | Example                                                     |
| ---------- | --------------------- | ----------------------------------------------------------- |
| `"gt"`     | Greater than          | `.add_attribute("genome_size", "gt", "1G")`                 |
| `"ge"`     | Greater than or equal | `.add_attribute("genome_size", "ge", "1G")`                 |
| `"lt"`     | Less than             | `.add_attribute("genome_size", "lt", "5G")`                 |
| `"le"`     | Less than or equal    | `.add_attribute("genome_size", "le", "3G")`                 |
| `"eq"`     | Equals (enum fields)  | `.add_attribute("assembly_level", "eq", "complete genome")` |
| `"exists"` | Field has a value     | `.add_attribute("c_value", "exists")`                       |

See [QueryBuilder reference → Attribute filters](docs/reference/query-builder.html#attribute-filters) for the full list of operators and field-specific options.

## Named Parameters vs Operators

Some filters use named parameters instead of operators:

```python
# Named parameters (set_taxa, set_rank, set_assemblies, etc.)
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .set_rank("species")
)

# Operators (add_attribute)
qb = (
    QueryBuilder("taxon")
    .add_attribute("genome_size", "ge", "1G")
)
```

See [QueryBuilder reference](docs/reference/query-builder.html) for a complete list of all methods and their parameters.

## Advanced: Query Description & Code Generation

```python
# Get a human-readable description of the query
description = qb.describe()
print(description)

# Generate code snippets in other languages
snippets = qb.snippet(site_name="goat", sdk_name="goat_sdk", languages=["r", "javascript"])
print(snippets["r"])
print(snippets["javascript"])
```

See [Quickstart → Description & Snippet Generation](docs/quickstart.html#description--code-snippets) for examples.

## Full Documentation

For the complete API reference:

1. **In the repo:**

   ```bash
   quarto preview docs/
   ```

   Opens an interactive preview in your browser.

2. **In artifacts:**
   The rendered HTML docs are included. Open `docs/index.html` in your browser.

3. **Static files:**
   - [docs/reference/query-builder.html](docs/reference/query-builder.html) — Complete method reference
   - [docs/reference/parse.html](docs/reference/parse.html) — Response parsing functions
   - [docs/quickstart.html](docs/quickstart.html) — Full tutorials with all methods

## Validation & Debugging

Use the validation scripts to verify the SDK works:

```bash
# Quick smoke test (import, instantiate, build URL)
bash scripts/validate_python_sdk.sh ./goat_sdk-*.whl

# Deep validation (test .count(), .search(), parse_response_status with real API calls)
bash scripts/validate_python_sdk.sh --deep ./goat_sdk-*.whl
```

If you encounter issues, the deep validation shows which methods are working with actual examples.

## Common Patterns

### Chain vs. Step-by-step

```python
# Chain style
qb = QueryBuilder("taxon").set_taxa(["Mammalia"]).add_field("genome_size").set_size(50)

# Step-by-step
qb = QueryBuilder("taxon")
qb.set_taxa(["Mammalia"])
qb.add_field("genome_size")
qb.set_size(50)

# Both work identically
```

### Validate Before Searching

```python
qb = QueryBuilder("taxon").set_taxa(["Mammalia"]).add_field("genome_size")

errors = qb.validate()
if errors:
    print(f"Validation errors: {errors}")
else:
    results = qb.search()
```

### DataFrame Output

```python
# pandas (default)
df = qb.search_df()

# polars (faster parsing for large results)
df = qb.search_polars()

# Raw JSON (if needed)
raw = qb.search(format="json")
```

## Getting Help

1. **Check the examples** — See all methods with code samples in [docs/reference/query-builder.html](docs/reference/query-builder.html)
2. **Run deep validation** — Test all methods with real API calls: `bash scripts/validate_python_sdk.sh --deep ./goat_sdk-*.whl`
3. **Read the tutorials** — [docs/quickstart.html](docs/quickstart.html) has step-by-step walkthroughs
4. **Inspect responses** — Use `json.loads(parse_response_status(...))` to see metadata about your query results

---

**Last updated:** April 2026 | **SDK version:** See `goat_sdk.__version__`
