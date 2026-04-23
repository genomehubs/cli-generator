#!/bin/bash
# Validate R SDK by testing QueryBuilder operations
# Usage: bash scripts/validate_r_sdk.sh /path/to/r/package
# Example: bash scripts/validate_r_sdk.sh ./r/goat

set -e

if [ -z "$1" ]; then
    echo "Usage: bash scripts/validate_r_sdk.sh /path/to/r/package"
    exit 1
fi

R_PKG_DIR="$1"

# Normalize path to absolute for R
R_PKG_DIR="$(cd "$R_PKG_DIR" 2>/dev/null && pwd)" || {
    echo "✗ R package directory not found: $1"
    exit 1
}

echo "Testing R SDK..."

# Check if R is installed
if ! command -v Rscript &> /dev/null; then
    echo "⊙ R not found (skipping R SDK validation)"
    echo "   Install R from https://www.r-project.org/ to test"
    exit 0
fi

R_VERSION=$(Rscript --version 2>&1 | head -1)
echo "✓ R found: $R_VERSION"

# Test QueryBuilder operations in R
# Create a temporary R script file
R_SCRIPT_FILE=$(mktemp)
trap "rm -f $R_SCRIPT_FILE" EXIT

cat > "$R_SCRIPT_FILE" <<'REOF'
    tryCatch({
        # Load required packages
        library(devtools, quietly = TRUE)

        # Get the R package directory from command-line arguments
        args <- commandArgs(trailingOnly = TRUE)
        pkg_dir <- args[1]

        if (!dir.exists(pkg_dir)) {
            stop("Package directory not found: ", pkg_dir)
        }

        # Install package dependencies
        install.packages(c("R6", "httr", "jsonlite", "yaml"),
                        repos = "https://cloud.r-project.org",
                        quiet = TRUE)

        # Load the package from local source
        devtools::load_all(pkg_dir, quiet = TRUE)

        # Test 1: Load the library
        cat("Import successful\n")

        # Test 2: Instantiate QueryBuilder
        qb <- QueryBuilder$new("taxon")
        cat("Instantiation successful\n")

        # Test 3: Call builder methods
        qb$set_taxa(c("Mammalia"), filter_type = "tree")
        qb$add_field("genome_size")
        cat("Methods successful\n")

        # Test 4: Generate URL
        url <- qb$to_url()
        cat("URL generated:", url, "\n")

        # Verify URL contains expected parts
        if (!grepl("api/v2/search", url)) {
            stop("URL missing API endpoint")
        }
        if (!grepl("taxonomy=ncbi", url)) {
            stop("URL missing taxonomy parameter")
        }
        if (!grepl("query=tax", url)) {
            stop("URL missing taxa query")
        }
        if (!grepl("genome_size", url)) {
            stop("URL missing field")
        }

        cat("✓ All R SDK tests passed\n")

    }, error = function(e) {
        cat("✗ Error:", conditionMessage(e), "\n")
        quit(status = 1)
    })
REOF

R_RESULT=$(Rscript --vanilla "$R_SCRIPT_FILE" "$R_PKG_DIR" 2>&1)

if [ $? -eq 0 ]; then
    echo "$R_RESULT" | sed 's/^/  /'
    echo "✓ R SDK validation passed"
    exit 0
else
    echo "$R_RESULT" | sed 's/^/  /'
    echo "✗ R SDK validation failed"
    exit 1
fi
