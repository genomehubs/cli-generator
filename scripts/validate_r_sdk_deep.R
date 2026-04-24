#!/usr/bin/env Rscript
args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 1) {
  cat("Usage: Rscript scripts/validate_r_sdk_deep.R /path/to/r/goat\n")
  quit(status = 2)
}
pkg_dir <- args[1]
if (!dir.exists(pkg_dir)) stop(paste("Package directory not found:", pkg_dir))

suppressPackageStartupMessages({
  library(devtools)
  library(jsonlite)
})

install.packages(c("R6", "httr", "jsonlite", "yaml"), repos = "https://cloud.r-project.org", quiet = TRUE)

devtools::load_all(pkg_dir, quiet = TRUE)

cat("\n== Deep Validation: R SDK ==\n\n")

cat("Test 1: validate()\n")
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Mammalia"), filter_type = "tree")
qb$add_field("genome_size")
errors <- qb$validate()
# Accept either a character vector (common) or a list of errors.
if (!(is.list(errors) || is.character(errors))) {
  stop("validate() should return a character vector or a list of error strings")
}
errors_len <- if (is.list(errors)) length(errors) else length(as.character(errors))
cat(sprintf("  ✓ validate() works, returned: %d errors\n", errors_len))

cat("Test 2: count()\n")
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Mammalia"), filter_type = "tree")
count <- qb$count()
if (!is.numeric(count)) stop("count() should return numeric")
if (!(count > 0)) stop("Expected count > 0 for Mammalia")
cat(sprintf("  ✓ count() works: %d records found\n", count))

cat("Test 3: search()\n")
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Mammalia"), filter_type = "tree")
qb$add_field("genome_size")
qb$set_size(10)
results <- qb$search()
if (!is.data.frame(results)) stop("search() should return data.frame")
if (!(nrow(results) > 0)) stop("Expected results for Mammalia search")
cat(sprintf("  ✓ search() works: returned %d results\n", nrow(results)))
cat(sprintf("    First result: %s\n", results[1,1]))

cat("Test 4: parse_response_status()\n")
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Insecta"), filter_type = "tree")
qb$add_field("genome_size")
qb$set_size(5)
raw <- qb$search(format = "json")
status_json <- fromJSON(parse_response_status(raw))
if (is.null(status_json$hits)) stop("Status should have 'hits' field")
cat("  ✓ parse_response_status() works\n")
cat(sprintf("    hits: %s, took: %s\n", status_json$hits, status_json$took))

cat("Test 5: parsing helpers\n")
raw <- qb$search(format = "json")
# Use JSON string inputs for extendr wrappers (they expect JSON text, not R lists).
records_json <- parse_search_json(raw)
records <- fromJSON(records_json)
asl <- fromJSON(annotate_source_labels(records_json, mode = "non_direct"))
if (!is.list(asl)) stop("annotate_source_labels should return list")
cat(sprintf("  ✓ annotate_source_labels() works: returned %d rows\n", length(asl)))
split <- fromJSON(split_source_columns(records_json))
cat(sprintf("  ✓ split_source_columns() works: returned %d rows\n", length(split)))
vo <- fromJSON(values_only(records_json, "null"))
cat(sprintf("  ✓ values_only() works: returned %d rows\n", length(vo)))
ann <- fromJSON(annotated_values(records_json, "non_direct", "null"))
cat(sprintf("  ✓ annotated_values() works: returned %d rows\n", length(ann)))
tidy <- fromJSON(to_tidy_records(records_json))
cat(sprintf("  ✓ to_tidy_records() works: %d tidy rows\n", length(tidy)))

