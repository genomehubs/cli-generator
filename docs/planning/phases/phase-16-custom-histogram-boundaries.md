# Phase XX — Custom Histogram Boundaries (with Date & Label Support)

**Status:** Ready to implement
**Prerequisite:** Phase 15 (filter expressions in place)
**Priority:** Enables user control over binning; especially valuable for category axes

---

## Scope

This phase adds **custom boundary control** to histogram aggregations with **native date support** and **custom bucket labels**. Users define explicit breakpoints; ES buckets values into ranges defined by those boundaries.

### Included features

1. **Numeric boundaries** — explicit breakpoints for any numeric field

   ```yaml
   report: histogram
   x:
     field: genome_size
     boundaries: [1000000, 10000000, 100000000, 1000000000]
   ```

   → Buckets: `[1M-10M]`, `[10M-100M]`, `[100M-1B]`, `[1B+]`

2. **Date boundaries** — calendar intervals or explicit timestamps

   ```yaml
   report: histogram
   x:
     field: release_date
     scale: date
     boundaries:
       intervals: [week, month, quarter] # or explicit: ["2020-01-01", "2021-01-01", "2022-01-01"]
   ```

3. **Custom bucket labels** — override auto-generated labels

   ```yaml
   report: histogram
   x:
     field: genome_size
     boundaries: [1000000, 10000000, 100000000, 1000000000]
     labels: [">1M-10M", ">10M-100M", ">100M-1B", ">1B"]
   ```

4. **Early validation** — detect config errors before ES query
   - Boundaries sorted (numeric) or valid (dates)
   - Labels count matches bucket count
   - Date scale & interval consistency
   - Field metadata consistency (type, cardinality)

### Not included (deferred to Phase-XX-boundaries-advanced)

- Auto-suggestion of optimal boundaries from data distribution
- Timezone-aware date arithmetic (use UTC for now)
- Complex interval patterns beyond calendar intervals
- Boundary presets / templates

---

## Data model

### Rust: extend `AxisOpts`

**File:** `crates/genomehubs-query/src/report/axis.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisOpts {
    pub min: Option<String>,
    pub max: Option<String>,
    pub fixed_values: Vec<(String, String)>,
    pub size: usize,
    pub show_other: bool,
    pub scale: Scale,
    pub sort: SortMode,
    pub interval: Option<DateInterval>,
    // NEW:
    #[serde(default)]
    pub boundaries: Option<Boundaries>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Boundaries {
    Numeric(Vec<f64>),
    Date {
        #[serde(default)]
        intervals: Option<Vec<String>>,  // ["week", "month", "quarter"]
        #[serde(default)]
        explicit: Option<Vec<String>>,   // ["2020-01-01", "2021-01-01", ...]
    },
}
```

### AxisSpec mirror

```rust
pub struct AxisSpec {
    pub field: String,
    pub role: AxisRole,
    pub summary: AxisSummary,
    pub value_type: ValueType,
    pub opts: AxisOpts,
    // ... existing fields ...
}
// No changes needed; AxisOpts now carries boundaries
```

---

## Implementation touchpoints

### 1. Bounds computation (`bounds.rs`)

**File:** `crates/genomehubs-api/src/report/bounds.rs`

When `axis_opts.boundaries.is_some()`:

- **Skip ES bounds probe** if boundaries explicitly provided
- **Convert date intervals to boundaries** if needed:
  - `"week"` → timestamp boundaries starting today
  - `"month"` → month boundaries for current year + next
  - `"quarter"` → Q1, Q2, Q3, Q4 boundaries
- **Generate bucket labels** from boundary pairs
  - Numeric: `"{low}-{high}"` (format intelligently: 1B, 10M, etc.)
  - Date: `"{date1:short}–{date2:short}"`
  - Use `opts.labels` if provided (validate count matches)

**Example logic:**

