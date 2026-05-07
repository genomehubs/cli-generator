# Phase XX: Custom Histogram Boundaries

**Status:** Design capture (not sequenced into ordered phases yet)
**Rationale:** User control over binning strategy; enables clean bucketing for specialized use cases
**Priority:** Post-MVP nice-to-have

---

## Overview

Allow users to specify custom boundary values for histogram aggregations on any axis (x, y, or category). Instead of ES auto-binning with fixed intervals, users provide explicit breakpoints that define bucket ranges. Especially valuable for category axes where boundaries determine the "natural" groupings for analysis.

### Use case example

```json
{
  "axes": [
    {
      "field": "genome_size",
      "position": "x",
      "boundaries": [1000000, 10000000, 100000000, 1000000000]
    },
    { "field": "assembly_level", "position": "y" }
  ]
}
```

X-axis is bucketed as: `[1M-10M]`, `[10M-100M]`, `[100M-1B]`, `[1B+]`

---

## Implementation

### 1. Data model

Add `boundaries` field to `AxisInput` in `axis.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AxisInput {
    pub field: String,
    pub position: AxisPosition,
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub bin_count: Option<usize>,
    #[serde(default)]
    pub show_other: Option<bool>,
    #[serde(default)]
    pub boundaries: Option<Vec<f64>>,  // NEW
}
```

Propagate to `AxisSpec`:

```rust
pub struct AxisSpec {
    pub field: String,
    pub position: AxisPosition,
    pub scale: Option<String>,
    pub bin_count: Option<usize>,
    pub show_other: bool,
    pub boundaries: Option<Vec<f64>>,  // NEW
}
```

### 2. Bounds computation

In `bounds.rs`, when `spec.boundaries.is_some()`:

- **Skip ES bounds probe** — no need to query ES for min/max
- **Return boundaries directly** as domain `[first, last]`
- **Auto-generate labels** from boundary pairs:
  - For `[0, 10, 20, 100]` → `["0-10", "10-20", "20-100"]`
  - Plus optional `"other"` bucket for values outside ranges if `show_other` is true

```rust
pub fn compute_bounds(
    spec: &AxisSpec,
    cache: &mut BoundsCache,
) -> Result<BoundsResult, ApiError> {
    // If boundaries provided, use them directly
    if let Some(boundaries) = &spec.boundaries {
        let mut labels = Vec::new();
        for i in 0..boundaries.len() - 1 {
            labels.push(format!("{}-{}", boundaries[i] as i64, boundaries[i + 1] as i64));
        }
        if spec.show_other {
            labels.push("other".to_string());
        }

        return Ok(BoundsResult {
            domain: [boundaries[0], boundaries[boundaries.len() - 1]],
            interval: None,  // Not used for custom boundaries
            cat_labels: labels,
        });
    }

    // Otherwise, existing bounds logic (ES probe)
    ...
}
```

### 3. Aggregation building

In `agg.rs`, when building aggregations, dispatch on `spec.boundaries.is_some()`:

**If boundaries exist:**

- Use `filters` aggregation (not histogram) with range queries
- One range filter per boundary pair, plus optional "other" catch-all

Example for `[0, 10, 20, 100]`:

```json
{
  "agg_name": {
    "filters": {
      "filters": {
        "0-10": { "range": { "attributes.long_value": { "gte": 0, "lt": 10 } } },
        "10-20": { "range": { "attributes.long_value": { "gte": 10, "lt": 20 } } },
        "20-100": { "range": { "attributes.long_value": { "gte": 20, "lt": 100 } } },
        "other": { "bool": { "must_not": [
          { "range": { "attributes.long_value": { "gte": 0, "lt": 100 } } }
        ] } }
      }
    },
    "aggs": { ... }  // nested aggs for nested x/y
  }
}
```

**If boundaries missing:**

- Use existing logic (histogram, terms, date_histogram based on ValueType)

Helper function in `agg.rs`:

