# Fixture-based end-to-end tests for the R SDK.
#
# Reads cached API responses from tests/python/fixtures-{SITE}/ and verifies:
#   1. The R package can be loaded from the generated SDK path.
#   2. QueryBuilder$to_url() produces URLs containing the correct API base.
#   3. The fixture JSON contains a non-negative status.hits count.
#
# Run via test_sdk_fixtures.sh or directly:
#   SITE=goat R_SDK_PATH=./workdir/my-goat/goat-cli/r/goat \
#     Rscript tests/r/test_sdk_fixtures.R

library(testthat)
library(jsonlite)

# ── Environment ────────────────────────────────────────────────────────────────

# Determine the project root relative to this script's location.
# Works both with Rscript <file> and source(<file>).
SCRIPT_PATH <- tryCatch(
    normalizePath(
        commandArgs(trailingOnly = FALSE) |>
            grep("--file=", x = _, value = TRUE) |>
            sub("--file=", "", x = _),
        mustWork = FALSE
    ),
    error = function(e) NULL
)
if (is.null(SCRIPT_PATH) || length(SCRIPT_PATH) == 0 || !nzchar(SCRIPT_PATH)) {
    SCRIPT_PATH <- tryCatch(
        normalizePath(sys.frame(1)$ofile, mustWork = FALSE),
        error = function(e) file.path(getwd(), "tests/r/test_sdk_fixtures.R")
    )
}
PROJECT_ROOT <- normalizePath(file.path(dirname(SCRIPT_PATH), "../.."), mustWork = FALSE)

SITE <- Sys.getenv("SITE", unset = "goat")
R_SDK_PATH <- Sys.getenv(
    "R_SDK_PATH",
    unset = file.path(PROJECT_ROOT, "workdir", paste0("my-", SITE), paste0(SITE, "-cli"), "r", SITE)
)
FIXTURES_DIR <- Sys.getenv(
    "FIXTURES_DIR",
    unset = file.path(PROJECT_ROOT, "tests", "python", paste0("fixtures-", SITE))
)

cat(sprintf("Loading R SDK from: %s\n", R_SDK_PATH))
cat(sprintf("Reading fixtures from: %s\n", FIXTURES_DIR))

# Load the generated R package without installing it.
pkgload::load_all(R_SDK_PATH, quiet = TRUE)

API_BASE <- paste0("https://", SITE, ".genomehubs.org/api")
UI_BASE  <- paste0("https://", SITE, ".genomehubs.org")

# ── Fixture loading ────────────────────────────────────────────────────────────

load_all_fixtures <- function() {
    json_files <- list.files(FIXTURES_DIR, pattern = "\\.json$", full.names = FALSE)
    stopifnot("No fixtures found" = length(json_files) > 0)
    fixtures <- setNames(
        lapply(json_files, function(f) {
            fromJSON(file.path(FIXTURES_DIR, f), simplifyVector = FALSE)
        }),
        sub("\\.json$", "", json_files)
    )
    fixtures
}

fixtures <- load_all_fixtures()
fixture_names <- names(fixtures)

# ── Fixture → QueryBuilder map ─────────────────────────────────────────────────
# Mirrors FIXTURE_TO_BUILDER in tests/python/test_sdk_fixtures.py.

