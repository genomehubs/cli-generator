# V2 API Analysis: OR Operators, searchPaginated, and Batch Operations

**Date Analyzed:** May 5, 2026
**Location:** `/local-api-copy/src/api/v2/`

---

## 1. Top-Level OR Operator for Query Combination

### Overview

The v2 API **does support** top-level OR operators for combining queries. OR queries are processed differently from AND queries and use Elasticsearch's `bool.should` clause with `minimum_should_match: 1`.

### Implementation Details

**File:** `functions/combineOrQueries.js`

#### Function: `combineOrQueries(queries)`

Combines multiple query objects using Elasticsearch OR (should) logic:

```javascript
export const combineOrQueries = (queries) => {
  // Validates and filters queries
  const validQueries = queries.filter((q) => q);

  if (validQueries.length === 1) return validQueries[0];

  // Build should clause from all queries
  const shouldClauses = validQueries
    .map((q, i) => {
      // Strip ALL inner_hits (deeply recursive)
      return i > 0 ? stripAllInnerHits(q.query) : q.query;
    })
    .filter((q) => q !== null);

  // Combine with bool query
  const combinedQuery = {
    bool: {
      should: shouldClauses,
      minimum_should_match: 1,
    },
  };

  return {
    size,
    from,
    query: combinedQuery,
    _source: _source, // from template
    sort: sort, // from template
    aggs: {
      /* merged from all queries */
    },
  };
};
```

**Key characteristics:**

- Uses `minimum_should_match: 1` (OR semantics)
- Merges aggregations from all queries
- Preserves sorting from the first query template
- **Strips ALL inner_hits** from combined queries to avoid nesting issues
- Returns size/from pagination parameters from first query

#### How OR Queries Are Detected and Processed

**File:** `functions/getResults.js`, function `generateQuery()`

```javascript
export const generateQuery = async (params) => {
  const { query } = params;

  // Detect OR pattern (case-insensitive)
  if (query && typeof query === "string" && query.match(/\s+or\s+/i)) {
    // Split by OR
    let orParts = query.split(/\s+or\s+/i).map((q) => q.trim());

    // Strip outer parentheses if matching
    orParts = orParts.map((part) => {
      if (part.startsWith("(") && part.endsWith(")")) {
        // Check for matching parentheses
        let depth = 0;
        let isMatching = true;
        for (let i = 0; i < part.length; i++) {
          if (part[i] === "(") depth++;
          if (part[i] === ")") depth--;
          if (depth === 0 && i < part.length - 1) {
            isMatching = false;
            break;
          }
        }
        if (isMatching && depth === 0) {
          return part.substring(1, part.length - 1);
        }
      }
      return part;
    });

    // Return async function that:
    // 1. Generates individual query for each OR part
    // 2. Builds Elasticsearch query structures via searchByTaxon()
    // 3. Combines with combineOrQueries()
    // 4. Executes combined query
    // 5. Processes results
  }

  // No OR pattern found - use standard single query generation
  return generateSingleQuery(params);
};
```

**OR Query Processing Workflow:**

1. Detect `OR` (case-insensitive, escaped)
2. Split query string on `OR`
3. For each part: call `generateSingleQuery()` → call `searchByTaxon()` → get ES query structure
4. Call `combineOrQueries(orQueryBodies)` to merge with `bool.should`
5. Execute combined query via `client.search()`
6. Process results like normal search results

### Query Syntax Examples

```
tax_rank(species) OR tax_rank(genus)
(assembly_level=complete genome) OR (assembly_level=chromosome)
tax_name(canis) OR tax_name(rosa)
```

---

## 2. Multi-Query Patterns in Same Query

### Newline-Separated Queries (AND within single query)

**File:** `functions/getResults.js`, function `generateSingleQuery()`

```javascript
if (query && query.match(/\n/) && query.split(/\n/)[1] > "") {
  multiTerm = query
    .toLowerCase()
    .split(/\n/)
    .map((v) => v.trim())
    .filter((v) => v > "");
}
```

**Behavior:** Queries separated by newlines are treated as separate search terms to be combined within `getRecordsByTaxon()` logic (AND semantics by default).

### AND-Separated Query Terms (within single query)

Queries with `AND` are split and validated separately:

```javascript
for (let term of query.split(/\s+and\s+/)) {
  // Parse and validate each term
  let { parts, validation, subset } = validateTerm(term, lookupTypes[result]);
  // ... add to filters, properties, etc.
}
```

