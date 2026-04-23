#!/bin/bash
# Deep validation of R SDK - comprehensive test with $count(), $search(), response parsing
# Usage: bash scripts/validate_r_sdk_deep.sh /path/to/r/goat
# This tests real API calls, so it's slower but more thorough

set -e

R_PKG_DIR="${1:?R package directory required}"

# Normalize path to absolute
R_PKG_DIR="$(cd "$R_PKG_DIR" 2>/dev/null && pwd)" || {
    echo "✗ R package directory not found: $1"
    exit 1
}

# Check if R is installed
if ! command -v Rscript &> /dev/null; then
    echo "⊙ R not found (skipping R SDK deep validation)"
    echo "   Install R from https://www.r-project.org/ to test"
    exit 0
fi

echo "Running deep validation for R SDK..."

# Create temp R script
TMP_R=$(mktemp)
trap "rm -f $TMP_R" EXIT

cat > "$TMP_R" << 'REOF'
    tryCatch({
        # Load required packages
        library(devtools, quietly = TRUE)
        library(jsonlite, quietly = TRUE)

        # Get the R package directory from command args
        args <- commandArgs(trailingOnly = TRUE)
        pkg_dir <- args[1]

        if (!dir.exists(pkg_dir)) {
            stop("Package directory not found: ", pkg_dir)
        }

        # Install dependencies
        install.packages(c("R6", "httr", "jsonlite", "yaml"),
                        repos = "https://cloud.r-project.org",
                        quiet = TRUE)

        # Load package
        devtools::load_all(pkg_dir, quiet = TRUE)

        cat("\n== Deep Validation: R SDK ==\n\n")

        # Test 1: Validate method
        cat("Test 1: Validation ($validate())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_field("genome_size")
        tryCatch({
          errors <- qb$validate()
          cat(sprintf("  ✓ validate() works, returned: %s\n", class(errors)[[1]]))
        }, error = function(e) {
          stop("validate() failed: ", conditionMessage(e))
        })

        # Test 2: Count method (real API call)
        cat("Test 2: Count ($count())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        count <- qb$count()
        stopifnot(is.numeric(count), "count() should return numeric")
        stopifnot(count > 0, "Expected count > 0 for Mammalia")
        cat(sprintf("  ✓ count() works: %d records found\n", count))

        # Test 3: Search method (real API call)
        cat("Test 3: Search ($search())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_field("genome_size")
        qb$set_size(10)
        results <- qb$search()
        stopifnot(is.data.frame(results), "search() should return data.frame")
        stopifnot(nrow(results) > 0, "Expected results for Mammalia search")
        cat(sprintf("  ✓ search() works: returned %d results\n", nrow(results)))
        cat(sprintf("    First result: %s\n", results[1, "taxon_name"]))

        # Test 4: Search with attribute filter
        cat("Test 4: Attribute filters ($add_attribute())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_attribute("genome_size", "ge", "1G")
        qb$add_field("genome_size")
        qb$set_size(10)
        results <- qb$search()
        stopifnot(all(!is.na(results$genome_size)), "All results should have genome_size")
        cat(sprintf("  ✓ add_attribute() works: %d results with genome_size >= 1G\n", nrow(results)))

        # Test 5: Multiple attribute filters
        cat("Test 5: Multiple attribute filters\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_attribute("genome_size", "ge", "1G")
        qb$add_attribute("genome_size", "le", "3G")
        qb$add_field("genome_size")
        qb$set_size(10)
        results <- qb$search()
        stopifnot(nrow(results) > 0, "Expected results in 1G-3G range")
        cat(sprintf("  ✓ Multiple filters work: %d results with 1G <= genome_size <= 3G\n", nrow(results)))

        # Test 6: Response parsing
        cat("Test 6: Response parsing (parse_response_status())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Insecta"), filter_type = "tree")
        qb$add_field("genome_size")
        qb$set_size(5)
        response <- qb$search_raw()
        status_json <- fromJSON(cli_generator::parse_response_status(response))
        stopifnot(!is.null(status_json$hits), "Status should have 'hits' field")
        stopifnot(!is.null(status_json$took), "Status should have 'took' field")
        cat(sprintf("  ✓ parse_response_status() works\n"))
        cat(sprintf("    Total hits: %d\n", status_json$hits))
        cat(sprintf("    Query time: %dms\n", status_json$took))

        # Test 7: Describe method
        cat("Test 7: Query description ($describe())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_attribute("genome_size", "ge", "1G")
        description <- qb$describe()
        stopifnot(is.character(description), "describe() should return character")
        stopifnot(nchar(description) > 0, "Description should not be empty")
        cat(sprintf("  ✓ describe() works\n"))
        cat(sprintf("    %s...\n", substr(description, 1, 100)))

        # Test 8: Snippet generation
        cat("Test 8: Code snippet generation ($snippet())\n")
        snippets <- qb$snippet(languages = c("python", "r", "javascript"))
        stopifnot("python" %in% names(snippets), "Should generate python snippet")
        stopifnot("r" %in% names(snippets), "Should generate r snippet")
        stopifnot("javascript" %in% names(snippets), "Should generate javascript snippet")
        cat(sprintf("  ✓ snippet() works for all languages\n"))

        cat("\n✓ All deep validation tests passed!\n\n")

    }, error = function(e) {
        cat("✗ Error:", conditionMessage(e), "\n")
        quit(status = 1)
    })
REOF

Rscript --vanilla "$TMP_R" "$R_PKG_DIR" || exit 1

echo "✓ R SDK deep validation passed"