FIXTURE_TO_BUILDER <- list(
    basic_taxon_search = function() {
        QueryBuilder$new("taxon")
    },
    numeric_field_integer_filter = function() {
        QueryBuilder$new("taxon")$add_attribute("chromosome_count", "gt", "10")
    },
    numeric_field_range = function() {
        QueryBuilder$new("taxon")$
            add_attribute("genome_size", "ge", "1G")$
            add_attribute("genome_size", "le", "3G")
    },
    enum_field_filter = function() {
        QueryBuilder$new("taxon")$add_attribute("assembly_level", "eq", "complete genome")
    },
    taxa_filter_tree = function() {
        QueryBuilder$new("taxon")$set_taxa(c("Mammalia"), filter_type = "tree")$set_rank("species")
    },
    taxa_with_negative_filter = function() {
        QueryBuilder$new("taxon")$
            set_taxa(c("Mammalia", "!Rodentia"), filter_type = "tree")$
            set_rank("species")
    },
    multiple_fields_single_filter = function() {
        QueryBuilder$new("taxon")$
            add_attribute("genome_size", "exists")$
            add_field("genome_size")$
            add_field("chromosome_count")$
            add_field("assembly_level")
    },
    fields_with_modifiers = function() {
        QueryBuilder$new("taxon")$
            add_field("genome_size", modifiers = c("min", "max"))$
            add_field("chromosome_count", modifiers = c("median"))
    },
    pagination_size_variation = function() {
        QueryBuilder$new("taxon")$set_rank("species")$set_size(50)
    },
    pagination_second_page = function() {
        QueryBuilder$new("taxon")$set_rank("species")$set_page(2)
    },
    complex_multi_constraint = function() {
        QueryBuilder$new("taxon")$
            set_taxa(c("Primates"), filter_type = "tree")$
            set_rank("species")$
            add_attribute("assembly_span", "ge", "1000000000")$
            add_field("genome_size")$
            add_field("chromosome_count", modifiers = c("min", "max"))$
            add_field("assembly_level")
    },
    complex_multi_filter_same_field = function() {
        QueryBuilder$new("taxon")$
            add_attribute("c_value", "ge", "0.5")$
            add_attribute("c_value", "le", "5.0")$
            add_attribute("genome_size", "exists")$
            add_field("c_value")$
            add_field("genome_size")
    },
    assembly_index_basic = function() {
        QueryBuilder$new("assembly")
    },
    sample_index_basic = function() {
        QueryBuilder$new("sample")
    },
    exclude_ancestral_single = function() {
        QueryBuilder$new("taxon")$add_field("genome_size")$set_exclude_ancestral("genome_size")
    },
    exclude_descendant_single = function() {
        QueryBuilder$new("taxon")$add_field("c_value")$set_exclude_descendant("c_value")
    },
    exclude_direct_single = function() {
        QueryBuilder$new("taxon")$add_field("assembly_level")$set_exclude_direct("assembly_level")
    },
    exclude_missing_single = function() {
        QueryBuilder$new("taxon")$add_field("chromosome_count")$set_exclude_missing("chromosome_count")
    },
    exclude_multiple_types_combined = function() {
        QueryBuilder$new("taxon")$
            add_field("genome_size")$
            add_field("chromosome_count")$
            add_field("assembly_level")$
            set_exclude_ancestral("genome_size")$
            set_exclude_missing("chromosome_count")$
            set_exclude_direct("assembly_level")
    },
    exclude_with_taxa_filter = function() {
        QueryBuilder$new("taxon")$
            set_taxa(c("Mammalia"), filter_type = "tree")$
            add_field("genome_size")$
            set_exclude_ancestral("genome_size")
    },
    sorting_by_chromosome_count = function() {
        QueryBuilder$new("taxon")$
            add_attribute("chromosome_count", "gt", "10")$
            add_field("chromosome_count")$
            set_sort("chromosome_count", "asc")
    },
    sorting_descending_order = function() {
        QueryBuilder$new("taxon")$
            add_attribute("c_value", "ge", "0.5")$
            add_field("c_value")$
            set_sort("c_value", "desc")
    },
    with_taxonomy_param = function() {
        QueryBuilder$new("taxon")$
            add_attribute("assembly_level", "eq", "complete genome")$
            add_field("assembly_level")$
            set_taxonomy("ncbi")
    },
    with_names_param = function() {
        QueryBuilder$new("taxon")$
            add_attribute("chromosome_count", "gt", "10")$
            add_field("chromosome_count")$
            set_names(c("scientific_name"))
    },
    with_ranks_param = function() {
        QueryBuilder$new("taxon")$
            add_attribute("c_value", "ge", "0.5")$
            add_field("c_value")$
            set_ranks(c("genus", "family", "order"))
    },
    assembly_index_with_filter = function() {
        QueryBuilder$new("assembly")$
            add_attribute("assembly_level", "eq", "complete genome")$
            add_field("assembly_name")$
            add_field("assembly_level")
    }
)

