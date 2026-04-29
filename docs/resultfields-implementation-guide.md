# Implementation Guide: `/resultFields` Critical Blocker

**Urgency:** 🔴 TOP PRIORITY — Unblocks all downstream work
**Estimated Effort:** 3-4 days (1-2 days solo; less if pair-programmed)
**Acceptance Criteria:** Endpoint returns field metadata matching old API shape

---

## Problem Statement

**Current State:**

- ✅ `attr_types()` function (in `src/core/attr_types.rs`) queries ES attributes index
- ✅ Returns: `(TypesMap, SynonymsMap)` — internal Rust data structures
- ❌ No HTTP endpoint exposing this
- ❌ Users can't fetch field metadata → can't validate queries → searches fail silently

**Solution:**

- Wrap `attr_types()` with response formatting
- Add HTTP handler
- Deploy; verify against old API

---

## Implementation Breakdown

### Phase 1: Audit Current `attr_types.rs` (4 hours)

**Goal:** Understand what data attr_types returns; identify any gaps.

#### File: `src/core/attr_types.rs`

Read the full file. Key things to check:

1. **TypeMeta struct** — What fields does it have?

   ```rust
   pub struct TypeMeta {
       pub group: String,          // "taxon", "assembly", etc.
       pub name: String,           // "genome_size", "assembly_level"
       pub r#type: Option<String>, // "long", "keyword", etc.
       pub summary: Option<Value>, // ["min", "max", "median"] for aggregatable fields
       pub default_summary: Option<String>,
       pub return_type: Option<String>,
       pub synonyms: Vec<String>,  // ["gc_percent", "gc_pc"]
       pub processed_type: Option<String>, // "integer", "keyword", "ordered_keyword"
       pub processed_summary: Option<String>, // Computed: "long_value", "keyword_value.raw"
       pub processed_simple: Option<String>,
   }
   ```

   **Action items:**
   - [ ] Verify all fields are present that old API needs
   - [ ] Check if `display_name`, `display_group`, `description` are missing (they might be in FieldDef, not TypeMeta)
   - [ ] Check if constraints (enum values) are captured

2. **TypesMap / SynonymsMap** — What do these contain?

   ```rust
   pub type TypesMap = HashMap<String, HashMap<String, TypeMeta>>;
   //                  result_group → field_name → metadata

   pub type SynonymsMap = HashMap<String, HashMap<String, String>>;
   //                     result_group → synonym → canonical_name
   ```

   **Action items:**
   - [ ] Verify structure matches what old API needs
   - [ ] Check: are identifiers separate from fields? (old API has both)

3. **set_processed_type() and set_processed_summary()** — Are these correct?
   - Compare logic to old API's similar functions (if any)
   - **Action:** Run test query; inspect output

#### Deliverable: Checklist of what's present vs. missing

---

### Phase 2: Compare to Old API Response (4 hours)

**Goal:** Understand exact format old API returns; plan transformations.

#### File: `local-api-copy/src/api/v2/routes/resultFields.js`

Read the entire file. Key observations:

```javascript
export const getResultFields = async (req, res) => {
  let fields = {};
  let identifiers = {};
  let status = {};
  const q = req.expandedQuery || req.query || {};
  let release = q.release || config.release;
  let { hub, source } = config;

  try {
    ({ typesMap: fields } = await attrTypes({ ...q }));
    ({ typesMap: identifiers } = await attrTypes({
      ...q,
      indexType: "identifiers",
    }));
    status = { success: true };
  } catch (message) {
    logError({ req, message });
    status = { success: false, error: "Unable to fetch fields" };
  }

  let response = { status, fields, identifiers, hub, release, source };
  return res.status(200).send(formatJson(response, q.indent));
};
```

**Key insights:**

1. Calls `attrTypes()` twice: once for fields (result=taxon), once for identifiers (indexType=identifiers)
2. Returns:
   - `status`: success/error status
   - `fields`: TypesMap for main fields
   - `identifiers`: TypesMap for identifier types
   - `hub`: site name (from config)
   - `release`: API version/date (from config)
   - `source`: data source name (from config)

#### Deliverable: Sample output JSON from old API

```bash
curl -s "http://localhost:9000/api/v2/resultFields?result=taxon" | jq . > /tmp/old_api_result_fields.json
# Inspect this file carefully
```

---

### Phase 3: Create `src/core/result_fields.rs` (6-8 hours)

**Goal:** Format attr_types output to match old API response shape.

#### File to create: `src/core/result_fields.rs`

