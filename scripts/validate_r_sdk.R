#!/usr/bin/env Rscript
tryCatch(
  {
    # Load required packages
    library(devtools, quietly = TRUE)
    library(jsonlite, quietly = TRUE)

    args <- commandArgs(trailingOnly = TRUE)

    # When invoked with no args we act as an existence probe (called by the
    # shell wrapper). Exit success so the wrapper knows the helper script exists.
    if (length(args) < 1) {
      cat("R helper script present\n")
      quit(status = 0)
    }

    pkg_dir <- args[1]

    if (!dir.exists(pkg_dir)) stop("Package directory not found: ", pkg_dir)

    install.packages(c("R6", "httr", "jsonlite", "yaml"),
      repos = "https://cloud.r-project.org",
      quiet = TRUE
    )

    devtools::load_all(pkg_dir, quiet = TRUE)

    cat("Import successful\n")

    qb <- QueryBuilder$new("taxon")
    cat("Instantiation successful\n")

    qb$set_taxa(c("Mammalia"), filter_type = "tree")
    qb$add_field("genome_size")
    cat("Methods successful\n")

    url <- qb$to_url()
    cat("URL generated:", url, "\n")

    if (!grepl("api/v2/search", url)) stop("URL missing API endpoint")
    if (!grepl("taxonomy=ncbi", url)) stop("URL missing taxonomy parameter")
    if (!grepl("query=tax", url)) stop("URL missing taxa query")
    if (!grepl("genome_size", url)) stop("URL missing field")

    cat("✓ All R SDK tests passed\n")
  },
  error = function(e) {
    cat("✗ Error:", conditionMessage(e), "\n")
    quit(status = 1)
  }
)
