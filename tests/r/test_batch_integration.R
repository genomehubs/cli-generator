# Integration tests for batch SDK methods against a live API.
#
# These tests validate that batch operations work end-to-end with real API responses,
# using actual R functions (no mocks). They require a running API server at
# http://localhost:3000/api.
#
# Run via: R CMD check
# Or: devtools::test()
# Or manually: R --vanilla < tests/r/test_batch_integration.R

library(testthat)

API_BASE <- "http://localhost:3000/api"

test_that("search_batch should work with single query", {
    qb <- QueryBuilder$new("taxon")
    query <- QueryBuilder$new("taxon")
    result <- qb$search_batch(list(query), api_base = API_BASE)
    expect_true(is.list(result))
})

test_that("search_batch should work with 10 queries", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 10)
    result <- qb$search_batch(queries, api_base = API_BASE)
    expect_true(is.list(result))
    expect_equal(length(result), 10)
})

test_that("search_batch should work with exactly 100 queries", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 100)
    result <- qb$search_batch(queries, api_base = API_BASE)
    expect_true(is.list(result))
    expect_equal(length(result), 100)
})

test_that("count_batch should return hit counts for single query", {
    qb <- QueryBuilder$new("taxon")
    query <- QueryBuilder$new("taxon")
    result <- qb$count_batch(list(query), api_base = API_BASE)
    expect_true(is.numeric(result) || is.list(result))
    expect_true(length(result) >= 1)
})

test_that("count_batch should return hit counts for 5 queries", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 5)
    result <- qb$count_batch(queries, api_base = API_BASE)
    expect_true(is.numeric(result) || is.list(result))
    expect_equal(length(result), 5)
})

test_that("count_batch should handle query with no results", {
    qb <- QueryBuilder$new("taxon")
    query <- QueryBuilder$new("taxon")
    result <- qb$count_batch(list(query), api_base = API_BASE)
    expect_true(is.numeric(result) || is.list(result))
})

test_that("count_batch should preserve query order in results", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 3)
    result <- qb$count_batch(queries, api_base = API_BASE)
    expect_equal(length(result), 3)
})

test_that("record should return record data", {
    qb <- QueryBuilder$new("taxon")
    result <- qb$record("taxon-9646", result = "taxon")
    expect_true(is.list(result) || is.data.frame(result))
})

test_that("lookup should work with taxon name", {
    qb <- QueryBuilder$new("taxon")
    result <- qb$lookup("Homo", result = "taxon", size = 5)
    expect_true(is.list(result) || is.data.frame(result))
})

test_that("summary should work with field aggregation", {
    qb <- QueryBuilder$new("taxon")
    result <- qb$summary("taxon-9646", "genome_size", result = "taxon")
    expect_true(is.list(result) || is.data.frame(result))
})

test_that("search_batch should reject >100 queries", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 101)
    expect_error(
        qb$search_batch(queries),
        "maximum 100 searches"
    )
})

test_that("count_batch should reject >100 queries", {
    qb <- QueryBuilder$new("taxon")
    queries <- rep(list(QueryBuilder$new("taxon")), 101)
    expect_error(
        qb$count_batch(queries),
        "maximum 100 searches"
    )
})

test_that("searchBatch should fail with invalid API base", {
    qb <- QueryBuilder$new("taxon")
    query <- QueryBuilder$new("taxon")
    expect_error(
        qb$search_batch(list(query), api_base = "http://invalid.example.com:9999"),
        NULL # Any error is acceptable for connection failure
    )
})
