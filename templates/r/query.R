#' Query builder for {{ site_display_name }}
#'
#' Build API queries programmatically with method chaining.
#' All mutating methods return `self` invisibly for chaining.
#'
#' @section Methods:
#'
#' \describe{
#'   \item{\code{new(index)}}{Initialise a new query for an index (e.g., "taxon").}
#'   \item{\code{add_attribute(name, operator, value)}}{Add a filter on a field value.}
#'   \item{\code{set_taxa(..., filter_type = "name")}}{Filter by one or more taxon names.}
#'   \item{\code{add_field(name)}}{Select a specific field to return.}
#'   \item{\code{set_fields(names)}}{Replace the field selection.}
#'   \item{\code{set_size(size)}}{Set the maximum number of results per page.}
#'   \item{\code{set_page(page)}}{Set the 1-based page number.}
#'   \item{\code{add_sort(name, direction = "asc")}}{Sort results by a field.}
#'   \item{\code{to_query_yaml()}}{Serialise query state to YAML.}
#'   \item{\code{to_params_yaml()}}{Serialise execution parameters to YAML.}
#'   \item{\code{to_url(endpoint = "search")}}{Build and return the API URL (no network call).}
#'   \item{\code{count()}}{Fetch the count of matching records.}
#'   \item{\code{search(format = "tsv")}}{Fetch results; returns parsed content.}
#'   \item{\code{describe(field_metadata = NULL, mode = "concise")}}{Get a prose description.}
#'   \item{\code{snippet(languages = c("r"), site_name = "{{ site_name }}", sdk_name = "{{ r_package_name }}", api_base = "")}}{Generate code snippets.}
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
    index_name        = NA_character_,
    taxa_names        = character(0),
    taxa_filter_type  = "name",
    attributes        = list(),
    fields            = list(),
    sort_key          = NULL,
    sort_order        = "asc",
    size              = 10L,
    page              = 1L,
    include_estimates = TRUE,
    tidy              = FALSE,
    taxonomy          = "ncbi"
  ),
  public = list(
    #' @description Initialise a new query builder.
    #' @param index The index to query (e.g., "taxon", "assembly").
    initialize = function(index) {
      private$index_name        <- index
      private$taxa_names        <- character(0)
      private$taxa_filter_type  <- "name"
      private$attributes        <- list()
      private$fields            <- list()
      private$sort_key          <- NULL
      private$sort_order        <- "asc"
      private$size              <- 10L
      private$page              <- 1L
      private$include_estimates <- TRUE
      private$tidy              <- FALSE
      private$taxonomy          <- "ncbi"
      invisible(self)
    },

    #' @description Add an attribute (field value) filter.
    #' @param name The field name.
    #' @param operator Comparison operator (e.g., "eq", "ne", "gt", "ge", "lt", "le").
    #' @param value The value to compare against.
    add_attribute = function(name, operator, value) {
      private$attributes[[length(private$attributes) + 1]] <- list(
        name     = name,
        operator = operator,
        value    = as.character(value)
      )
      invisible(self)
    },

    #' @description Filter by taxa.
    #' @param ... One or more taxon names (character). Prefix with "!" for NOT filters.
    #' @param filter_type "tree" to include all descendants, "name" for exact match.
    set_taxa = function(..., filter_type = "name") {
      private$taxa_names       <- c(...)
      private$taxa_filter_type <- filter_type
      invisible(self)
    },

    #' @description Select a field to return in results.
    #' @param name The field name.
    add_field = function(name) {
      private$fields[[length(private$fields) + 1]] <- name
      invisible(self)
    },

    #' @description Replace the field selection.
    #' @param names A character vector of field names.
    set_fields = function(names) {
      private$fields <- as.list(names)
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
    add_sort = function(name, direction = "asc") {
      private$sort_key   <- name
      private$sort_order <- direction
      invisible(self)
    },

    #' @description Serialise the query state to a YAML string for the Rust engine.
    #' @return A character string.
    to_query_yaml = function() {
      doc <- list(index = private$index_name)

      if (length(private$taxa_names) > 0) {
        doc$taxa              <- as.list(private$taxa_names)
        doc$taxon_filter_type <- private$taxa_filter_type
      }

      if (length(private$attributes) > 0) {
        doc$attributes <- private$attributes
      }

      if (length(private$fields) > 0) {
        doc$fields <- lapply(private$fields, function(f) list(name = f))
      }

      yaml::as.yaml(doc)
    },

    #' @description Serialise execution parameters to a YAML string for the Rust engine.
    #' @return A character string.
    to_params_yaml = function() {
      # Build YAML manually to guarantee true/false, not R's yes/no.
      include_est <- if (isTRUE(private$include_estimates)) "true" else "false"
      tidy_val    <- if (isTRUE(private$tidy)) "true" else "false"
      lines <- c(
        paste0("size: ",              private$size),
        paste0("page: ",              private$page),
        paste0("include_estimates: ", include_est),
        paste0("tidy: ",              tidy_val),
        paste0("taxonomy: ",          private$taxonomy)
      )
      if (!is.null(private$sort_key)) {
        lines <- c(lines,
          paste0("sort_by: ",    private$sort_key),
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

    #' @description Fetch the count of records matching this query.
    #' @return An integer.
    count = function() {
      url      <- self$to_url("count")
      response <- httr::GET(url, httr::accept("application/json"))
      httr::stop_for_status(response)
      body <- httr::content(response, as = "parsed", type = "application/json")
      as.integer(body[["count"]] %||% 0L)
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
        sep  <- if (format == "tsv") "\t" else ","
        text <- httr::content(response, as = "text", encoding = "UTF-8")
        utils::read.table(text = text, header = TRUE, sep = sep,
                          stringsAsFactors = FALSE, quote = "\"")
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
    snippet = function(
        languages = c("r"),
        site_name = "{{ site_name }}",
        sdk_name  = "{{ r_package_name }}",
        api_base  = "{{ api_base }}"
    ) {
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

      selections <- vapply(private$fields, as.character, character(1))

      snapshot <- list(
        filters    = filters,
        sorts      = sorts,
        flags      = character(0),
        selections = selections,
        traversal  = NULL,
        summaries  = list()
      )

      snapshot_json <- jsonlite::toJSON(snapshot, null = "null", auto_unbox = FALSE)
      lang_str      <- paste(languages, collapse = ",")
      snippets_json <- render_snippet(snapshot_json, site_name, api_base, sdk_name, lang_str)
      jsonlite::fromJSON(snippets_json)
    }
  )
)