```rust
use anyhow::Result;
use crate::core::attr_types::{attr_types, TypesMap, SynonymsMap};
use serde_json::{json, Value};

/// Response envelope for /resultFields endpoint
pub struct ResultFieldsResponse {
    pub status: serde_json::json!({ "success": true }),
    pub fields: TypesMap,
    pub identifiers: TypesMap,
    pub hub: String,
    pub release: String,
    pub source: String,
}

/// Fetch field metadata from attributes index and format for API response
pub fn get_result_fields(
    es_base: &str,
    result: &str,          // "taxon", "assembly", "sample", "multi"
    hub: &str,
    release: &str,
    source: &str,
) -> Result<Value> {
    // Query attributes index for main fields
    let (fields_map, _) = attr_types(es_base, "attributes", result)?;

    // Query for identifiers (separate index or same with indexType filter?)
    let (identifiers_map, _) = attr_types(es_base, "identifiers", result)?;

    // Format response
    Ok(json!({
        "status": { "success": true },
        "fields": fields_map,
        "identifiers": identifiers_map,
        "hub": hub,
        "release": release,
        "source": source,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_fields_response_has_required_keys() {
        // Mock test: doesn't call ES
        // Just verify structure
    }
}
```

**Tasks:**

- [ ] Create file `src/core/result_fields.rs`
- [ ] Implement `get_result_fields()` function
- [ ] Add to `src/core/mod.rs` (pub mod result_fields;)
- [ ] Add unit tests (mock ES response, verify formatting)

#### Questions to Resolve:

1. **Identifiers index** — Is it separate? Same index with different query?
   - Check old attrTypes.js implementation
   - Ask: does ES have "identifiers" index or just "attributes" with `indexType` parameter?

2. **Hub, release, source** — Where do these come from?
   - Hub: site name (from config)
   - Release: API version or timestamp?
   - Source: data source (NCBI, UniProt, etc.)?
   - Check: `src/core/config.rs` for these values

3. **Response shape** — Exact field names and nesting?
   - Get sample from old API: `curl .../resultFields?result=taxon | jq keys`
   - Verify all keys present in Rust version

#### Deliverable: Tested `result_fields.rs` module

---

### Phase 4: Wire HTTP Handler (4-6 hours)

**Goal:** Add HTTP endpoint that calls `get_result_fields()` and returns JSON.

#### Where to wire this: TBD

Depends on your API generation system. Typical approach:

**Option A: Generated API (if using cli-generator)**

- Add handler to generated project template
- Endpoint: `GET /api/v2/resultFields?result=taxon&index=attributes`

**Option B: Standalone server**

- Create `src/bin/api.rs` with Axum/Actix router
- Add route: `GET /api/v2/resultFields` → handler

#### Assumed structure (adapt to your setup):

```rust
// In generated API handler file
use crate::core::result_fields::get_result_fields;
use axum::{extract::Query, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ResultFieldsParams {
    result: Option<String>,    // "taxon", "assembly", etc.
    index: Option<String>,     // "attributes", "identifiers"
}

pub async fn handle_result_fields(
    Query(params): Query<ResultFieldsParams>,
    // Access to ES base URL, config, etc. (inject via state/context)
) -> Result<Json<Value>, ApiError> {
    let result = params.result.unwrap_or("taxon".to_string());

    let response = get_result_fields(
        &ES_BASE,  // e.g., "http://localhost:9200"
        &result,
        "genomehubs",
        "2026-04-28",
        "NCBI",
    )?;

    Ok(Json(response))
}
```

**Tasks:**

- [ ] Create handler function
- [ ] Wire to HTTP router
- [ ] Test manually: `curl http://localhost:8000/api/v2/resultFields?result=taxon`

#### Deliverable: Working HTTP endpoint

---

### Phase 5: Testing & Validation (8+ hours)

**Goal:** Verify output matches old API; fix discrepancies.

#### Test 1: Golden File Test

```bash
# 1. Get old API output
curl -s "http://localhost:9000/api/v2/resultFields?result=taxon" > /tmp/old_api.json

# 2. Get new API output
curl -s "http://localhost:8000/api/v2/resultFields?result=taxon" > /tmp/new_api.json

# 3. Compare (allowing for minor differences)
diff <(jq -S . /tmp/old_api.json) <(jq -S . /tmp/new_api.json)
```

**Expected minor differences:**

- Field order (sort keys? old API vs new)
- Whitespace (indent)
- Missing keys that old API doesn't need
- **Unacceptable differences:** Missing required keys, wrong types, wrong values

#### Test 2: Structure Validation

```rust
#[test]
fn result_fields_response_structure() {
    let response = serde_json::json!({
        "status": { "success": true },
        "fields": { "genome_size": {...} },
        "identifiers": { ... },
        "hub": "genomehubs",
        "release": "2026-04-28",
        "source": "NCBI",
    });

    assert!(response.get("status").is_some());
    assert!(response.get("fields").is_some());
    assert!(response.get("identifiers").is_some());
    assert_eq!(response["hub"], "genomehubs");
}
```

#### Test 3: Field Metadata Completeness

For each field in response, verify:

- ✅ `name` present and non-empty
- ✅ `type` matches ES field type
- ✅ `processed_type` is set (integer, keyword, float, etc.)
- ✅ `synonyms` array present (may be empty)
- ✅ `summary` array for aggregatable fields

#### Test 4: Synonym Mapping

```bash
# Query with synonym, verify it resolves to canonical name
query_builder.add_attribute("gc_percent", ">=", "50")  # synonym
# Internally should normalize to: "gc_percentage"
```

#### Deliverable: Test report; documented discrepancies (if any)

---