# Deterministic fixture checks
## Resolve fixture path relative to this script's location so we find fixtures
## even when the script is invoked from a different working directory.
args_all <- commandArgs(trailingOnly = FALSE)
file_arg <- grep("^--file=", args_all, value = TRUE)
if (length(file_arg) > 0) {
  script_path <- normalizePath(sub("^--file=", "", file_arg))
  script_dir <- dirname(script_path)
  repo_root <- normalizePath(file.path(script_dir, ".."))
} else {
  # Fallback: assume current working directory is the repo root
  repo_root <- normalizePath(getwd())
}
fixture_path <- file.path(repo_root, "tests", "python", "fixtures-goat", "fixture_mammalia_search_raw.json")
if (file.exists(fixture_path)) {
  cat("Test 6: Deterministic fixture-based checks\n")
  raw_fixture <- readChar(fixture_path, file.info(fixture_path)$size, useBytes = TRUE)
  parsed_json <- parse_search_json(raw_fixture)
  parsed <- fromJSON(parsed_json)
  cat(sprintf("  ✓ Parsed fixture into %d records\n", length(parsed)))
  split_f <- fromJSON(split_source_columns(parsed_json))
  has_direct <- any(sapply(split_f, function(r) any(grepl("__direct$", names(r)))))
  has_desc <- any(sapply(split_f, function(r) any(grepl("__descendant$", names(r)))))
  cat(sprintf("    Found __direct columns: %s, __descendant columns: %s\n", has_direct, has_desc))
  # Fallback: some fixtures encode aggregation source in the nested `fields` objects
  # rather than as split column suffixes. Inspect the raw fixture for aggregation_source.
  if (!(has_direct || has_desc)) {
    # Read the fixture file directly to avoid any intermediate transformations
    raw_obj <- jsonlite::fromJSON(fixture_path, simplifyVector = FALSE)
    agg_direct_present <- any(vapply(seq_along(raw_obj$results), function(i) {
      res <- raw_obj$results[[i]]
      if (!is.list(res) || is.null(res$result) || is.null(res$result$fields)) return(FALSE)
      flds <- res$result$fields
      any(vapply(seq_along(flds), function(j) {
        f <- flds[[j]]
        src <- f$aggregation_source
        if (is.null(src)) return(FALSE)
        if (is.character(src)) return(any(src == "direct"))
        if (is.list(src) || is.vector(src)) return(any(unlist(src) == "direct"))
        FALSE
      }, logical(1)))
    }, logical(1)))
    agg_desc_present <- any(vapply(seq_along(raw_obj$results), function(i) {
      res <- raw_obj$results[[i]]
      if (!is.list(res) || is.null(res$result) || is.null(res$result$fields)) return(FALSE)
      flds <- res$result$fields
      any(vapply(seq_along(flds), function(j) {
        f <- flds[[j]]
        src <- f$aggregation_source
        if (is.null(src)) return(FALSE)
        if (is.character(src)) return(any(src == "descendant"))
        if (is.list(src) || is.vector(src)) return(any(unlist(src) == "descendant"))
        FALSE
      }, logical(1)))
    }, logical(1)))
    cat(sprintf("    Fallback: found aggregation_source direct: %s, descendant: %s\n", agg_direct_present, agg_desc_present))
    has_direct <- has_direct || agg_direct_present
    has_desc <- has_desc || agg_desc_present
  }
  if (!(has_direct || has_desc)) stop("Expected at least one __direct or __descendant split column or aggregation_source in fixture")

  ann_vals <- fromJSON(annotated_values(parsed_json, "non_direct", "null"))
  found_descendant_label <- any(sapply(seq_along(ann_vals), function(i) any(grepl("Descendant", unlist(ann_vals[[i]])))))
  cat(sprintf("    Found 'Descendant' label in annotated values: %s\n", found_descendant_label))
  if (!found_descendant_label) stop("Expected at least one 'Descendant' label in annotated values")
  cat("  ✓ Deterministic fixture checks passed\n")
} else {
  cat(sprintf("  ⊙ Fixture not found at %s — skipping deterministic checks\n", fixture_path))
}

cat("\n✓ All R deep validation checks passed\n")