```rust
pub fn compute_bounds(axis_spec: &AxisSpec) -> Result<BoundsResult, ApiError> {
    if let Some(boundaries) = &axis_spec.opts.boundaries {
        match axis_spec.value_type {
            ValueType::Numeric => {
                validate_numeric_boundaries(boundaries)?;
                let labels = generate_numeric_labels(boundaries, &axis_spec.opts.labels)?;
                Ok(BoundsResult::numeric([boundaries[0], boundaries[boundaries.len()-1]], labels.len(), axis_spec.opts.scale))
            }
            ValueType::Date => {
                let resolved = resolve_date_boundaries(boundaries)?;
                let labels = generate_date_labels(&resolved, &axis_spec.opts.labels)?;
                Ok(BoundsResult::date([resolved[0], resolved[resolved.len()-1]], labels, axis_spec.opts.interval))
            }
            _ => Err(ApiError::BadRequest("boundaries not supported for this value type".into()))
        }
    } else {
        // Existing logic: ES probe
        ...
    }
}
```

### 2. Aggregation builder (`agg.rs`)

**File:** `crates/genomehubs-api/src/report/agg.rs`

Modify `NumericHistogramAggBuilder` and `DateHistogramAggBuilder`:

- Accept custom boundaries from `axis_spec`
- Use `terms` agg with explicit boundary ranges instead of fixed-interval histogram
- Or use `range` agg if ES supports it

**Pattern** (pseudo-code):

```rust
impl NumericHistogramAggBuilder {
    fn build(&self, bounds: &BoundsResult) -> Value {
        if bounds.has_custom_boundaries() {
            // Use range agg with explicit buckets
            serde_json::json!({
                "range": {
                    "field": self.field,
                    "ranges": bounds.custom_ranges()  // [{"to": 10M}, {"from": 10M, "to": 100M}, ...]
                }
            })
        } else {
            // Existing: histogram agg with auto interval
            ...
        }
    }
}
```

### 3. Validation (`validation.rs`)

**File:** `crates/genomehubs-query/src/validation.rs` (new validation function)

```rust
pub fn validate_axis_boundaries(
    axis: &AxisSpec,
    metadata: Option<&FieldMetadata>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if let Some(boundaries) = &axis.opts.boundaries {
        match axis.value_type {
            ValueType::Numeric => {
                // Check boundaries are sorted
                if let Boundaries::Numeric(vals) = boundaries {
                    for i in 1..vals.len() {
                        if vals[i] <= vals[i-1] {
                            errors.push(format!("Boundaries must be strictly increasing; got {} after {}", vals[i], vals[i-1]));
                        }
                    }
                }
            }
            ValueType::Date => {
                // Validate date format and sorting
                if let Boundaries::Date { explicit: Some(dates), .. } = boundaries {
                    let parsed: Result<Vec<_>, _> = dates.iter()
                        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
                        .collect();
                    if let Err(e) = parsed {
                        errors.push(format!("Invalid date format (expected YYYY-MM-DD): {}", e));
                    } else {
                        let dates_parsed = parsed.unwrap();
                        for i in 1..dates_parsed.len() {
                            if dates_parsed[i] <= dates_parsed[i-1] {
                                errors.push(format!("Dates must be strictly increasing"));
                            }
                        }
                    }
                }
            }
            _ => errors.push(format!("Boundaries not supported for {:?}", axis.value_type)),
        }

        // Validate labels if provided
        if let Some(labels) = &axis.opts.labels {
            let bucket_count = if let Boundaries::Numeric(vals) = boundaries {
                vals.len() - 1
            } else {
                2  // Placeholder; real count depends on resolved intervals
            };
            if labels.len() != bucket_count {
                errors.push(format!(
                    "Label count mismatch: provided {} labels but have {} buckets",
                    labels.len(),
                    bucket_count
                ));
            }
        }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

### 4. Python SDK: `ReportBuilder` methods

**File:** `python/cli_generator/query.py` and templates

```python
class ReportBuilder:
    def set_axis_boundaries(self, axis_role: str, boundaries: list[float | str], *, labels: list[str] | None = None) -> "ReportBuilder":
        """Set custom boundaries for an axis (x, y, or cat).

        Args:
            axis_role: "x", "y", or "cat"
            boundaries: For numeric axes: sorted numbers. For date axes: ISO 8601 strings or interval names.
            labels: Optional custom bucket labels. Count must match bucket count (len(boundaries) - 1).

        Returns:
            Self for chaining.
        """
        key = f"{axis_role}_opts"
        if key not in self._doc:
            self._doc[key] = {}
        self._doc[key]["boundaries"] = boundaries
        if labels is not None:
            self._doc[key]["labels"] = labels
        return self