```rust
fn build_custom_boundaries_agg(
    boundaries: &[f64],
    value_field: &str,
    show_other: bool,
) -> Value {
    let mut filters = Map::new();

    for i in 0..boundaries.len() - 1 {
        let label = format!("{}-{}", boundaries[i] as i64, boundaries[i + 1] as i64);
        filters.insert(label, json!({
            "range": {
                value_field: {
                    "gte": boundaries[i],
                    "lt": boundaries[i + 1]
                }
            }
        }));
    }

    if show_other {
        let lower = boundaries[0];
        let upper = boundaries[boundaries.len() - 1];
        filters.insert("other".to_string(), json!({
            "bool": {
                "must_not": [
                    {
                        "range": {
                            value_field: {
                                "gte": lower,
                                "lt": upper
                            }
                        }
                    }
                ]
            }
        }));
    }

    json!({
        "filters": {
            "filters": filters
        }
    })
}
```

### 4. Response extraction

In `report_types.rs`, extractors need to branch based on whether boundaries were used.

**If boundaries were used:**

- Read from filters aggregation buckets (structure is `{label: {doc_count, ...},...}`)
- Iterate through labels in boundary order (not arbitrary filter order)
- Preserve "other" bucket at the end

**If boundaries missing:**

- Use existing extraction logic (histogram array or terms object)

Example branch in `extract_cat_histograms`:

```rust
fn extract_cat_histograms(
    resp: &Value, agg_name: &str, x_field: &str, x_bucket_agg: &str,
    main_bucket_count: usize, cat_labels: &[String], show_other: bool,
    cat_is_numeric: bool,
    boundaries_used: bool,  // NEW PARAM
    main_counts: &[u64],
) -> Value {
    let by_value = &resp[agg_name]["by_value"];

    if boundaries_used {
        // Read from filters agg: by_value.buckets is {label: {doc_count, ...}, ...}
        // Iterate in cat_labels order (which matches boundary pair order)
        let mut result = Vec::new();
        for label in cat_labels {
            if let Some(bucket) = by_value.get(label) {
                let doc_count = bucket.get("doc_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                // Extract nested x-axis histogram from bucket[x_field][x_bucket_agg]
                result.push((label.clone(), doc_count, ...));
            }
        }
        result.into()
    } else {
        // Existing logic for histogram/terms agg
        ...
    }
}
```

### 5. Handler updates

In `run_histogram_report` and `run_scatter_report`:

```rust
// Determine if custom boundaries were used
let boundaries_used = x_spec.boundaries.is_some()
    || y_spec.boundaries.is_some()
    || cat_spec.boundaries.is_some();

// Pass flag through to extractors
let result = extract_cat_histograms(
    &resp, &agg_name, &x_field, &x_bucket_agg,
    main_bucket_count, &cat_labels, show_other,
    cat_is_numeric, boundaries_used,  // NEW PARAM
    &main_counts,
);
```

---

## Test coverage

1. **Unit test:** Bounds computation with boundaries set (skip ES probe, return labels)
2. **Unit test:** Custom boundaries agg builder (filters agg structure correct)
3. **Integration test:** Scatter report with custom x-axis boundaries (fetch ES, extract filters buckets)
4. **Integration test:** Histogram with custom category boundaries (both keyword and numeric cat branches)
5. **Edge case:** Empty boundary list (should error or default to current behavior)
6. **Edge case:** Single boundary value (no ranges, treat as "all other")
7. **Edge case:** Unsorted boundaries (should error or auto-sort)

---

## Backward compatibility

- `boundaries` is `Option<Vec<f64>>`, defaulting to `None`
- Existing API clients unaffected (field absent = use current logic)
- No changes to response structure (same histogram/category output format)
- Extraction logic branches on flag, not on presence of field

---

## Future enhancements

1. **Date boundaries:** Allow ISO 8601 date strings in `boundaries` for date fields
   - `["2020-01-01", "2021-01-01", "2022-01-01"]` → year buckets
   - Requires `Vec<DateBoundary>` or discriminated union

2. **Custom bucket labels:** Allow user-provided labels instead of auto-generated "min-max"

   ```json
   {
     "boundaries": [0, 10, 100],
     "labels": ["small", "medium", "large"]
   }
   ```

3. **Boundary validation:** Assert boundaries are sorted, non-empty, and within reasonable domain
   - Could precompute in bounds.rs and fail early with clear error

4. **Performance:** For large boundary count, consider ES `range` agg (aggregates ranges in one pass) instead of filters agg (one query per boundary)