### Phase 6: Integration with Validation (4 hours)

**Goal:** Update `validate()` to use `/resultFields` for field checking.

#### File: `src/core/validate.rs`

**Current state:** Likely incomplete or not using /resultFields.

**What's needed:**

1. Fetch field metadata at validation time
2. Cache it (per QueryBuilder instance, or global with TTL)
3. When validating `add_attribute("genome_size", ...)`:
   - Check: is "genome_size" a known field or synonym?
   - Check: does field type support the operator (= vs. >= for keyword)?
   - Return: validation error if not

**Pseudo-code:**

```rust
impl QueryBuilder {
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Fetch field metadata (from /resultFields endpoint or attr_types)
        let fields_map = self.fetch_field_metadata();  // Call attr_types or HTTP

        // For each attribute in query
        for attr in &self.attributes {
            // Normalize field name via synonyms
            let canonical_name = self.resolve_synonym(&attr.name, &synonyms_map)?;

            // Check: field exists
            if !fields_map.contains_key(&canonical_name) {
                errors.push(format!("Unknown field: {}", attr.name));
                continue;
            }

            let field_meta = &fields_map[&canonical_name];

            // Check: operator valid for field type
            if !self.is_valid_operator(&attr.operator, &field_meta.processed_type) {
                errors.push(format!("Operator {} not valid for field type {}",
                    attr.operator, field_meta.processed_type));
            }

            // Check: modifiers valid for field type
            for modifier in &attr.modifiers {
                if !field_meta.summary.contains(modifier) {
                    errors.push(format!("Modifier {} not supported for field {}",
                        modifier, attr.name));
                }
            }
        }

        errors
    }
}
```

#### Deliverable: Integrated validation; test suite

---

## Files Changed / Created

| File                        | Action        | Effort   | Notes                                |
| --------------------------- | ------------- | -------- | ------------------------------------ |
| `src/core/result_fields.rs` | Create        | 2 days   | New module; format attr_types output |
| `src/core/mod.rs`           | Modify        | 15 min   | Add `pub mod result_fields;`         |
| `src/core/validate.rs`      | Modify        | 1 day    | Integrate field metadata lookup      |
| HTTP handler (location TBD) | Create/Modify | 1-2 days | Wire endpoint to router              |
| Tests                       | Create        | 1-2 days | Unit + integration tests             |

**Total:** 3-4 days

---

## Verification Checklist

Before considering this complete:

- [ ] `attr_types()` function tested with live ES
- [ ] `result_fields.rs` module created and unit-tested
- [ ] HTTP endpoint wired and returns JSON
- [ ] Output structure matches old API (manual inspection)
- [ ] Golden file test passes (old vs new output)
- [ ] Validation integrates field metadata correctly
- [ ] Synonym normalization works (test: synonym → canonical)
- [ ] Enum values extracted correctly (if present in metadata)
- [ ] No ES connection errors; proper error messages on failure
- [ ] Code follows style guide (cargo fmt, clippy)

---

## Risks & Mitigations

| Risk                                    | Mitigation                                                             |
| --------------------------------------- | ---------------------------------------------------------------------- |
| Identifiers vs. fields indexing unclear | Query old attrTypes.js; ask if separate index or same with filter      |
| Missing fields in TypeMeta struct       | Compare TypeMeta to old TypeMeta.js; add missing fields                |
| Response shape mismatch                 | Golden file test; compare keys and types carefully                     |
| Performance: attr_types() too slow      | Add caching (24-hour TTL); measure ES query time                       |
| Enum values not extracted               | Check TypeMeta.summary field; update set_processed_summary() if needed |

---

## Success Criteria

✅ `/resultFields` endpoint exists
✅ Returns JSON matching old API structure
✅ All required fields present: status, fields, identifiers, hub, release, source
✅ Field metadata (name, type, processed_type, summary, synonyms) present and correct
✅ Validation integrates and works (unknown field → error)
✅ Zero ES connection errors or silent failures (errors bubble up)
✅ Code passes all tests and linter checks

---

## Timeline Estimate

| Phase                        | Days         | Notes                                |
| ---------------------------- | ------------ | ------------------------------------ |
| 1. Audit current code        | 0.5          | Quick read-through                   |
| 2. Compare to old API        | 0.5          | Get sample outputs; understand shape |
| 3. Create result_fields.rs   | 2            | Implementation + unit tests          |
| 4. Wire HTTP handler         | 1–1.5        | Depends on your framework            |
| 5. Testing & validation      | 2–2.5        | Golden tests, integration tests      |
| 6. Integrate with validation | 1            | Update validate.rs                   |
| **Total**                    | **7–8 days** | **Or 3–4 days if pair-programmed**   |

---

## Next Steps

1. **Today:** Read `src/core/attr_types.rs` and `local-api-copy/src/api/v2/routes/resultFields.js`
2. **Tomorrow:** Create `result_fields.rs` stub; audit for gaps
3. **This week:** Implement + test + wire HTTP endpoint
4. **By end of week:** Endpoint live and validated

Questions? Ask about identifiers indexing, config structure (hub/release/source), or HTTP framework.
