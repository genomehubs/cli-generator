#' Query builder for {{ site_display_name }}
#'
#' Build API queries programmatically with method chaining.
#' All mutating methods return `self` invisibly for chaining.
#'
#' @section Methods:
#'
#' \describe{
#'   \item{\code{new(index)}}{Initialise a new query for an index (e.g., "taxon").}
#'   \item{\code{set_taxa(..., filter_type = "name")}}{Filter by one or more taxon names.}
#'   \item{\code{set_rank(rank)}}{Restrict to a taxonomic rank, e.g. "species".}
#'   \item{\code{set_assemblies(accessions)}}{Filter by assembly accession IDs.}
#'   \item{\code{set_samples(accessions)}}{Filter by sample accession IDs.}
#'   \item{\code{add_attribute(name, operator, value, modifiers = NULL)}}{Add a filter on a field value.}
#'   \item{\code{set_attributes(attributes)}}{Replace all attribute filters at once.}
#'   \item{\code{add_field(name, modifiers = NULL)}}{Select a specific field to return.}
#'   \item{\code{set_fields(fields)}}{Replace the field selection at once.}
#'   \item{\code{set_names(name_classes)}}{Set taxon name classes to include.}
#'   \item{\code{set_ranks(ranks)}}{Set lineage rank columns to include.}
#'   \item{\code{set_exclude_ancestral(fields)}}{Exclude ancestrally derived estimates.}
#'   \item{\code{add_exclude_ancestral(field)}}{Add a field to exclude ancestrally derived values for.}
#'   \item{\code{set_exclude_descendant(fields)}}{Exclude descendant-derived estimates.}
#'   \item{\code{add_exclude_descendant(field)}}{Add a field to exclude descendant-derived values for.}
#'   \item{\code{set_exclude_direct(fields)}}{Exclude directly estimated values.}
#'   \item{\code{add_exclude_direct(field)}}{Add a field to exclude direct estimates for.}
#'   \item{\code{set_exclude_missing(fields)}}{Exclude records with missing values.}
#'   \item{\code{add_exclude_missing(field)}}{Add a field to exclude records with missing values for.}
#'   \item{\code{set_exclude_derived(fields)}}{Exclude non-direct estimates (shorthand).}
#'   \item{\code{set_exclude_estimated(fields)}}{Exclude ancestral and missing (shorthand).}
#'   \item{\code{set_size(size)}}{Set the maximum number of results per page.}
#'   \item{\code{set_page(page)}}{Set the 1-based page number.}
#'   \item{\code{set_sort(name, direction = "asc")}}{Sort results by a field.}
#'   \item{\code{set_include_estimates(value)}}{Control whether estimated values are included.}
#'   \item{\code{set_taxonomy(taxonomy)}}{Set the taxonomy source (e.g. "ncbi").}
#'   \item{\code{to_query_yaml()}}{Serialise query state to YAML.}
#'   \item{\code{to_params_yaml()}}{Serialise execution parameters to YAML.}
#'   \\item{\\code{to_url(endpoint = \"search\")}}{Build and return the API URL (no network call).}\n#'   \\item{\\code{to_ui_url(endpoint = \"search\")}}{Build and return the UI URL (no network call).}
#'   \item{\code{count()}}{Fetch the count of matching records.}
#'   \item{\code{search(format = "tsv")}}{Fetch results; returns parsed content.}
#'   \item{\code{validate()}}{Validate the query; returns a character vector of error messages.}
#'   \item{\code{describe(field_metadata = NULL, mode = "concise")}}{Get a prose description.}
#'   \item{\code{snippet(languages, site_name, sdk_name, api_base)}}{Generate code snippets.}
#'   \item{\code{reset()}}{Clear query state, preserving index and params.}
#'   \item{\code{merge(other)}}{Merge non-default state from another builder into this one.}
#'   \item{\code{combine(...)}}{Create a new builder merged from multiple builders (static method).}
#' }
#'
#' @examples
#' \dontrun{
#' qb <- QueryBuilder$new("taxon")$
#'   set_taxa(c("Mammalia"), filter_type = "tree")$
#'   add_attribute("genome_size", "ge", "1000000000")$
#'   add_field("assembly_span")
#'
#' # URL without a network call
#' qb$to_url()
#'
#' # Prose description
#' qb$describe()
#'
#' # Count matching records
#' qb$count()
#' }
#'
#' @export
QueryBuilder <- R6::R6Class(
  "QueryBuilder",
  private = list(
    index_name = NA_character_,
    taxa_names = character(0),
    taxa_filter_type = "name",
    rank_name = NULL,
    assemblies = character(0),
    samples = character(0),
    names_list = character(0),
    ranks_list = character(0),
    exclude_ancestral = character(0),
    exclude_descendant = character(0),
    exclude_direct = character(0),
    exclude_missing = character(0),
    attributes = list(),
    fields = list(),
    sort_key = NULL,
    sort_order = "asc",
    size = 10L,
    page = 1L,
    include_estimates = TRUE,
    tidy = FALSE,
    taxonomy = "ncbi",
    validation_level = "full",
    api_base_url = "{{ api_base }}",
    ui_base_url = "{{ ui_base }}",

    # Return a copy of `fields` as a character vector, or character(0) if NULL.
    normalise_fields = function(fields) {
      if (is.null(fields)) character(0) else as.character(fields)
    }
  ),
  public = list(
    #' @description Initialise a new query builder.
    #' @param index The index to query (e.g., "taxon", "assembly").
    #' @param validation_level Validation mode for \code{validate()}: \code{"full"} (default)
    #'   attempts v3 API metadata endpoints before falling back to local files;
    #'   \code{"partial"} uses only embedded local files.
    #' @param api_base Base URL for API metadata endpoints (v3+). Defaults to the
    #'   site API base.
    initialize = function(index,
                          validation_level = "full",
                          api_base = "{{ api_base }}") {
      private$index_name <- index
      private$validation_level <- validation_level
      private$api_base_url <- api_base
      private$taxa_names <- character(0)
      private$taxa_filter_type <- "name"
      private$rank_name <- NULL
      private$assemblies <- character(0)
      private$samples <- character(0)
      private$names_list <- character(0)
      private$ranks_list <- character(0)
      private$exclude_ancestral <- character(0)
      private$exclude_descendant <- character(0)
      private$exclude_direct <- character(0)
      private$exclude_missing <- character(0)
      private$attributes <- list()
      private$fields <- list()
      private$sort_key <- NULL
      private$sort_order <- "asc"
      private$size <- 10L
      private$page <- 1L
      private$include_estimates <- TRUE
      private$tidy <- FALSE
      private$taxonomy <- "ncbi"
      invisible(self)
    },

    #' @description Add an attribute (field value) filter.
    #' @param name The field name.
    #' @param operator Comparison operator (e.g., "eq", "ne", "gt", "ge", "lt", "le").
    #' @param value The value to compare against.
    #' @param modifiers Optional character vector of attribute modifiers.
    add_attribute = function(name, operator, value = NULL, modifiers = NULL) {
      entry <- list(
        name     = name,
        operator = operator,
        value    = as.character(value)
      )
      if (!is.null(modifiers) && length(modifiers) > 0) {
        entry$modifiers <- modifiers
      }
      private$attributes[[length(private$attributes) + 1]] <- entry
      invisible(self)
    },

    #' @description Replace all attribute filters at once.
    #' @param attributes A list of attribute filter items, each a named list with name, operator, value.
    set_attributes = function(attributes) {
      private$attributes <- as.list(attributes)
      invisible(self)
    },

    #' @description Filter by taxa.
    #' @param taxa A character vector of taxon names. Prefix with "!" for NOT filters.
    #' @param filter_type "tree" to include all descendants, "name" for exact match.
    set_taxa = function(taxa, filter_type = "name") {
      private$taxa_names <- taxa
      private$taxa_filter_type <- filter_type
      invisible(self)
    },

    #' @description Restrict results to a specific taxonomic rank.
    #' @param rank A rank name, e.g. "species", "genus".
    set_rank = function(rank) {
      private$rank_name <- rank
      invisible(self)
    },

    #' @description Filter by assembly accession IDs.
    #' @param accessions A character vector of accession IDs.
    set_assemblies = function(accessions) {
      private$assemblies <- accessions
      invisible(self)
    },

    #' @description Filter by sample accession IDs.
    #' @param accessions A character vector of accession IDs.
    set_samples = function(accessions) {
      private$samples <- accessions
      invisible(self)
    },

    #' @description Select a field to return in results.
    #' @param name The field name.
    #' @param modifiers Optional character vector of field modifiers.
    add_field = function(name, modifiers = NULL) {
      entry <- list(name = name)
      if (!is.null(modifiers) && length(modifiers) > 0) {
        entry$modifiers <- modifiers
      }
      private$fields[[length(private$fields) + 1]] <- entry
      invisible(self)
    },

    #' @description Replace the field selection at once.
    #' @param fields A list of field items, each a named list with at least a \code{name} entry.
    set_fields = function(fields) {
      private$fields <- as.list(fields)
      invisible(self)
    },

    #' @description Set taxon name classes to include in results.
    #' @param name_classes A character vector of name classes (e.g. "common").
    set_names = function(name_classes) {
      private$names_list <- name_classes
      invisible(self)
    },

    #' @description Set lineage rank columns to include in results.
    #' @param ranks A character vector of rank names.
    set_ranks = function(ranks) {
      private$ranks_list <- ranks
      invisible(self)
    },

    #' @description Exclude records with ancestrally derived estimated values.
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_ancestral = function(fields) {
      private$exclude_ancestral <- private$normalise_fields(fields)
      invisible(self)
    },

    #' @description Add a field to exclude ancestrally derived values for.
    #' @param field A single field name.
    add_exclude_ancestral = function(field) {
      if (!(field %in% private$exclude_ancestral)) {
        private$exclude_ancestral <- c(private$exclude_ancestral, field)
      }
      invisible(self)
    },

    #' @description Exclude records with descendant-derived estimated values.
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_descendant = function(fields) {
      private$exclude_descendant <- private$normalise_fields(fields)
      invisible(self)
    },

    #' @description Add a field to exclude descendant-derived values for.
    #' @param field A single field name.
    add_exclude_descendant = function(field) {
      if (!(field %in% private$exclude_descendant)) {
        private$exclude_descendant <- c(private$exclude_descendant, field)
      }
      invisible(self)
    },

    #' @description Exclude records with directly estimated values.
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_direct = function(fields) {
      private$exclude_direct <- private$normalise_fields(fields)
      invisible(self)
    },

    #' @description Add a field to exclude direct estimates for.
    #' @param field A single field name.
    add_exclude_direct = function(field) {
      if (!(field %in% private$exclude_direct)) {
        private$exclude_direct <- c(private$exclude_direct, field)
      }
      invisible(self)
    },

    #' @description Exclude records with missing values.
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_missing = function(fields) {
      private$exclude_missing <- private$normalise_fields(fields)
      invisible(self)
    },

    #' @description Add a field to exclude records with missing values for.
    #' @param field A single field name.
    add_exclude_missing = function(field) {
      if (!(field %in% private$exclude_missing)) {
        private$exclude_missing <- c(private$exclude_missing, field)
      }
      invisible(self)
    },

    #' @description Exclude all non-direct estimates (ancestral and descendant).
    #' Shorthand for set_exclude_ancestral() + set_exclude_descendant().
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_derived = function(fields) {
      normalised <- private$normalise_fields(fields)
      private$exclude_ancestral <- normalised
      private$exclude_descendant <- normalised
      invisible(self)
    },

    #' @description Exclude ancestral estimates and missing values.
    #' Shorthand for set_exclude_ancestral() + set_exclude_missing().
    #' @param fields A character vector of field names, or NULL to clear.
    set_exclude_estimated = function(fields) {
      normalised <- private$normalise_fields(fields)
      private$exclude_ancestral <- normalised
      private$exclude_missing <- normalised
      invisible(self)
    },

    #' @description Set the maximum number of results per page.
    #' @param size A positive integer.
    set_size = function(size) {
      private$size <- as.integer(size)
      invisible(self)
    },

    #' @description Set the 1-based page number.
    #' @param page A positive integer.
    set_page = function(page) {
      private$page <- as.integer(page)
      invisible(self)
    },

    #' @description Sort results by a field.
    #' @param name The field to sort by.
    #' @param direction "asc" or "desc".
    set_sort = function(name, direction = "asc") {
      private$sort_key <- name
      private$sort_order <- direction
      invisible(self)
    },

    #' @description Control whether estimated values are included.
    #' @param value Logical; \code{TRUE} to include estimates (default), \code{FALSE} to exclude.
    set_include_estimates = function(value) {
      private$include_estimates <- isTRUE(value)
      invisible(self)
    },

    #' @description Set the taxonomy source.
    #' @param taxonomy A taxonomy identifier string (e.g. "ncbi", "ott").
    set_taxonomy = function(taxonomy) {
      private$taxonomy <- taxonomy
      invisible(self)
    },

    #' @description Serialise the query state to a YAML string for the Rust engine.
    #' @return A character string.
    to_query_yaml = function() {
      doc <- list(index = private$index_name)

      if (length(private$taxa_names) > 0) {
        doc$taxa <- as.list(private$taxa_names)
        doc$taxon_filter_type <- private$taxa_filter_type
      }

      if (!is.null(private$rank_name)) {
        doc$rank <- private$rank_name
      }

      if (length(private$assemblies) > 0) {
        doc$assemblies <- as.list(private$assemblies)
      }

      if (length(private$samples) > 0) {
        doc$samples <- as.list(private$samples)
      }

      if (length(private$names_list) > 0) {
        doc$names <- as.list(private$names_list)
      }

      if (length(private$ranks_list) > 0) {
        doc$ranks <- as.list(private$ranks_list)
      }

      if (length(private$exclude_ancestral) > 0) {
        doc$excludeAncestral <- as.list(private$exclude_ancestral)
      }

      if (length(private$exclude_descendant) > 0) {
        doc$excludeDescendant <- as.list(private$exclude_descendant)
      }

      if (length(private$exclude_direct) > 0) {
        doc$excludeDirect <- as.list(private$exclude_direct)
      }

      if (length(private$exclude_missing) > 0) {
        doc$excludeMissing <- as.list(private$exclude_missing)
      }

      if (length(private$attributes) > 0) {
        doc$attributes <- private$attributes
      }

      if (length(private$fields) > 0) {
        doc$fields <- private$fields
      }

      yaml::as.yaml(doc)
    },

    #' @description Serialise execution parameters to a YAML string for the Rust engine.
    #' @return A character string.
    to_params_yaml = function() {
      # Build YAML manually to guarantee true/false, not R's yes/no.
      include_est <- if (isTRUE(private$include_estimates)) "true" else "false"
      tidy_val <- if (isTRUE(private$tidy)) "true" else "false"
      lines <- c(
        paste0("size: ", private$size),
        paste0("page: ", private$page),
        paste0("include_estimates: ", include_est),
        paste0("tidy: ", tidy_val),
        paste0("taxonomy: ", private$taxonomy)
      )
      if (!is.null(private$sort_key)) {
        lines <- c(
          lines,
          paste0("sort_by: ", private$sort_key),
          paste0("sort_order: ", private$sort_order)
        )
      }
      paste(c(lines, ""), collapse = "\n")
    },

    #' @description Build the API URL for this query without making a network call.
    #' @param endpoint API endpoint name (default: "search").
    #' @return A character string containing the full URL.
    to_url = function(endpoint = "search") {
      build_url(self$to_query_yaml(), self$to_params_yaml(), endpoint)
    },

    #' @description Build the UI URL for this query without making a network call.
    #' @param endpoint UI route name (default: "search").
    #' @return A character string containing the full UI URL.
    to_ui_url = function(endpoint = "search") {
      build_ui_url(self$to_query_yaml(), self$to_params_yaml(), endpoint)
    },

    #' @description Fetch the count of records matching this query.
    #' @return An integer.
    count = function() {
      url <- self$to_url("count")
      response <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(response)
      raw_text <- httr::content(response, as = "text", encoding = "UTF-8")
      status <- jsonlite::fromJSON(parse_response_status(raw_text))
      as.integer(status[["hits"]] %||% 0L)
    },

    #' @description Fetch results for this query.
    #' @param format Response format: "tsv" (default), "csv", or "json".
    #' @return Parsed content: a data.frame for tsv/csv, a list for json.
    search = function(format = "tsv") {
      url <- self$to_url("search")
      accept_type <- switch(format,
        tsv  = "text/tab-separated-values",
        csv  = "text/csv",
        json = "application/json",
        "application/json"
      )
      response <- httr::GET(url, httr::accept(accept_type))
      httr::stop_for_status(response)
      if (format %in% c("tsv", "csv")) {
        sep <- if (format == "tsv") "\t" else ","
        text <- httr::content(response, as = "text", encoding = "UTF-8")
        utils::read.table(
          text = text, header = TRUE, sep = sep,
          stringsAsFactors = FALSE, quote = "\""
        )
      } else {
        httr::content(response, as = "parsed", type = "application/json")
      }
    },

    #' @description Get a human-readable description of this query.
    #' @param field_metadata Optional named list of field metadata.
    #' @param mode "concise" (default) or "verbose".
    #' @return A character string.
    describe = function(field_metadata = NULL, mode = "concise") {
      meta_json <- if (is.null(field_metadata) || length(field_metadata) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(field_metadata, auto_unbox = TRUE)
      }
      describe_query(
        self$to_query_yaml(),
        self$to_params_yaml(),
        meta_json,
        mode
      )
    },

    #' @description Generate runnable code snippets in one or more languages.
    #' @param languages Character vector of language codes (default: `c("r")`).
    #' @param site_name Short site name for snippet context.
    #' @param sdk_name Package import name for snippet context.
    #' @param api_base API base URL.
    #' @return A named list mapping language codes to generated source code strings.
    snippet = function(languages = c("r"),
                       site_name = "{{ site_name }}",
                       sdk_name = "{{ r_package_name }}",
                       api_base = "{{ api_base }}") {
      filters <- lapply(private$attributes, function(attr) {
        list(
          jsonlite::unbox(attr$name),
          jsonlite::unbox(attr$operator),
          jsonlite::unbox(attr$value %||% "")
        )
      })

      sorts <- if (!is.null(private$sort_key)) {
        list(list(
          jsonlite::unbox(private$sort_key),
          jsonlite::unbox(private$sort_order)
        ))
      } else {
        list()
      }

      selections <- vapply(private$fields, function(f) f[["name"]], character(1))

      snapshot <- list(
        index        = jsonlite::unbox(private$index_name),
        taxa         = as.list(private$taxa_names),
        taxon_filter = jsonlite::unbox(private$taxa_filter_type),
        rank         = if (!is.null(private$rank_name)) jsonlite::unbox(private$rank_name) else NULL,
        filters      = filters,
        sorts        = sorts,
        flags        = character(0),
        selections   = selections,
        traversal    = NULL,
        summaries    = list()
      )

      snapshot_json <- jsonlite::toJSON(snapshot, null = "null", auto_unbox = FALSE)
      lang_str <- paste(languages, collapse = ",")
      snippets_json <- render_snippet(snapshot_json, site_name, api_base, sdk_name, lang_str)
      jsonlite::fromJSON(snippets_json)
    },

    #' @description Validate the query against field metadata.
    #' @param validation_level Override the instance setting for this call:
    #'   \code{"full"} (default) attempts to fetch metadata from v3 API endpoints;
    #'   \code{"partial"} uses only embedded local files.
    #' @param field_metadata Named list of field metadata (from `FieldMeta` objects).
    #'   Pass `NULL` to skip per-attribute checks.
    #' @param validation_config Named list of validation configuration (prefixes, name
    #'   classes, etc.). Pass `NULL` to use built-in defaults.
    #' @param synonyms Named list mapping attribute synonyms to canonical names.
    #'   Pass `NULL` for none.
    #' @return A character vector of error strings; an empty vector means the query is valid.
    validate = function(validation_level = NULL, field_metadata = NULL, validation_config = NULL, synonyms = NULL) {
      level <- if (!is.null(validation_level)) validation_level else private$validation_level
      meta_json <- if (is.null(field_metadata) || length(field_metadata) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(field_metadata, auto_unbox = TRUE)
      }
      config_json <- if (is.null(validation_config) || length(validation_config) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(validation_config, auto_unbox = TRUE)
      }
      synonyms_json <- if (is.null(synonyms) || length(synonyms) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(synonyms, auto_unbox = TRUE)
      }
      validate_query_json(self$to_query_yaml(), meta_json, config_json, synonyms_json)
    },

    #' @description Reset query filters, preserving index and execution parameters.
    reset = function() {
      private$taxa_names <- character(0)
      private$taxa_filter_type <- "name"
      private$rank_name <- NULL
      private$assemblies <- character(0)
      private$samples <- character(0)
      private$names_list <- character(0)
      private$ranks_list <- character(0)
      private$attributes <- list()
      private$fields <- list()
      private$sort_key <- NULL
      private$sort_order <- "asc"
      invisible(self)
    },

    #' @description Merge non-default state from another builder into this one.
    #' @param other A \code{QueryBuilder} instance.
    merge = function(other) {
      other_p <- other$.__enclos_env__$private
      if (length(other_p$taxa_names) > 0) {
        private$taxa_names <- other_p$taxa_names
        private$taxa_filter_type <- other_p$taxa_filter_type
      }
      if (!is.null(other_p$rank_name)) {
        private$rank_name <- other_p$rank_name
      }
      if (length(other_p$assemblies) > 0) {
        private$assemblies <- other_p$assemblies
      }
      if (length(other_p$samples) > 0) {
        private$samples <- other_p$samples
      }
      if (length(other_p$names_list) > 0) {
        private$names_list <- other_p$names_list
      }
      if (length(other_p$ranks_list) > 0) {
        private$ranks_list <- other_p$ranks_list
      }
      if (length(other_p$attributes) > 0) {
        private$attributes <- other_p$attributes
      }
      if (length(other_p$fields) > 0) {
        private$fields <- other_p$fields
      }
      if (!is.null(other_p$sort_key)) {
        private$sort_key <- other_p$sort_key
        private$sort_order <- other_p$sort_order
      }
      invisible(self)
    },

    #' @description Create a new builder that is the cumulative merge of multiple builders.
    #' @param ... Two or more \code{QueryBuilder} instances.
    #' @return A new \code{QueryBuilder}.
    combine = function(...) {
      builders <- list(...)
      if (length(builders) == 0) stop("combine() requires at least one QueryBuilder")
      result <- QueryBuilder$new(builders[[1]]$.__enclos_env__$private$index_name)
      for (b in builders) result$merge(b)
      result
    }
  )
)