---

## 3. searchPaginated Endpoint

**File:** `routes/searchPaginated.js`

### Key Differences from Standard `search` Endpoint

| Feature                | `search`                                    | `searchPaginated`                     |
| ---------------------- | ------------------------------------------- | ------------------------------------- |
| **Pagination Method**  | `from`/`size` (offset-based)                | `search_after` (cursor-based)         |
| **Use Case**           | Small-medium result sets                    | Large result sets (10K+)              |
| **Memory Efficiency**  | Lower (maintains state for each page)       | Higher (no server-side state)         |
| **Stability**          | Results can shift if docs are added/removed | Consistent with `search_after` token  |
| **Sort Support**       | Any field                                   | Requires sort for `search_after`      |
| **Response Structure** | Array in `hits`                             | Array in `hits` + `pagination` object |

### Implementation Details

```javascript
export const getSearchPaginated = async (req, res) => {
  const {
    query,
    result = "taxon",
    taxonomy,
    limit = 100, // Per-page limit
    searchAfter, // JSON array of sort values
    sortBy = undefined, // Sort field
    sortOrder = "asc", // Sort order
  } = req.query;

  // Validate limit (max 10,000)
  const pageSize = Math.min(Math.max(parseInt(limit) || 100, 1), 10000);

  // Use getResults with size=0 to validate query first
  const queryValidation = await getResults({
    ...req.query,
    size: 0,
  });

  // Build sort clause with tiebreaker
  const idField = `${result}_id`;
  const effectiveSortBy = sortBy || idField;
  const sortClause = [{ [effectiveSortBy]: sortOrder }];
  // Add tiebreaker (avoids sorting on special _id field)
  if (effectiveSortBy !== idField) {
    sortClause.push({ [idField]: sortOrder });
  }

  // Execute search
  const searchParams = {
    index: indexName({ result, taxonomy }),
    size: pageSize,
    body: resultsResponse.query,
    sort: sortClause,
    track_total_hits: false,
  };

  if (searchAfter) {
    searchParams.body.search_after = JSON.parse(searchAfter);
  }

  const response = await client.search(searchParams);
  const { hits } = response.body.hits;

  return {
    status: { success: true, hits: hits.length, took },
    hits,
    pagination: {
      limit: pageSize,
      count: hits.length,
      hasMore: hits.length === pageSize,
      searchAfter: hits[hits.length - 1]?.sort || null, // For next page
    },
  };
};
```

### Response Format

```json
{
  "status": { "success": true, "hits": 100, "took": 45 },
  "hits": [
    /* array of results */
  ],
  "pagination": {
    "limit": 100,
    "count": 100,
    "hasMore": true,
    "searchAfter": ["species_name_value", "taxon_id_value"]
  }
}
```

### Client Usage Pattern

```
Page 1: GET /searchPaginated?query=...&limit=100
Page 2: GET /searchPaginated?query=...&limit=100&searchAfter=[value1,value2]
Page 3: GET /searchPaginated?query=...&limit=100&searchAfter=[value3,value4]
...
```

---

## 4. Count Endpoint

**File:** `routes/count.js`

### Implementation

```javascript
export const getSearchResultCount = async (req, res) => {
  try {
    const q = req.expandedQuery || req.query || {};
    let response = await getResultCount(q);
    return res.status(200).send(formatJson(response, q.indent));
  } catch (message) {
    return res.status(400).send({ status: "error" });
  }
};
```

### getResultCount Function

**File:** `functions/getResultCount.js`

```javascript
export const getResultCount = async (params) => {
  params.size = 0; // Return no results, only count
  let exclusions = setExclusions(params);
  let result = await getResults({
    ...params,
    exclusions,
  });

  let response = { status: {}, count: 0 };
  ["success", "error"].forEach((key) => {
    if (result.status.hasOwnProperty(key)) {
      response.status[key] = result.status[key];
    }
  });

  if (result.status.hasOwnProperty("hits")) {
    response.count = result.status.hits; // Extract hit count
  }

  return response;
};
```

### Response Format

```json
{
  "status": { "success": true },
  "count": 42500
}
```

### Key Characteristics

- **NOT a batch endpoint** — count is a single-query operation
- Reuses `getResults()` with `size=0` (efficient — no data transfer)
- Works with all query types: AND, OR, taxonomic, metadata filters, etc.
- **Supports same query syntax as search** (including OR operators)