```

Also add:

```python
    def set_axis_date_intervals(self, axis_role: str, intervals: list[str]) -> "ReportBuilder":
        """Set date intervals for a date-scaled axis.

        Args:
            axis_role: "x", "y", or "cat"
            intervals: E.g. ["week", "month", "quarter"]

        Returns:
            Self for chaining.
        """
        key = f"{axis_role}_opts"
        if key not in self._doc:
            self._doc[key] = {}
        self._doc[key]["boundaries"] = {"intervals": intervals}
        return self
```

### 5. Mirror to Tera, JS, R templates

- Identical Python SDK changes to `templates/python/query.py.tera`
- JS: `setAxisBoundaries(axisRole, boundaries, opts)` and `setAxisDateIntervals(...)`
- R: `set_axis_boundaries()` and `set_axis_date_intervals()`
- Update `CANONICAL_REPORT_BUILDER_METHODS` in `test_sdk_parity.py`

### 6. Tests

**Unit tests** in `tests/python/test_core.py`:

- Numeric boundaries YAML contains boundaries key
- Date boundaries with intervals
- Custom labels appear in YAML
- Label count validation in ReportBuilder

**Fixture tests** (builder-only, no API responses needed):

- `histogram_numeric_boundaries`
- `histogram_date_intervals`
- `histogram_boundaries_custom_labels`

**Validation tests** in `tests/python/test_validation.py`:

- Boundaries must be sorted
- Labels count mismatch caught early
- Date format validation

### 7. Documentation

- Quarto reference: `set_axis_boundaries()`, `set_axis_date_intervals()` with examples
- GETTING_STARTED.md: add "Custom histogram boundaries" section with use cases

---

## Validation flow

```
User input (YAML)
    ↓
[Deserialize into ReportConfig]
    ↓
[Create AxisSpec with AxisOpts.boundaries]
    ↓
[Validate boundaries early] ← NEW: catch format/sort errors before ES query
    ├─ Check sorted (numeric) or valid (date)
    ├─ Check labels count
    └─ Check date format
    ↓
[compute_bounds()]
    ├─ If custom boundaries: skip ES probe, use provided values
    └─ If auto: existing ES probe logic
    ↓
[Aggregation builder]
    ├─ If boundaries provided: build range/terms agg with explicit buckets
    └─ If auto: existing fixed-interval histogram agg
    ↓
[ES query + pipeline + render]
```

---

## Examples

### Numeric boundaries

```python
from goat_sdk.query import QueryBuilder, ReportBuilder

rb = (ReportBuilder("histogram")
      .set_x("genome_size")
      .set_rank("species")
      .set_axis_boundaries("x", [1e6, 10e6, 100e6, 1e9, 10e9])
      .set_axis_labels("x", ["1M-10M", "10M-100M", "100M-1B", "1B-10B", "10B+"]))

qb = QueryBuilder("taxon").set_taxa(["Mammalia"])
data = qb.report(rb)
```

### Date boundaries

```python
rb = (ReportBuilder("histogram")
      .set_x("release_date")
      .set_rank("species")
      .set_axis_date_intervals("x", ["quarter"]))  # Q1, Q2, Q3, Q4 of current year

qb = QueryBuilder("assembly")
data = qb.report(rb)
```

---

## Deferred to Phase-XX-boundaries-advanced

- Automatic boundary suggestion algorithm (ML/statistical approach)
- Timezone-aware date boundaries
- Boundary presets / templates (e.g., "power-of-10" for genomes)
- Percentile-based boundaries (e.g., "split at 25th/50th/75th percentile")
