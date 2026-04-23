# R SDK — Quick Reference

This is a quick reference for the R SDK. **For comprehensive examples with all methods,
operators, and options, see the [QueryBuilder reference](docs/reference/query-builder.html) in the full documentation** or run `quarto preview docs/` in the generated project.

## Installation

```r
# Prerequisites
install.packages(c("devtools", "R6", "httr", "jsonlite", "yaml"))

# From the generated project
cd r/goat
R -e "devtools::install()"
```

## Quick Start

```r
library(goat)

# Create a query builder
qb <- QueryBuilder$new("taxon")

# Add filters and fields (methods chain with |>)
qb <- qb |>
  set_taxa(c("Mammalia"), filter_type = "tree") |>
  add_field("genome_size")

# Generate the URL (no network call)
cat(qb$to_url(), "\n")

# Or fetch results
results <- qb$search()
count <- qb$count()
```

## Core Operations

### Building Queries

| Operation               | Example                                          |
| ----------------------- | ------------------------------------------------ |
| **Create builder**      | `qb <- QueryBuilder$new("taxon")`                |
| **Set taxa**            | `$set_taxa(c("Mammalia"), filter_type = "tree")` |
| **Add field**           | `$add_field("genome_size")`                      |
| **Filter by attribute** | `$add_attribute("genome_size", "ge", "1G")`      |
| **Set result size**     | `$set_size(100)`                                 |
| **Sort results**        | `$set_sort("genome_size", "desc")`               |

### Fetching & Parsing

| Operation       | Example                      | Returns                               |
| --------------- | ---------------------------- | ------------------------------------- |
| **Validate**    | `qb$validate()`              | List of error strings (empty = valid) |
| **Count**       | `qb$count()`                 | Integer                               |
| **Search**      | `qb$search()`                | data.frame                            |
| **Search JSON** | `qb$search(format = "json")` | Character (JSON string)               |

### Response Parsing

```r
library(jsonlite)
from_goat <- function(qb) {
  cli_generator::parse_response_status(qb$search_raw())
}

# After fetching results
response_text <- qb$search_raw()
status_list <- fromJSON(cli_generator::parse_response_status(response_text))

# Access metadata
cat("Total hits:", status_list$hits, "\n")
cat("Took:", status_list$took, "ms\n")
```

## Examples

### Example 1: Simple Count

```r
library(goat)

qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia"), filter_type = "tree")

count <- qb$count()
cat(sprintf("Mammals: %d records\n", count))
```

### Example 2: Filter by Attribute

```r
# Find mammals with genome size >= 1 gigabase
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia"), filter_type = "tree") |>
  add_attribute("genome_size", "ge", "1G") |>
  add_field("genome_size")

errors <- qb$validate()
if (length(errors) == 0) {
  results <- qb$search()
  cat(sprintf("Found %d records\n", nrow(results)))
  print(head(results))
}
```

### Example 3: Multiple Operators

```r
# Mammals with genome size between 1G and 3G, with specific fields
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia"), filter_type = "tree") |>
  add_attribute("genome_size", "ge", "1G") |>
  add_attribute("genome_size", "le", "3G") |>
  add_field("genome_size") |>
  add_field("assembly_span") |>
  set_size(100)

results <- qb$search()
cat(sprintf("Found %d Mammals with 1-3G genomes\n", nrow(results)))
print(head(results))
```

### Example 4: Complex Query with Sorting

```r
# Insects with genome size info, sorted descending
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Insecta"), filter_type = "tree") |>
  add_attribute("genome_size", "exists") |>
  add_field("genome_size") |>
  add_field("assembly_span") |>
  set_sort("genome_size", "desc") |>
  set_size(50)

results <- qb$search()
cat(sprintf("Returned %d Insects with genome_size info\n", nrow(results)))
print(results[, c("taxon_name", "genome_size", "assembly_span")])
```

## Attribute Operators

When using `$add_attribute()`, the available operators depend on the field type:

| Operator   | Meaning               | Example                                                     |
| ---------- | --------------------- | ----------------------------------------------------------- |
| `"gt"`     | Greater than          | `$add_attribute("genome_size", "gt", "1G")`                 |
| `"ge"`     | Greater than or equal | `$add_attribute("genome_size", "ge", "1G")`                 |
| `"lt"`     | Less than             | `$add_attribute("genome_size", "lt", "5G")`                 |
| `"le"`     | Less than or equal    | `$add_attribute("genome_size", "le", "3G")`                 |
| `"eq"`     | Equals (enum fields)  | `$add_attribute("assembly_level", "eq", "complete genome")` |
| `"exists"` | Field has a value     | `$add_attribute("c_value", "exists")`                       |

See [QueryBuilder reference → Attribute filters](docs/reference/query-builder.html#attribute-filters) for the full list of operators and field-specific options.

## Named Parameters vs Operators

Some filters use named parameters instead of operators:

```r
# Named parameters (set_taxa, set_rank, set_assemblies, etc.)
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia"), filter_type = "tree") |>
  set_rank("species")

# Operators (add_attribute)
qb <- QueryBuilder$new("taxon") |>
  add_attribute("genome_size", "ge", "1G")
```

See [QueryBuilder reference](docs/reference/query-builder.html) for a complete list of all methods and their parameters.

## Advanced: Query Description & Code Generation

```r
# Get a human-readable description of the query
description <- qb$describe()
cat(description, "\n")

# Generate code snippets in other languages
snippets <- qb$snippet(languages = c("r", "python", "javascript"))
cat(snippets[["python"]], "\n")
cat(snippets[["javascript"]], "\n")
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
# Quick smoke test (load, instantiate, build URL)
bash scripts/validate_r_sdk.sh ./r/goat

# Deep validation (test $count(), $search(), parse_response_status with real API calls)
bash scripts/validate_r_sdk.sh --deep ./r/goat
```

If you encounter issues, the deep validation shows which methods are working with actual examples.

## Common Patterns

### Pipe vs. Step-by-step

```r
# Pipe style (R 4.1+)
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia")) |>
  add_field("genome_size") |>
  set_size(50)

# Step-by-step
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Mammalia"))
qb$add_field("genome_size")
qb$set_size(50)

# Both work identically
```

### Validate Before Searching

```r
qb <- QueryBuilder$new("taxon") |>
  set_taxa(c("Mammalia")) |>
  add_field("genome_size")

errors <- qb$validate()
if (length(errors) > 0) {
  cat("Validation errors:\n")
  print(errors)
} else {
  results <- qb$search()
  print(head(results))
}
```

### Working with Large Result Sets

```r
# Check count first
count <- qb$count()
cat(sprintf("Query will return ~%d records\n", count))

# Fetch in chunks if needed
qb_page1 <- qb |> set_size(1000) |> set_offset(0)
qb_page2 <- qb |> set_size(1000) |> set_offset(1000)

results1 <- qb_page1$search()
results2 <- qb_page2$search()
```

## Getting Help

1. **Check the examples** — See all methods with code samples in [docs/reference/query-builder.html](docs/reference/query-builder.html)
2. **Run deep validation** — Test all methods with real API calls: `bash scripts/validate_r_sdk.sh --deep ./r/goat`
3. **Read the tutorials** — [docs/quickstart.html](docs/quickstart.html) has step-by-step walkthroughs
4. **Inspect responses** — Use `jsonlite::fromJSON(parse_response_status(...))` to see metadata about your query results

---

**Last updated:** April 2026 | **SDK version:** See `packageVersion("goat")`