---

## 5. Batch Search (msearch) Endpoint

**File:** `routes/msearch.js`

### Purpose

Execute multiple independent searches in parallel using Elasticsearch's `msearch` API.

### Request Format (POST)

```json
{
  "searches": [
    {
      "query": "tax_rank(species)",
      "result": "taxon",
      "taxonomy": "ncbi",
      "fields": "taxon_id,scientific_name",
      "limit": 100
    },
    {
      "query": "tax_rank(genus)",
      "result": "taxon",
      "taxonomy": "ncbi",
      "limit": 50
    }
  ]
}
```

### Response Format

```json
{
  "status": { "success": true, "hits": 4500 },
  "results": [
    {
      "status": "success",
      "count": 523,
      "total": 523,
      "hits": [
        /* array of results */
      ],
      "search": {
        /* original search params */
      }
    },
    {
      "status": "success",
      "count": 891,
      "total": 891,
      "hits": [
        /* array of results */
      ],
      "search": {
        /* original search params */
      }
    }
  ]
}
```

### Key Implementation Details

```javascript
export const getMsearch = async (req, res) => {
  const { searches = [] } = req.body;

  // Validation
  if (!Array.isArray(searches) || searches.length === 0) {
    return error("Request body must contain array of searches");
  }
  if (searches.length > 100) {
    return error("Maximum 100 searches per request");
  }

  // Build msearch request body
  const msearchBody = [];

  for (let searchIdx = 0; searchIdx < searches.length; searchIdx++) {
    const searchRequest = searches[searchIdx];
    const { query, result, taxonomy, fields, limit, offset, ... } = searchRequest;

    try {
      // Build ES query using getResults()
      const searchQuery = await getResults({
        query, result, taxonomy, fields,
        size: Math.min(parseInt(limit) || 100, 10000),
        from: Math.max(parseInt(offset) || 0, 0),
        ...
      });

      // Extract query structure
      const searchBody = {
        size: searchQuery.query.size || 100,
        from: searchQuery.query.from || 0,
        query: searchQuery.query.query,
      };

      if (searchQuery.query._source) {
        searchBody._source = searchQuery.query._source;
      }
      if (searchQuery.query.aggs) {
        searchBody.aggs = searchQuery.query.aggs;
      }

      // Add to msearch body
      msearchBody.push(
        { index: indexName({ result, taxonomy }) },
        searchBody
      );
    } catch (error) {
      // Record error, continue processing
      searchErrors[searchIdx] = { status: "error", error: error.message };
      // Add placeholder to keep indices aligned
      msearchBody.push(
        { index: indexName({ result: "taxon", taxonomy: "ncbi" }) },
        { query: { match_none: {} } }
      );
    }
  }

  // Execute all searches in parallel
  const mSearchResponse = await client.msearch({ body: msearchBody });
  const responses = mSearchResponse?.body?.responses || [];

  // Process and format results
  const results = await Promise.all(
    responses.map(async (response, index) => {
      // ... extract hits, total count, etc.
      return {
        status: "success",
        count: processedHits.length,
        total: response.hits?.total?.value,
        hits: processedHits,
      };
    })
  );
};
```

### Batch Count via msearch

**NOT directly supported** — but you can:

1. **Option A:** Call `/count` endpoint for each query individually
2. **Option B:** Use `/msearch` with `size: 0` (returns `total` count without hits):
   ```json
   {
     "searches": [
       { "query": "...", "result": "taxon", "limit": 0 },
       { "query": "...", "result": "taxon", "limit": 0 }
     ]
   }
   ```

### Limitations

- **Maximum 100 searches per request**
- Each search is **independent** (no cross-search dependencies)
- **No aggregation across searches** (results are per-search)
- Queries can have **different result types, taxonomies, filters**
- Error in query build doesn't fail entire batch (returns error for that search)

---

## 6. Multi-Query/Batch Count Capabilities

### Single Count Operation

```
GET /count?query=tax_rank(species)&result=taxon&taxonomy=ncbi
→ { "status": {"success": true}, "count": 42500 }
```

### Count with OR

```
GET /count?query=tax_rank(species)%20OR%20tax_rank(genus)&result=taxon
→ { "status": {"success": true}, "count": 123456 }
```

### Batch Counts (Workaround)

