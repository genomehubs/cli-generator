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

        pkg <- "goat"
        getNamespaceExports(pkg)
        lsf.str(envir = asNamespace(pkg))
        exports <- getNamespaceExports(pkg)
        sapply(exports, function(n) {
            obj <- getExportedValue(pkg, n)
            paste(class(obj), collapse = "/")
        })
        cat("Exports:\n"); print(getNamespaceExports(pkg))

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
        if (!is.numeric(count)) stop("count() should return numeric")
        if (!(count > 0)) stop("Expected count > 0 for Mammalia")
        cat(sprintf("  ✓ count() works: %d records found\n", count))

        # Test 3: Search method (real API call)
        cat("Test 3: Search ($search())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_field("genome_size")
        qb$set_size(10)
        results <- qb$search()
        if (!is.data.frame(results)) stop("search() should return data.frame")
        if (!(nrow(results) > 0)) stop("Expected results for Mammalia search")
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
        if (!all(!is.na(results$genome_size))) stop("All results should have genome_size")
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
        if (!(nrow(results) > 0)) stop("Expected results in 1G-3G range")
        cat(sprintf("  ✓ Multiple filters work: %d results with 1G <= genome_size <= 3G\n", nrow(results)))

        # Test 6: Response parsing
        cat("Test 6: Response parsing (parse_response_status())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Insecta"), filter_type = "tree")
        qb$add_field("genome_size")
        qb$set_size(5)
        # Fetch raw JSON response for parsing (explicit json format)
        response <- qb$search(format = "json")
        status_json <- fromJSON(parse_response_status(response))
        if (is.null(status_json$hits)) stop("Status should have 'hits' field")
        if (is.null(status_json$took)) stop("Status should have 'took' field")
        cat(sprintf("  ✓ parse_response_status() works\n"))
        cat(sprintf("    Total hits: %d\n", status_json$hits))
        cat(sprintf("    Query time: %dms\n", status_json$took))

        # Test 7: Describe method
        cat("Test 7: Query description ($describe())\n")
        qb <- QueryBuilder$new("taxon")
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_attribute("genome_size", "ge", "1G")
        description <- qb$describe()
        if (!is.character(description)) stop("describe() should return character")
        if (!(nchar(description) > 0)) stop("Description should not be empty")
        cat(sprintf("  ✓ describe() works\n"))
        cat(sprintf("    %s...\n", substr(description, 1, 100)))

        # Test 8: Snippet generation
        cat("Test 8: Code snippet generation ($snippet())\n")
        snippets <- qb$snippet(languages = c("python", "r", "javascript"))
        if (!("python" %in% names(snippets))) stop("Should generate python snippet")
        if (!("r" %in% names(snippets))) stop("Should generate r snippet")
        if (!("javascript" %in% names(snippets))) stop("Should generate javascript snippet")
        cat(sprintf("  ✓ snippet() works for all languages\n"))

        cat("\n✓ All deep validation tests passed!\n\n")

    }, error = function(e) {
        cat("✗ Error:", conditionMessage(e), "\n")
        quit(status = 1)
    })
REOF

Rscript --vanilla "$TMP_R" "$R_PKG_DIR" || exit 1

echo "✓ R SDK deep validation passed"
