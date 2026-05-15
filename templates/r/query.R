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
    api_base_url = "{{ api_base | safe }}",
    api_version = "{{ api_version | safe }}",
    ui_base_url = "{{ ui_base | safe }}",
    # YAML overrides set by from_v2_url(); take priority in to_query_yaml/to_params_yaml
    query_yaml_override = NULL,
    params_yaml_override = NULL,
    lineage_rank_summary = list(),
    named_queries = list(),

    # Return a copy of `fields` as a character vector, or character(0) if NULL.
    normalise_fields = function(fields) {
      if (is.null(fields)) character(0) else as.character(fields)
    }
  ),
  public = list(
    #' @description Initialise a new query builder.
    #' @param index The index to query (e.g., "taxon", "assembly").
    initialize = function(index) {
      private$index_name <- index
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
      private$lineage_rank_summary <- list()
      private$named_queries <- list()
      invisible(self)
    },

    #' @description Add an attribute (field value) filter.
    #' @param name The field name.
    #' @param operator Comparison operator (e.g., "eq", "ne", "gt", "ge", "lt", "le").
    #' @param value The value to compare against.
    #' @param modifiers Optional character vector of attribute modifiers.
    add_attribute = function(name, operator, value = NULL, modifiers = NULL) {
      entry <- list(name = name, operator = operator)
      if (!is.null(value) && length(value) > 0) {
        entry$value <- as.character(value)
      }
      if (!is.null(modifiers) && length(modifiers) > 0) {
        entry$modifiers <- as.list(modifiers)
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

    #' @description Register a named sub-query for chain substitution.
    #' @details Values in attribute filters may reference this query using
    #'   dot-notation, e.g. \code{add_attribute("taxon_id", "eq", "queryA.taxon_id")}.
    #' @param query_key Name for this sub-query, e.g. \code{"queryA"}.
    #' @param query_string Filter expression, e.g. \code{"assembly_span>1e9"} or
    #'   \code{"assembly--assembly_span>1e9"} (v2 cross-index format).
    #' @param index Target index for the sub-query. \code{NULL} inherits the parent index.
    #' @param limit Maximum results to fetch (default 500, max 10000).
    #' @param inherit_scope Whether to scope the sub-query inside the parent taxon tree.
    #' @return Invisibly \code{self}.
    chain_query = function(query_key, query_string, index = NULL, limit = NULL, inherit_scope = NULL) {
      spec <- list(filter_expr = query_string)
      if (!is.null(index)) spec$index <- index
      if (!is.null(limit)) spec$limit <- as.integer(limit)
      if (!is.null(inherit_scope)) spec$inherit_scope <- isTRUE(inherit_scope)
      private$named_queries[[query_key]] <- spec
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
        entry$modifiers <- as.list(modifiers)
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

    #' @description Set lineage rank summary aggregation specs.
    #' @param specs A list of spec lists, each with `rank` and `fields` entries.
    set_lineage_rank_summary = function(specs) {
      private$lineage_rank_summary <- lapply(specs, function(s) s)
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

    #' @description Restrict results to exactly the supplied taxon IDs.
    #' @details Injected as an ES \code{terms} filter ANDed with the main query.
    #'   Maximum 65,536 IDs (ES hard limit).
    #' @param taxon_ids Character vector of IDs to include.
    #' @return Invisibly \code{self}.
    set_id_set = function(taxon_ids) {
      private$id_set <- as.character(taxon_ids)
      invisible(self)
    },

    #' @description Specify which ID field to filter on when using \code{set_id_set}.
    #' @details Determines the ES field to filter on:
    #'   \itemize{
    #'     \item "taxon" → taxon_id
    #'     \item "assembly" → assembly_id
    #'     \item "sample" → sample_id
    #'     \item "feature" → feature_id
    #'   }
    #'   Defaults to the current index type if not specified.
    #' @param id_type One of "taxon", "assembly", "sample", "feature".
    #' @return Invisibly \code{self}.
    set_id_type = function(id_type) {
      private$id_type <- id_type
      invisible(self)
    },

    #' @description Serialise the query state to a YAML string for the Rust engine.
    #' @return A character string.
    to_query_yaml = function() {
      if (!is.null(private$query_yaml_override)) {
        return(private$query_yaml_override)
      }
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

      if (length(private$lineage_rank_summary) > 0) {
        doc$lineage_rank_summary <- private$lineage_rank_summary
      }

      if (length(private$named_queries) > 0) {
        doc$named_queries <- private$named_queries
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
      if (!is.null(private$params_yaml_override)) {
        return(private$params_yaml_override)
      }
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
      if (!is.null(private$id_set) && length(private$id_set) > 0) {
        lines <- c(lines, "id_set:")
        for (id in private$id_set) {
          lines <- c(lines, paste0("  - ", id))
        }
      }
      if (!is.null(private$id_type)) {
        lines <- c(lines, paste0("id_type: ", private$id_type))
      }
      paste(c(lines, ""), collapse = "\n")
    },

    #' @description Build the API URL for this query without making a network call.
    #' @param endpoint API endpoint name (default: "search").
    #' @return A character string containing the full URL.
    to_v2_url = function(endpoint = "search") {
      build_url(self$to_query_yaml(), self$to_params_yaml(), endpoint)
    },

    #' @description Build the API URL (deprecated; use \code{to_v2_url}).
    #' @param endpoint API endpoint name (default: "search").
    #' @return A character string containing the full URL.
    to_url = function(endpoint = "search") {
      .Deprecated("to_v2_url")
      self$to_v2_url(endpoint)
    },

    #' @description Reconstruct a builder from a v2 API or UI URL.
    #' Detects whether the URL is a search or report URL and returns the
    #' appropriate builder type. Report URLs return a \code{ReportBuilder}
    #' with an embedded query.
    #' @param url A full v2 API or UI URL string.
    #' @return A \code{QueryBuilder} for search URLs or a \code{ReportBuilder} for report URLs.
    from_v2_url = function(url) {
      is_report <- grepl("[?&]report=", url) || grepl("/report(\\?|$)", url)
      if (is_report) {
        raw <- report_yaml_from_url_params(url)
        triple <- jsonlite::fromJSON(raw, simplifyVector = FALSE)
        if (!is.null(triple$error)) stop(paste("from_v2_url:", triple$error))
        qb <- QueryBuilder$new(private$index_name)
        qb$.__enclos_env__$private$query_yaml_override <- triple[[1]]
        qb$.__enclos_env__$private$params_yaml_override <- triple[[2]]
        rb <- ReportBuilder$new("_placeholder")
        rb$.__enclos_env__$private$report_yaml_override <- triple[[3]]
        rb$.__enclos_env__$private$embedded_query_builder <- qb
        return(rb)
      }
      raw <- query_yaml_from_url_params(url)
      pair <- jsonlite::fromJSON(raw, simplifyVector = FALSE)
      if (!is.null(pair$error)) stop(paste("from_v2_url:", pair$error))
      qb <- QueryBuilder$new(private$index_name)
      qb$.__enclos_env__$private$query_yaml_override <- pair[[1]]
      qb$.__enclos_env__$private$params_yaml_override <- pair[[2]]
      qb
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
      payload <- list(
        query_yaml = self$to_query_yaml(),
        params_yaml = self$to_params_yaml()
      )
      resp <- httr::POST(
        paste0(private$api_base_url, "/", private$api_version, "/count"),
        body = jsonlite::toJSON(payload, auto_unbox = TRUE),
        httr::content_type_json(),
        httr::accept_json()
      )
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      status <- tryCatch(
        jsonlite::fromJSON(parse_response_status(raw_text)),
        error = function(e) list(hits = 0)
      )
      hits <- status[["hits"]]
      if (is.null(hits)) hits <- 0
      as.integer(as.numeric(hits))
    },

    #' @description Fetch results for this query.
    #' @param format Response format: "tsv" (default), "csv", or "json".
    #' @return For tsv/csv: a data.frame. For json: the raw JSON text (pass to parse_search_json).
    search = function(format = "tsv") {
      if (format %in% c("tsv", "csv")) {
        url <- self$to_v2_url("search")
        sep <- if (format == "tsv") "\t" else ","
        accept_type <- if (format == "tsv") "text/tab-separated-values" else "text/csv"
        response <- httr::GET(url, httr::accept(accept_type))
        httr::stop_for_status(response)
        text <- httr::content(response, as = "text", encoding = "UTF-8")
        return(utils::read.table(
          text = text, header = TRUE, sep = sep,
          stringsAsFactors = FALSE, quote = '"'
        ))
      }
      # JSON: POST to v3
      payload <- list(
        query_yaml = self$to_query_yaml(),
        params_yaml = self$to_params_yaml()
      )
      resp <- httr::POST(
        paste0(private$api_base_url, "/", private$api_version, "/search"),
        body = jsonlite::toJSON(payload, auto_unbox = TRUE),
        httr::content_type_json(),
        httr::accept_json()
      )
      httr::stop_for_status(resp)
      httr::content(resp, as = "text", encoding = "UTF-8")
    },

    #' @description Fetch all matching records using v3 cursor-based pagination.
    #' @param max_records Maximum total records (NULL = no limit).
    #' @return A list of record lists.
    search_all = function(max_records = NULL) {
      CHUNK_SIZE <- 1000L
      cap <- if (is.null(max_records)) Inf else as.numeric(max_records)
      all_records <- list()
      search_after <- NULL
      orig_size <- private$size
      self$set_size(CHUNK_SIZE)
      on.exit(self$set_size(orig_size), add = TRUE)

      repeat {
        payload <- list(
          query_yaml = self$to_query_yaml(),
          params_yaml = self$to_params_yaml()
        )
        if (!is.null(search_after)) {
          payload[["search_after"]] <- search_after
        }
        resp <- httr::POST(
          paste0(private$api_base_url, "/", private$api_version, "/search"),
          body = jsonlite::toJSON(payload, auto_unbox = TRUE),
          httr::content_type_json()
        )
        httr::stop_for_status(resp)
        raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
        resp_data <- jsonlite::fromJSON(raw_text, simplifyVector = FALSE)

        records <- jsonlite::fromJSON(
          parse_search_json(raw_text),
          simplifyVector = FALSE
        )
        remaining <- cap - length(all_records)
        all_records <- c(all_records, head(records, ceiling(remaining)))

        search_after <- resp_data[["search_after"]]
        total <- resp_data[["status"]][["hits"]] %||% 0
        if (is.null(search_after) || length(all_records) >= min(cap, total)) break
      }

      if (!is.null(max_records)) head(all_records, max_records) else all_records
    },

    #' @description Fetch results and return as flat records, optionally joining lineage summary columns.
    #' @param lineage_summary Optional pre-fetched lineage summary list. If NULL and
    #'   lineage_rank_summary specs are set, search() is called to obtain both results
    #'   and summary in one request.
    #' @return A list of flat record lists.
    to_flat_records = function(lineage_summary = NULL) {
      raw_text <- self$search(format = "json")
      # Without an explicit config the field types (numeric vs categorical) are unknown,
      # so only join lineage columns when the caller provides lineage_summary explicitly.
      # This matches Python's behavior.
      if (is.null(lineage_summary)) {
        return(jsonlite::fromJSON(parse_search_json(raw_text), simplifyVector = FALSE))
      }
      config_json <- jsonlite::toJSON(lineage_summary, auto_unbox = TRUE)
      jsonlite::fromJSON(
        parse_search_with_lineage_summary(raw_text, config_json),
        simplifyVector = FALSE
      )
    },

    #' @description Reshape flat records into long/tidy format.
    #' @param records Flat record list, JSON string, or NULL to call to_flat_records().
    #' @param lineage_summary Optional lineage summary for to_flat_records() when records is NULL.
    #' @return A list of tidy record lists.
    to_tidy_records = function(records = NULL, lineage_summary = NULL) {
      if (is.null(records)) {
        flat <- self$to_flat_records(lineage_summary = lineage_summary)
        records_json <- jsonlite::toJSON(flat, auto_unbox = TRUE)
      } else if (is.character(records)) {
        records_json <- records
      } else {
        records_json <- jsonlite::toJSON(records, auto_unbox = TRUE)
      }
      jsonlite::fromJSON(to_tidy_records(records_json), simplifyVector = FALSE)
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
        taxa         = private$taxa_names,
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

    #' @description Validate the query against the bundled field metadata.
    #' @param field_metadata Named list of field metadata.  Pass `NULL` (default)
    #'   to load from the bundled `inst/generated/field_meta.json`.
    #' @param validation_config Named list of validation configuration.  Pass `NULL`
    #'   (default) to load from the bundled `inst/generated/validation_config.json`.
    #' @param synonyms Named list mapping attribute synonyms to canonical names.
    #'   Pass `NULL` (default) for none.
    #' @return A character vector of error strings; an empty vector means the query is valid.
    validate = function(field_metadata = NULL, validation_config = NULL, synonyms = NULL) {
      # Load field metadata from the generated JSON file if not supplied by the caller.
      # field_meta.json is per-index: list(taxon = list(...), assembly = list(...))
      if (is.null(field_metadata) || length(field_metadata) == 0) {
        meta_path <- system.file("generated/field_meta.json", package = .packageName)
        if (nchar(meta_path) == 0) {
          pkg_dir <- tryCatch(dirname(attr(utils::packageDescription(.packageName), "file")), error = function(e) NULL)
          if (!is.null(pkg_dir)) {
            meta_path <- file.path(pkg_dir, "generated", "field_meta.json")
          }
        }
        all_meta <- if (nchar(meta_path) > 0 && file.exists(meta_path)) {
          tryCatch(
            jsonlite::fromJSON(meta_path, simplifyVector = FALSE, simplifyDataFrame = FALSE, simplifyMatrix = FALSE),
            error = function(e) list()
          )
        } else {
          list()
        }
        field_metadata <- all_meta[[private$index_name]]
      }

      meta_json <- if (is.null(field_metadata) || length(field_metadata) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(field_metadata, auto_unbox = TRUE, null = "null")
      }

      # Load validation config from the generated JSON file if not supplied.
      if (is.null(validation_config) || length(validation_config) == 0) {
        cfg_path <- system.file("generated/validation_config.json", package = .packageName)
        if (nchar(cfg_path) > 0 && file.exists(cfg_path)) {
          validation_config <- tryCatch(
            jsonlite::fromJSON(cfg_path, simplifyVector = FALSE, simplifyDataFrame = FALSE, simplifyMatrix = FALSE),
            error = function(e) NULL
          )
        }
      }

      config_json <- if (is.null(validation_config) || length(validation_config) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(validation_config, auto_unbox = TRUE, null = "null")
      }

      synonyms_json <- if (is.null(synonyms) || length(synonyms) == 0) {
        "{}"
      } else {
        jsonlite::toJSON(synonyms, auto_unbox = TRUE, null = "null")
      }

      validate_query_json(self$to_query_yaml(), meta_json, config_json, synonyms_json)
    },

    #' @description Run a report query against the v3 /report endpoint.
    #' @param report A \code{ReportBuilder} instance.
    #' @param api_base Base URL of the API (default: from package).
    #' @return Raw report list from the response.
    report = function(report, api_base = NULL) {
      if (is.null(api_base)) {
        api_base <- private$api_base_url
      }
      url <- paste0(api_base, "/", private$api_version, "/report")
      payload <- list(
        query_yaml = self$to_query_yaml(),
        params_yaml = self$to_params_yaml(),
        report_yaml = report$to_report_yaml()
      )
      if (!is.null(report$.__enclos_env__$private$.display)) {
        payload$display <- report$.__enclos_env__$private$.display
      }
      if (isTRUE(report$.__enclos_env__$private$.include_plot_spec)) {
        payload$include_plot_spec <- TRUE
      }
      resp <- httr::POST(url,
        body = jsonlite::toJSON(payload, auto_unbox = TRUE),
        httr::add_headers("Content-Type" = "application/json"),
        httr::accept("application/json")
      )
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"),
        simplifyVector = FALSE
      )
      if (!is.null(data$plot_spec)) return(data)
      data$report %||% data
    },

    #' @description Execute multiple searches in a single batch request.
    #' @param queries List of QueryBuilder objects.
    #' @param api_base Base URL of the API (default: from package).
    #' @return List of batch search results.
    search_batch = function(queries, api_base = NULL) {
      if (length(queries) > 100) {
        stop("maximum 100 searches per batch request")
      }

      if (is.null(api_base)) {
        api_base <- private$api_base_url
      }

      url <- paste0(api_base, "/", private$api_version, "/search/batch")
      payload <- list(
        searches = lapply(queries, function(q) {
          list(
            query_yaml = q$to_query_yaml(),
            params_yaml = q$to_params_yaml()
          )
        })
      )

      resp <- httr::POST(url,
        body = jsonlite::toJSON(payload, auto_unbox = TRUE),
        httr::add_headers("Content-Type" = "application/json"),
        httr::accept("application/json")
      )
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      data <- jsonlite::fromJSON(parse_batch_json(raw_text), simplifyVector = FALSE)
      data$results %||% list()
    },

    #' @description Get hit counts for multiple queries in a batch request.
    #' @param queries List of QueryBuilder objects.
    #' @param api_base Base URL of the API (default: from package).
    #' @return Numeric vector of hit counts.
    count_batch = function(queries, api_base = NULL) {
      if (length(queries) > 100) {
        stop("maximum 100 searches per batch request")
      }

      if (is.null(api_base)) {
        api_base <- private$api_base_url
      }

      url <- paste0(api_base, "/", private$api_version, "/count/batch")
      payload <- list(
        searches = lapply(queries, function(q) {
          list(
            query_yaml = q$to_query_yaml(),
            params_yaml = q$to_params_yaml()
          )
        })
      )

      resp <- httr::POST(url,
        body = jsonlite::toJSON(payload, auto_unbox = TRUE),
        httr::add_headers("Content-Type" = "application/json"),
        httr::accept("application/json")
      )
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      data <- jsonlite::fromJSON(parse_batch_json(raw_text), simplifyVector = FALSE)

      counts <- sapply(data$results %||% list(), function(r) {
        as.numeric(r$status$hits %||% 0)
      })
      if (length(counts) == 0) numeric(0) else counts
    },

    #' @description Fetch a single record by ID or identifier.
    #' @param record_id Record ID to fetch (required).
    #' @param result Result type (taxon|assembly|sample); defaults to index type.
    #' @return Parsed record object.
    record = function(record_id, result = NULL) {
      if (is.null(record_id) || record_id == "") {
        stop("record() requires a record_id parameter")
      }
      result_type <- if (is.null(result)) private$index_name else result

      params <- list(recordId = record_id, result = result_type)
      url <- paste0(private$api_base_url, "/", private$api_version, "/record?")
      query_string <- paste(names(params), sapply(params, as.character), sep = "=", collapse = "&")
      url <- paste0(url, query_string)

      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
    },

    #' @description Fetch up to 1,000 records by ID in a single POST request.
    #' @param record_ids Character vector of record IDs (max 1,000; required).
    #' @param result Result type (taxon|assembly|sample); defaults to index type.
    #' @return Parsed batch record response list with a \code{records} element.
    record_batch = function(record_ids, result = NULL) {
      if (is.null(record_ids) || length(record_ids) == 0) {
        stop("record_batch() requires a non-empty record_ids vector")
      }
      result_type <- if (is.null(result)) private$index_name else result

      payload <- jsonlite::toJSON(list(record_ids = as.list(record_ids), result = result_type), auto_unbox = TRUE)
      url <- paste0(private$api_base_url, "/", private$api_version, "/record/batch")

      resp <- httr::POST(url, body = payload, httr::content_type_json(), httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
    },

    #' @description Run a positional report (oxford / ribbon / painting / circos).
    #' @param report Sub-type: "oxford", "ribbon", "painting", or "circos".
    #' @param group_by Attribute key for shared marker (e.g. "busco_gene").
    #' @param assemblies Character vector of assembly IDs.
    #' @param feature_type Optional primary_type filter.
    #' @param window_size Regional binning in bp (NULL = individual positions).
    #' @param reorient Auto-orient comparison sequences (default TRUE).
    #' @param max_features Hard cap on features fetched (default 10000).
    #' @param cat Optional category field for colour.
    #' @param cat_opts Category axis options string (list values explicitly).
    #' @param filter List of attribute filter dicts (field, operator, value, target).
    #' @param regions Region config list (cat, name_to_cat, bounds, min_features, max_expansion).
    #' @param max_connections_per_group Hard cap on M:N connections per group.
    #' @return Raw report list from the response.
    positional = function(report, group_by, assemblies, feature_type = NULL,
                          window_size = NULL, reorient = TRUE, max_features = 10000L,
                          cat = NULL, cat_opts = NULL, filter = NULL, regions = NULL,
                          max_connections_per_group = NULL) {
      doc <- list(report = report, group_by = group_by, assemblies = as.list(assemblies))
      if (!is.null(feature_type)) doc$feature_type <- feature_type
      if (!is.null(window_size)) doc$window_size <- as.integer(window_size)
      if (!reorient) doc$reorient <- FALSE
      if (max_features != 10000L) doc$max_features <- as.integer(max_features)
      if (!is.null(cat)) doc$cat <- cat
      if (!is.null(cat_opts)) doc$cat_opts <- cat_opts
      if (!is.null(filter) && length(filter) > 0) doc$filter <- filter
      if (!is.null(regions)) doc$regions <- regions
      if (!is.null(max_connections_per_group)) doc$max_connections_per_group <- as.integer(max_connections_per_group)

      positional_yaml <- yaml::as.yaml(doc)
      payload <- jsonlite::toJSON(list(
        query_yaml = self$to_query_yaml(),
        positional_yaml = positional_yaml
      ), auto_unbox = TRUE)
      url <- paste0(private$api_base_url, "/", private$api_version, "/positional")
      resp <- httr::POST(url, body = payload, httr::content_type_json(), httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      data <- jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
      if (!is.null(data$report)) data$report else data
    },

    #' @description Oxford dot-plot (exactly 2 assemblies). Wrapper around positional().
    oxford = function(group_by, assemblies, feature_type = NULL, window_size = NULL,
                      reorient = TRUE, max_features = 10000L, cat = NULL, cat_opts = NULL,
                      filter = NULL, regions = NULL, max_connections_per_group = NULL) {
      self$positional("oxford", group_by, assemblies, feature_type = feature_type,
                      window_size = window_size, reorient = reorient,
                      max_features = max_features, cat = cat, cat_opts = cat_opts,
                      filter = filter, regions = regions,
                      max_connections_per_group = max_connections_per_group)
    },

    #' @description Ribbon/synteny report (N >= 2 assemblies). Wrapper around positional().
    ribbon = function(group_by, assemblies, feature_type = NULL, window_size = NULL,
                      reorient = TRUE, max_features = 10000L, cat = NULL, cat_opts = NULL,
                      filter = NULL, regions = NULL, max_connections_per_group = NULL) {
      self$positional("ribbon", group_by, assemblies, feature_type = feature_type,
                      window_size = window_size, reorient = reorient,
                      max_features = max_features, cat = cat, cat_opts = cat_opts,
                      filter = filter, regions = regions,
                      max_connections_per_group = max_connections_per_group)
    },

    #' @description Chromosome painting (1 assembly). Wrapper around positional().
    painting = function(group_by, assembly, feature_type = NULL, window_size = NULL,
                        max_features = 10000L, cat = NULL, cat_opts = NULL,
                        filter = NULL, regions = NULL, max_connections_per_group = NULL) {
      self$positional("painting", group_by, list(assembly), feature_type = feature_type,
                      window_size = window_size, max_features = max_features,
                      cat = cat, cat_opts = cat_opts, filter = filter, regions = regions,
                      max_connections_per_group = max_connections_per_group)
    },

    #' @description Hybrid positional report combining remote and local assembly data.
    #' @param report Sub-type: "oxford", "ribbon", or "painting".
    #' @param group_by Shared marker identifier (e.g. "busco_gene").
    #' @param local_files Named list or list of named lists, each with:
    #'   \describe{
    #'     \item{busco}{Full text of a BUSCO full_table.tsv (required)}
    #'     \item{assembly_id}{Label for the assembly (required)}
    #'     \item{fai}{Full text of a .fai index (optional)}
    #'     \item{lengths}{Full text of a two-column lengths TSV (optional)}
    #'   }
    #' @param remote_assemblies Optional character vector of API assembly IDs (reference).
    #' @param reorient Auto-orient comparison sequences (default TRUE).
    #' @param cat Category field for colour coding (optional).
    #' @param window_size Bin size in bp (NULL for individual positions).
    #' @param max_connections_per_group Cap on M:N connections (0 = default 25).
    #' @return Report list in the same format as positional().
    hybrid_positional = function(report, group_by, local_files,
                                 remote_assemblies = NULL, reorient = TRUE,
                                 cat = NULL, window_size = NULL,
                                 max_connections_per_group = 0L) {
      local_sets <- lapply(local_files, function(entry) {
        asm_id <- entry[["assembly_id"]]
        raw <- jsonlite::fromJSON(parse_busco_tsv(asm_id, entry[["busco"]]))
        if (!is.null(raw[["error"]])) {
          stop(paste("parse_busco_tsv failed for '", asm_id, "':", raw[["error"]]))
        }
        if (!is.null(entry[["fai"]])) {
          lengths_map <- jsonlite::fromJSON(parse_fai(entry[["fai"]]))
          if (!is.null(lengths_map[["error"]])) {
            stop(paste("parse_fai failed for '", asm_id, "':", lengths_map[["error"]]))
          }
          raw[["sequence_lengths"]] <- lengths_map
          raw[["lengths_derived"]] <- FALSE
        } else if (!is.null(entry[["lengths"]])) {
          lengths_map <- jsonlite::fromJSON(parse_lengths_tsv(entry[["lengths"]]))
          if (!is.null(lengths_map[["error"]])) {
            stop(paste("parse_lengths_tsv failed for '", asm_id, "':", lengths_map[["error"]]))
          }
          raw[["sequence_lengths"]] <- lengths_map
          raw[["lengths_derived"]] <- FALSE
        }
        raw
      })

      ws <- if (is.null(window_size)) 0L else as.integer(window_size)

      if (is.null(remote_assemblies) || length(remote_assemblies) == 0L) {
        result_json <- positional_from_features(
          jsonlite::toJSON(local_sets, auto_unbox = TRUE),
          report, reorient, if (is.null(cat)) "" else cat, ws,
          as.integer(max_connections_per_group), ""
        )
        result <- jsonlite::fromJSON(result_json)
        if (!is.null(result[["error"]])) {
          stop(paste("positional_from_features failed:", result[["error"]]))
        }
        return(result)
      }

      positional_doc <- list(
        report = report,
        group_by = group_by,
        assemblies = as.list(remote_assemblies)
      )
      if (!is.null(cat)) positional_doc[["cat"]] <- cat
      if (!is.null(window_size)) positional_doc[["window_size"]] <- window_size
      if (!reorient) positional_doc[["reorient"]] <- FALSE
      if (max_connections_per_group > 0L) {
        positional_doc[["max_connections_per_group"]] <- as.integer(max_connections_per_group)
      }

      positional_yaml <- yaml::as.yaml(positional_doc)
      url <- paste0(private$api_base_url, "/", private$api_version, "/positional")
      payload <- jsonlite::toJSON(list(
        query_yaml = self$to_query_yaml(),
        positional_yaml = positional_yaml
      ), auto_unbox = TRUE)
      resp_text <- private$post_json_raw(url, payload)
      remote_report <- jsonlite::fromJSON(resp_text)[["report"]]

      result_json <- hybrid_positional(
        jsonlite::toJSON(remote_report, auto_unbox = TRUE),
        jsonlite::toJSON(local_sets, auto_unbox = TRUE),
        reorient,
        as.integer(max_connections_per_group)
      )
      result <- jsonlite::fromJSON(result_json)
      if (!is.null(result[["error"]])) {
        stop(paste("hybrid_positional failed:", result[["error"]]))
      }
      result
    },

    #' @description Lookup records by alternative identifiers (autocomplete/search-as-you-type).
    #' @param search_term Search term for lookup (required).
    #' @param result Result type (taxon|assembly|sample); defaults to index type.
    #' @param size Number of results to return (default: 10).
    #' @return Parsed lookup result.
    lookup = function(search_term, result = NULL, size = 10) {
      if (is.null(search_term) || search_term == "") {
        stop("lookup() requires a search_term parameter")
      }
      result_type <- if (is.null(result)) private$index_name else result

      params <- list(searchTerm = search_term, result = result_type, size = as.character(size))
      url <- paste0(private$api_base_url, "/", private$api_version, "/lookup?")
      query_string <- paste(names(params), sapply(params, as.character), sep = "=", collapse = "&")
      url <- paste0(url, query_string)

      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
    },

    #' @description Resolve multiple search terms to record IDs in a single POST.
    #' @param lookups Character vector of search terms, or a list of named lists
    #'   each containing \code{search_term} (required), \code{result} (optional),
    #'   and \code{size} (optional).
    #' @param result Default result type for items that omit it (default: index type).
    #' @param size Default page size for items that omit it (default: 10).
    #' @return Parsed batch lookup response list.
    lookup_batch = function(lookups, result = NULL, size = 10) {
      if (is.null(lookups) || length(lookups) == 0) {
        stop("lookup_batch() requires a non-empty lookups argument")
      }
      default_result <- if (is.null(result)) private$index_name else result

      normalise_item <- function(item) {
        if (is.character(item)) {
          return(list(search_term = item, result = default_result, size = size))
        }
        list(
          search_term = item[["search_term"]],
          result = if (!is.null(item[["result"]])) item[["result"]] else default_result,
          size = if (!is.null(item[["size"]])) item[["size"]] else size
        )
      }

      normalised <- lapply(lookups, normalise_item)
      payload <- jsonlite::toJSON(list(lookups = normalised), auto_unbox = TRUE)
      url <- paste0(private$api_base_url, "/", private$api_version, "/lookup/batch")
      resp <- httr::POST(url, httr::content_type_json(), body = payload, encode = "raw")
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
    },

    #' @description Fetch a PhyloPic silhouette record for a single taxon.
    #' @param taxon_id NCBI taxon ID (required).
    #' @param taxonomy Taxonomy name (default: "ncbi").
    #' @return Silhouette record list, or NULL when no silhouette is found.
    phylopic = function(taxon_id, taxonomy = "ncbi") {
      if (is.null(taxon_id) || taxon_id == "") {
        stop("phylopic() requires a taxon_id parameter")
      }
      params <- list(taxon_id = taxon_id, taxonomy = taxonomy)
      url <- paste0(private$api_base_url, "/", private$api_version, "/phylopic?")
      query_string <- paste(names(params), sapply(params, as.character), sep = "=", collapse = "&")
      url <- paste0(url, query_string)

      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(parse_phylopic_json(raw_text), simplifyVector = FALSE)
    },

    #' @description Fetch PhyloPic silhouette records for multiple taxa in one request.
    #' @param taxon_ids Character vector of NCBI taxon IDs (1-200, required).
    #' @param taxonomy Taxonomy name (default: "ncbi").
    #' @return List of silhouette record lists each with a taxon_id element.
    phylopic_batch = function(taxon_ids, taxonomy = "ncbi") {
      if (is.null(taxon_ids) || length(taxon_ids) == 0) {
        stop("phylopic_batch() requires at least one taxon_id")
      }
      if (length(taxon_ids) > 200) {
        stop("phylopic_batch() accepts at most 200 taxon IDs per request")
      }
      url <- paste0(private$api_base_url, "/", private$api_version, "/phylopic/batch")
      payload <- jsonlite::toJSON(
        list(taxon_ids = as.list(taxon_ids), taxonomy = taxonomy),
        auto_unbox = TRUE
      )
      resp <- httr::POST(url,
        body = payload,
        httr::add_headers("Content-Type" = "application/json"),
        httr::accept("application/json")
      )
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(parse_phylopic_batch_json(raw_text), simplifyVector = FALSE)
    },

    #' @description Fetch aggregated metadata in a single request.
    #' @return List with indices, taxonomies, ranks, and versions elements.
    metadata = function() {
      url <- paste0(private$api_base_url, "/", private$api_version, "/metadata")
      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"), simplifyVector = FALSE)
      keys <- c("indices", "taxonomies", "ranks", "versions")
      data[intersect(keys, names(data))]
    },

    #' @description Return the list of available index names.
    #' @return Character vector of index names.
    indices = function() {
      url <- paste0(private$api_base_url, "/", private$api_version, "/metadata/indices")
      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"), simplifyVector = TRUE)
      data[["indices"]] %||% character(0)
    },

    #' @description Return field metadata for the given index.
    #' @param index Index name (e.g. "taxon" or "assembly") (required).
    #' @return Named list of field metadata.
    fields = function(index) {
      if (is.null(index) || index == "") stop("fields() requires an index name")
      url <- paste0(private$api_base_url, "/", private$api_version, "/metadata/fields?result=", utils::URLencode(index, reserved = TRUE))
      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"), simplifyVector = FALSE)
      data[["fields"]] %||% list()
    },

    #' @description Return the list of available taxonomy names.
    #' @return Character vector of taxonomy names.
    taxonomies = function() {
      url <- paste0(private$api_base_url, "/", private$api_version, "/metadata/taxonomies")
      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"), simplifyVector = TRUE)
      data[["taxonomies"]] %||% character(0)
    },

    #' @description Return the list of recognised taxonomic rank names.
    #' @return Character vector of rank names.
    ranks = function() {
      url <- paste0(private$api_base_url, "/", private$api_version, "/metadata/ranks")
      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      data <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"), simplifyVector = TRUE)
      data[["ranks"]] %||% character(0)
    },

    #' @description Fetch summary aggregations for specific fields.
    #' @param record_id Record ID to summarize (required).
    #' @param fields Comma-separated field names to summarize (required).
    #' @param result Result type (taxon|assembly|sample); defaults to index type.
    #' @param summary_types Summary types to compute (default: "min,max,mean").
    #' @return Parsed summary object.
    summary = function(record_id, fields, result = NULL, summary = "histogram") {
      #' @description Fetch summary aggregations for a field across a taxon clade.
      #' @param record_id Taxon ID whose clade is aggregated (required).
      #' @param fields Field name to aggregate (required).
      #' @param result Result type (default: index type).
      #' @param summary Aggregation type: \code{"histogram"} (default) or \code{"terms"}.
      #' @return Parsed summary response list.
      if (is.null(record_id) || record_id == "") {
        stop("summary() requires a record_id parameter")
      }
      if (is.null(fields) || fields == "") {
        stop("summary() requires a fields parameter")
      }
      result_type <- if (is.null(result)) private$index_name else result

      params <- list(recordId = record_id, result = result_type, fields = fields, summary = summary)
      url <- paste0(private$api_base_url, "/", private$api_version, "/summary?")
      query_string <- paste(names(params), sapply(params, as.character), sep = "=", collapse = "&")
      url <- paste0(url, query_string)

      resp <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(resp)
      raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
      jsonlite::fromJSON(raw_text, simplifyVector = FALSE)
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

#' @title ReportBuilder
#' @description Build report configurations for v3 /report POST calls.
#'
#' @details
#' Constructs the \code{report_yaml} that controls how a report query is
#' visualised.  Designed to be paired with a \code{QueryBuilder}:
#'
#' \preformatted{
#' rb <- ReportBuilder$new("histogram")$set_x("genome_size")$set_rank("species")
#' data <- qb$report(rb)
#' }
#'
#' @export
ReportBuilder <- R6::R6Class("ReportBuilder",
  private = list(
    .doc = NULL,
    # Set by QueryBuilder$from_v2_url() for report URLs
    report_yaml_override = NULL,
    embedded_query_builder = NULL,
    # Set via set_display(); passed as the `display` key in the POST body.
    .display = NULL,
    # Set via set_include_plot_spec(); requests a PlotSpec in the response.
    .include_plot_spec = FALSE
  ),
  public = list(
    #' @description Initialise the builder with a report type.
    #' @param report_type One of \code{"histogram"}, \code{"scatter"},
    #'   \code{"map"}, \code{"tree"}, \code{"countPerRank"}, \code{"sources"},
    #'   \code{"arc"}.
    initialize = function(report_type) {
      private$.doc <- list(report = report_type)
      invisible(self)
    },

    #' @description Set the X-axis field (histogram, scatter, arc reports).
    #' @param field Field name.
    #' @param opts Optional axis options string.
    set_x = function(field, opts = "") {
      private$.doc$x <- field
      if (nchar(opts) > 0) private$.doc$x_opts <- opts
      invisible(self)
    },

    #' @description Set the Y-axis field or fields (scatter reports).
    #' @param field Field name or character vector of field names.
    #' @param opts Optional axis options string.
    set_y = function(field, opts = "") {
      private$.doc$y <- field
      if (nchar(opts) > 0) private$.doc$y_opts <- opts
      invisible(self)
    },

    #' @description Set the category breakdown field.
    #' @param field Field name.
    #' @param opts Optional axis options string.
    set_cat = function(field, opts = "") {
      private$.doc$cat <- field
      if (nchar(opts) > 0) private$.doc$cat_opts <- opts
      invisible(self)
    },

    #' @description Set the query field (countPerRank reports).
    #' @param field Field name.
    set_query = function(field) {
      private$.doc$query <- field
      invisible(self)
    },

    #' @description Set the taxonomic rank to aggregate at.
    #' @param rank Rank string, e.g. \code{"species"}.
    set_rank = function(rank) {
      private$.doc$rank <- rank
      invisible(self)
    },

    #' @description Set the list of taxonomic ranks (countPerRank reports).
    #' @param ranks Character vector of ranks.
    set_ranks = function(ranks) {
      private$.doc$ranks <- as.list(ranks)
      invisible(self)
    },

    #' @description Set additional fields to include in results.
    #' @param fields Character vector of field names.
    set_fields = function(fields) {
      private$.doc$fields <- as.list(fields)
      invisible(self)
    },

    #' @description Filter by assembly/sample status.
    #' @param value Status filter string, e.g. \code{"0"}.
    set_status_filter = function(value) {
      private$.doc$status_filter <- value
      invisible(self)
    },

    #' @description Set the rank for category label aggregation.
    #' @param rank Rank string.
    set_cat_rank = function(rank) {
      private$.doc$cat_rank <- rank
      invisible(self)
    },

    #' @description Collapse monotypic nodes in tree reports.
    #' @param value Logical; default \code{TRUE}.
    set_collapse_monotypic = function(value = TRUE) {
      private$.doc$collapse_monotypic <- value
      invisible(self)
    },

    #' @description Preserve this rank when collapsing monotypic nodes.
    #' @param rank Rank string.
    set_preserve_rank = function(rank) {
      private$.doc$preserve_rank <- rank
      invisible(self)
    },

    #' @description Set the rank to count descendants at (tree reports).
    #' @param rank Rank string.
    set_count_rank = function(rank) {
      private$.doc$count_rank <- rank
      invisible(self)
    },

    #' @description Set the geographic location field (map reports).
    #' @param field Field name.
    set_location_field = function(field) {
      private$.doc$location_field <- field
      invisible(self)
    },

    #' @description Set the geohash resolution for map reports (1-12).
    #' @param resolution Integer resolution.
    set_hex_resolution = function(resolution) {
      private$.doc$hex_resolution <- as.integer(resolution)
      invisible(self)
    },

    #' @description Set the max map points before switching to hexbin mode.
    #' @param threshold Integer threshold.
    set_map_threshold = function(threshold) {
      private$.doc$map_threshold <- as.integer(threshold)
      invisible(self)
    },

    #' @description Set the max scatter points before switching to binned mode.
    #' @param threshold Integer threshold.
    set_scatter_threshold = function(threshold) {
      private$.doc$scatter_threshold <- as.integer(threshold)
      invisible(self)
    },

    # ── Arc report methods ───────────────────────────────────────────────────────────────

    #' @description Set the feature filter (numerator) for an arc report.
    #' @param term Filter expression, e.g. \code{"genome_size>3000000000"}.
    #' @return Invisibly \code{self}.
    set_feature = function(term) {
      private$.doc$feature <- term
      invisible(self)
    },

    #' @description Set the reference filter (denominator) for an arc report.
    #' @param term Filter expression, e.g. \code{"genome_size>0"}.
    #' @return Invisibly \code{self}.
    set_reference = function(term) {
      private$.doc$reference <- term
      invisible(self)
    },

    #' @description Set the context filter (enables arc2 ratio) for an arc report.
    #' @param term Filter expression for the broader backdrop.
    #' @return Invisibly \code{self}.
    set_context = function(term) {
      private$.doc$context <- term
      invisible(self)
    },

    #' @description Add a concentric ring to a multi-ring arc report.
    #' @param feature_term Filter for this ring's numerator.
    #' @param reference_term Override the outer reference for this ring only.
    #' @param label Human-readable label for this ring.
    #' @return Invisibly \code{self}.
    add_ring = function(feature_term, reference_term = NULL, label = NULL) {
      ring <- list(feature = feature_term)
      if (!is.null(reference_term)) ring$reference <- reference_term
      if (!is.null(label)) ring$label <- label
      if (is.null(private$.doc$rings)) private$.doc$rings <- list()
      private$.doc$rings[[length(private$.doc$rings) + 1]] <- ring
      invisible(self)
    },

    #' @description Run the same feature/reference arc once per taxonomic rank.
    #' @param ranks Character vector of rank names.
    #' @return Invisibly \code{self}.
    set_arc_ranks = function(ranks) {
      private$.doc$ranks <- as.list(ranks)
      invisible(self)
    },

    #' @description Set custom boundaries for a histogram axis (x, y, or cat).
    #' @details For numeric axes, boundaries define explicit breakpoints. For date
    #'   axes, provide ISO 8601 date strings or interval names ("week", "month",
    #'   "quarter").
    #' @param axis_role Axis to configure — one of \code{"x"}, \code{"y"}, or
    #'   \code{"cat"}.
    #' @param boundaries For numeric: numeric vector in ascending order.
    #'   For date: character vector of ISO 8601 strings or interval names.
    #' @param labels Optional character vector of custom bucket labels. Count
    #'   must equal \code{length(boundaries) - 1} for numeric, or the number
    #'   of resolved intervals for dates.
    #' @return Invisibly \code{self}.
    set_axis_boundaries = function(axis_role, boundaries, labels = NULL) {
      key <- paste0(axis_role, "_opts")
      if (is.null(private$.doc[[key]])) {
        private$.doc[[key]] <- list()
      }
      private$.doc[[key]]$boundaries <- boundaries
      if (!is.null(labels)) {
        private$.doc[[key]]$labels <- labels
      }
      invisible(self)
    },

    #' @description Set date-based intervals for a date-scaled axis.
    #' @details Convenience method for setting standard calendar intervals on a
    #'   date axis. Intervals are expanded server-side to boundaries for the
    #'   current time window.
    #' @param axis_role Axis to configure — one of \code{"x"}, \code{"y"}, or
    #'   \code{"cat"}.
    #' @param intervals Character vector of interval names, e.g.
    #'   \code{c("week", "month", "quarter")}.
    #' @return Invisibly \code{self}.
    set_axis_date_intervals = function(axis_role, intervals) {
      key <- paste0(axis_role, "_opts")
      if (is.null(private$.doc[[key]])) {
        private$.doc[[key]] <- list()
      }
      private$.doc[[key]]$boundaries <- list(intervals = as.list(intervals))
      invisible(self)
    },

    #' @description Set display/presentation options for this report.
    #' @details Accepts either a named list or a YAML string. The value is
    #'   passed as the \code{display} field in the API request and returned
    #'   in the response unchanged. Rendering is always client-side.
    #' @param value Named list or YAML string with display options such as
    #'   \code{title}, \code{width}, \code{height}, \code{color_scheme},
    #'   \code{x_label}, etc.
    #' @return Invisibly \code{self}.
    set_display = function(value) {
      private$.display <- value
      invisible(self)
    },

    #' @description Request a \code{plot_spec} field in the API response.
    #' @param value Whether to include the plot spec (default: \code{TRUE}).
    #' @return Invisibly \code{self}.
    set_include_plot_spec = function(value = TRUE) {
      private$.include_plot_spec <- value
      invisible(self)
    },

    #' @description Return a short English description of this report configuration.
    #' @return A phrase suitable for embedding in prose, e.g.
    #'   \code{"a histogram of genome size by species rank"}.
    describe = function() {
      describe_report_yaml(self$to_report_yaml())
    },

    #' @description Return the report configuration as a YAML string.
    to_report_yaml = function() {
      if (!is.null(private$report_yaml_override)) {
        return(private$report_yaml_override)
      }
      yaml::as.yaml(private$.doc)
    },

    #' @description Return a character vector of validation errors.
    #' @param field_meta Optional named list of field metadata.
    #' @return Character vector of error strings (empty = valid).
    validate = function(field_meta = NULL) {
      meta_json <- if (is.null(field_meta)) {
        "{}"
      } else {
        jsonlite::toJSON(field_meta, auto_unbox = TRUE)
      }
      jsonlite::fromJSON(validate_report_yaml(self$to_report_yaml(), meta_json))
    },

    #' @description Execute this report against a QueryBuilder's query.
    #' @param query_builder A \code{QueryBuilder} instance.
    #' @param api_base Base URL of the API (default: from QueryBuilder).
    #' @return Raw report list from the response.
    run = function(query_builder = NULL, api_base = NULL) {
      qb <- if (!is.null(query_builder)) query_builder else private$embedded_query_builder
      if (is.null(qb)) {
        stop("run() requires a QueryBuilder argument or a ReportBuilder created via QueryBuilder$from_v2_url()")
      }
      qb$report(self, api_base = api_base)
    }
  )
)

#' Build a PlotSpec from local delimited text content without an API call.
#'
#' Reads TSV/CSV content in-memory — no API call required.  Column types are
#' auto-detected: columns where every non-empty value is numeric become
#' numeric; everything else remains character.
#'
#' @param content Character scalar. Full text of the TSV/CSV file. Read from
#'   a file with \code{readr::read_file()} or \code{readLines()}.
#' @param report_type Character scalar. One of \code{"histogram"},
#'   \code{"scatter"}, or \code{"bar"}.  Defaults to \code{"histogram"}.
#' @param column_map Named character vector. Maps axis roles (\code{"x"},
#'   \code{"y"}) to column names in the file, e.g.
#'   \code{c(x = "genome_size", y = "c_value")}.  Pass \code{NULL} or
#'   \code{character(0)} for positional defaults (first column → x, second
#'   column → y).
#' @param display Named list of display options (title, width, height, etc.).
#'   Defaults to an empty list.
#' @param delimiter Character scalar. Field separator: \code{"\t"} for TSV,
#'   \code{","} for CSV.  Defaults to \code{"\t"}.
#' @return Named list representing the PlotSpec.
#' @export
local_plot_spec <- function(
    content,
    report_type = "histogram",
    column_map = NULL,
    display = list(),
    delimiter = "\t") {
  col_map <- if (is.null(column_map)) list() else as.list(column_map)
  col_map_json <- jsonlite::toJSON(col_map, auto_unbox = TRUE)
  display_json <- jsonlite::toJSON(display, auto_unbox = TRUE)
  raw <- local_plot_spec_json(content, report_type, col_map_json, display_json, delimiter)
  parsed <- jsonlite::fromJSON(raw, simplifyVector = FALSE)
  if (!is.null(parsed[["error"]])) {
    stop(parsed[["error"]])
  }
  parsed
}

#' Merge annotation lists into plot_spec data rows by a shared key.
#'
#' For each row in \code{plot_spec$data$rows} whose value for \code{join_key}
#' matches an annotation entry, the annotation's fields are added to the row
#' (annotation fields take precedence on key collision).  Rows with no
#' matching annotation are left unchanged.
#'
#' @param plot_spec Named list. A PlotSpec object (from the API or from
#'   \code{\link{local_plot_spec}}).
#' @param annotations List of named lists. Each must contain at least
#'   \code{join_key} and the fields to add.
#' @param join_key Character scalar. Name of the column used to match rows
#'   to annotation entries.
#' @return The modified \code{plot_spec} list (same object, modified in place).
#' @export
merge_annotations <- function(plot_spec, annotations, join_key) {
  index <- stats::setNames(
    annotations,
    vapply(annotations, function(a) as.character(a[[join_key]]), character(1L))
  )
  rows <- plot_spec[["data"]][["rows"]]
  if (is.null(rows)) {
    return(plot_spec)
  }
  plot_spec[["data"]][["rows"]] <- lapply(rows, function(row) {
    key_val <- as.character(row[[join_key]])
    if (!is.null(key_val) && key_val %in% names(index)) {
      modifyList(row, index[[key_val]])
    } else {
      row
    }
  })
  plot_spec
}