1. **Sequential calls:**

   ```
   GET /count?query=tax_rank(species)&...
   GET /count?query=tax_rank(genus)&...
   GET /count?query=tax_rank(family)&...
   ```

2. **Via msearch with size=0:**
   ```
   POST /msearch
   { "searches": [
     { "query": "tax_rank(species)", "result": "taxon", "limit": 0 },
     { "query": "tax_rank(genus)", "result": "taxon", "limit": 0 }
   ]}
   ```
   Response includes `"total"` count for each search.

### Count Modes (Individual vs. Total)

- **Current behavior:** `count` endpoint returns **only the total** for the query
- **Individual counts via msearch:** Each search in the batch returns its own count (`"total"` field)
- **NO built-in "unique count" mode** — no deduplication across OR queries

---

## 7. Query Combination Patterns and Limitations

### Valid Patterns

| Pattern                                  | Semantics | Notes                                 |
| ---------------------------------------- | --------- | ------------------------------------- |
| `term1 AND term2 AND term3`              | AND       | Multiple conditions must all match    |
| `term1\nterm2\nterm3`                    | AND       | Newline-separated (multi-line)        |
| `term1 OR term2 OR term3`                | OR        | At least one must match (bool.should) |
| `(term1 AND term2) OR (term3 AND term4)` | Mixed     | Parentheses with mixed AND/OR         |

### Implementation Constraints

1. **OR Query Processing:**
   - Each OR part is passed through `generateSingleQuery()`
   - Each generates its own Elasticsearch query via `searchByTaxon()`
   - Queries are combined using `combineOrQueries()` with `bool.should`
   - **All inner_hits are stripped** to avoid nesting issues

2. **Aggregation Merging:**
   - When combining OR queries, aggregations from all queries are **merged** (not nested)
   - First query's size/offset is used for pagination

3. **inner_hits Handling:**
   - `combineOrQueries()` has a `stripAllInnerHits()` function that **recursively removes ALL inner_hits**
   - This prevents deeply nested inner_hits structures that can cause Elasticsearch issues
   - **Note:** This may affect fields/data that depend on inner_hits (e.g., matched attributes)

4. **Error Handling:**
   - msearch: Errors in one search don't fail the batch
   - OR queries: If any part fails to generate, the entire OR query fails
   - Count: Same error handling as search (returns error status)

---

## 8. Files Summary

| File                            | Purpose                                                                |
| ------------------------------- | ---------------------------------------------------------------------- |
| `functions/combineOrQueries.js` | Combines multiple ES queries with bool.should logic                    |
| `functions/getResults.js`       | Main query generator; detects OR patterns; orchestrates query building |
| `functions/getResultCount.js`   | Wrapper for count; calls getResults with size=0                        |
| `routes/search.js`              | Primary search endpoint (offset-based pagination)                      |
| `routes/searchPaginated.js`     | Cursor-based pagination via search_after                               |
| `routes/count.js`               | Count endpoint                                                         |
| `routes/msearch.js`             | Batch search endpoint                                                  |

---

## 9. Key Takeaways for Implementation

### What IS Supported

✅ Top-level OR operators for combining queries
✅ AND operators for multiple conditions
✅ Mixed AND/OR with parentheses
✅ Batch searches via msearch (up to 100 per request)
✅ Cursor-based pagination via searchPaginated
✅ Count for any query type (including OR)
✅ Individual counts in msearch results (via `"total"` field)

### What IS NOT Supported

❌ Batch count endpoint (use msearch with `limit: 0` as workaround)
❌ Unique/deduplication count across OR queries
❌ Cross-search dependencies in msearch
❌ Aggregation across multiple msearch results

### Constraints & Quirks

⚠️ OR queries have all `inner_hits` stripped recursively
⚠️ msearch: Maximum 100 searches per request
⚠️ searchPaginated: Requires sorting for consistent pagination
⚠️ Count: Returns only total count, no breakdown by result type

---

## 10. References

**Relevant ES Docs:**

- [ES bool.should](https://www.elastic.co/guide/en/elasticsearch/reference/current/query-dsl-bool-query.html#bool-should)
- [ES search_after](https://www.elastic.co/guide/en/elasticsearch/reference/current/paginate-search-results.html#search-after)
- [ES msearch](https://www.elastic.co/guide/en/elasticsearch/reference/current/search-multi-search.html)