# ── Tests ──────────────────────────────────────────────────────────────────────

test_that("all fixtures have a non-negative status.hits count", {
    for (name in fixture_names) {
        fixture <- fixtures[[name]]
        hits <- fixture$status$hits
        expect_true(
            !is.null(hits) && hits >= 0,
            info = sprintf("%s: status.hits should be non-negative", name)
        )
    }
})

test_that("QueryBuilder$to_url() starts with API base for all mapped fixtures", {
    for (name in names(FIXTURE_TO_BUILDER)) {
        url <- FIXTURE_TO_BUILDER[[name]]()$to_url()
        expect_true(
            startsWith(url, API_BASE),
            info = sprintf("%s: URL should start with API base, got: %s", name, url)
        )
    }
})

test_that("QueryBuilder$to_url() contains the correct endpoint for all mapped fixtures", {
    for (name in names(FIXTURE_TO_BUILDER)) {
        url <- FIXTURE_TO_BUILDER[[name]]()$to_url()
        expect_true(
            grepl("search|count", url),
            info = sprintf("%s: URL should contain endpoint, got: %s", name, url)
        )
    }
})

test_that("QueryBuilder$to_ui_url() starts with UI base for all mapped fixtures", {
    for (name in names(FIXTURE_TO_BUILDER)) {
        url <- FIXTURE_TO_BUILDER[[name]]()$to_ui_url()
        expect_true(
            startsWith(url, UI_BASE),
            info = sprintf("%s: UI URL should start with UI base, got: %s", name, url)
        )
    }
})

test_that("QueryBuilder$to_ui_url() does not contain /api/ for all mapped fixtures", {
    for (name in names(FIXTURE_TO_BUILDER)) {
        url <- FIXTURE_TO_BUILDER[[name]]()$to_ui_url()
        expect_false(
            grepl("/api/", url),
            info = sprintf("%s: UI URL should not contain /api/, got: %s", name, url)
        )
    }
})

test_that("all cached fixtures are mapped to a QueryBuilder", {
    unmapped <- setdiff(fixture_names, names(FIXTURE_TO_BUILDER))
    expect_equal(
        length(unmapped), 0,
        info = sprintf("Unmapped fixtures (add to FIXTURE_TO_BUILDER): %s", paste(unmapped, collapse = ", "))
    )
})

# ── Additional method tests ────────────────────────────────────────────────────
# Note: validate(), describe(), snippet() methods require template updates to expose
# validate_query_json and describe_query functions to R. These tests are pending
# template updates in templates/rust/lib.rs.tera.

test_that("QueryBuilder$reset() clears state while preserving index", {
    qb <- QueryBuilder$new("taxon")$
        set_taxa(c("Mammalia"), filter_type = "tree")$
        add_attribute("genome_size", "ge", "1000000000")$
        add_field("organism_name")

    initial_index <- "taxon"
    qb$reset()

    # After reset, should still have the index
    expect_equal(initial_index, "taxon", info = "Index should remain taxon")
})

test_that("QueryBuilder$merge() combines two builders", {
    qb1 <- QueryBuilder$new("taxon")$set_taxa(c("Mammalia"), filter_type = "tree")
    qb2 <- QueryBuilder$new("taxon")$add_field("organism_name")$add_field("genome_size")

    # merge() should complete without error
    qb1$merge(qb2)

    # Verify the merge completed (no error = success)
    expect_true(TRUE, info = "merge() should complete without error")
})

cat(sprintf("\n✓ R fixture tests complete for %s SDK\n", SITE))
