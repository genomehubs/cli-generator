# Phase 4: Report Axis Type System

**Depends on:** Phase 0 (ApiStatus), Phase 1 (es_client)
**Blocks:** Phase 5 (report infrastructure uses these types), Phase 6 (report routes use them)
**Estimated scope:** 1 new Rust module (`crates/genomehubs-query/src/report/`), ~5 files

This phase is entirely within `crates/genomehubs-query` — pure types and parsers with
no I/O. All types compile to WASM because `genomehubs-query` is WASM-compatible.

---

## Goal

Define the axis type system that underpins all report types. Every axis in every
report (histogram, scatter, tree, map, arc) is described by an `AxisSpec`; the type
system determines how ES aggregations are built, how domains/bounds are computed, and
how SDK users express axis configuration.

---

## Files to Create

```
crates/genomehubs-query/src/report/
    mod.rs          — re-exports; declares sub-modules
    axis.rs         — AxisRole, ValueType, Scale, AxisSummary, DateInterval, AxisOpts, AxisSpec
    bounds.rs       — BoundsResult (type only; logic in Phase 5's bounds.rs in genomehubs-api)
```

Expose `report` module from `crates/genomehubs-query/src/lib.rs` (for WASM use if needed
in Phase 6) and from `crate` in `genomehubs-api`.

---

## Files to Modify

| File                                 | Change                                 |
| ------------------------------------ | -------------------------------------- |
| `crates/genomehubs-query/src/lib.rs` | `pub mod report;`                      |
| `crates/genomehubs-query/Cargo.toml` | No change needed (serde already a dep) |

---

## Implementation

### `crates/genomehubs-query/src/report/mod.rs`

```rust
//! Report axis type system.
//!
//! All types in this module are serialisable and WASM-compatible.
//! They express the full configuration space for a single report axis —
//! what field to aggregate, how to bin it, and how to present the result.

pub mod axis;
pub mod bounds;

pub use axis::{AxisOpts, AxisRole, AxisSpec, AxisSummary, DateInterval, Scale, ValueType};
pub use bounds::BoundsResult;
```

---

### `crates/genomehubs-query/src/report/axis.rs`

````rust
use serde::{Deserialize, Serialize};

/// Which role an axis plays in the report layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AxisRole {
    X,
    Y,
    Z,
    Cat,
}

/// Inferred or declared type of the field values.
///
/// Determines which ES aggregation is used for binning and
/// which scale types are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Numeric,
    Keyword,
    Date,
    GeoPoint,
    TaxonRank,
}

/// Scale to apply to axis values before rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    #[default]
    Linear,
    Log,
    Log2,
    Log10,
    Sqrt,
    Ordinal,
    Date,
}

/// Which summary statistic to compute when the field has multiple values per bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AxisSummary {
    #[default]
    Value,
    Min,
    Max,
    Count,
    Length,
    Mean,
    Median,
}

/// Calendar interval for date axis binning.
///
/// Maps directly to ES `calendar_interval` values.
/// When present, overrides the automatic tick-count based binning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateInterval {
    Day,
    Week,
    Month,
    Quarter,
    Year,
    Decade,
}

impl DateInterval {
    /// Return the ES `calendar_interval` string for this interval.
    pub fn to_es_interval(self) -> &'static str {
        match self {
            DateInterval::Day => "1d",
            DateInterval::Week => "1w",
            DateInterval::Month => "1M",
            DateInterval::Quarter => "3M",
            DateInterval::Year => "1y",
            DateInterval::Decade => "10y",
        }
    }
}

/// Sort mode for categorical axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortMode {
    #[default]
    Count,
    Key,
    Alpha,
}

/// Per-axis display and aggregation options.
///
/// Parsed from a `;`-delimited option string in `report_yaml`:
/// ```
/// x_opts: "fixed_values;domain_min,domain_max;size[+];scale"
/// ```
///
/// Fields:
/// - `fixed_values` — comma-separated list of forced categories (optional)
/// - `domain_min,domain_max` — numeric domain bounds (optional, skipped if blank)
/// - `size` — number of buckets; append `+` to enable "other" bucket (default 10)
/// - `scale` — one of linear, log, log2, log10, sqrt, ordinal, date (default: linear/ordinal
///             depending on ValueType)
/// - `interval` — date bucket interval: day, week, month, quarter, year, decade
/// - `sort` — count, key, alpha (default: count)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisOpts {
    pub fixed_values: Vec<String>,
    pub domain: Option<[f64; 2]>,
    pub size: usize,
    pub show_other: bool,
    pub scale: Scale,
    pub sort: SortMode,
    pub interval: Option<DateInterval>,
}

impl Default for AxisOpts {
    fn default() -> Self {
        Self {
            fixed_values: vec![],
            domain: None,
            size: 10,
            show_other: false,
            scale: Scale::Linear,
            sort: SortMode::Count,
            interval: None,
        }
    }
}

impl AxisOpts {
    /// Parse axis options from a `;`-delimited option string.
    ///
    /// Format: `"fixed_values;domain_min,domain_max;size[+];scale[;sort][;interval]"`
    ///
    /// Each segment is optional; use empty segments as placeholders:
    /// - `";;20;log10"` → size=20, scale=log10, all others default
    /// - `"Chromosome,Scaffold;;5+;ordinal"` → fixed_values with show_other
    ///
    /// Returns `AxisOpts::default()` on empty string.
    pub fn from_str(s: &str) -> Self {
        if s.is_empty() {
            return Self::default();
        }
        let parts: Vec<&str> = s.split(';').collect();
        let mut opts = Self::default();

        // Segment 0: fixed_values (comma-separated)
        if let Some(seg) = parts.first() {
            if !seg.is_empty() {
                opts.fixed_values = seg.split(',').map(|v| v.trim().to_string()).collect();
            }
        }

        // Segment 1: domain "min,max"
        if let Some(seg) = parts.get(1) {
            let bounds: Vec<f64> = seg.split(',')
                .filter_map(|v| v.trim().parse().ok())
                .collect();
            if bounds.len() == 2 {
                opts.domain = Some([bounds[0], bounds[1]]);
            }
        }

        // Segment 2: size (with optional '+' for show_other)
        if let Some(seg) = parts.get(2) {
            let seg = seg.trim();
            if seg.ends_with('+') {
                opts.show_other = true;
                if let Ok(n) = seg.trim_end_matches('+').parse() {
                    opts.size = n;
                }
            } else if let Ok(n) = seg.parse() {
                opts.size = n;
            }
        }

        // Segment 3: scale
        if let Some(seg) = parts.get(3) {
            opts.scale = parse_scale(seg.trim());
        }

        // Segment 4: sort (optional)
        if let Some(seg) = parts.get(4) {
            opts.sort = parse_sort(seg.trim());
        }

        // Segment 5: interval (optional)
        if let Some(seg) = parts.get(5) {
            opts.interval = parse_date_interval(seg.trim());
        }

        opts
    }
}

fn parse_scale(s: &str) -> Scale {
    match s {
        "log" => Scale::Log,
        "log2" => Scale::Log2,
        "log10" => Scale::Log10,
        "sqrt" => Scale::Sqrt,
        "ordinal" => Scale::Ordinal,
        "date" => Scale::Date,
        _ => Scale::Linear,
    }
}

fn parse_sort(s: &str) -> SortMode {
    match s {
        "key" => SortMode::Key,
        "alpha" => SortMode::Alpha,
        _ => SortMode::Count,
    }
}

fn parse_date_interval(s: &str) -> Option<DateInterval> {
    match s {
        "day" | "1d" => Some(DateInterval::Day),
        "week" | "1w" => Some(DateInterval::Week),
        "month" | "1M" | "1m" => Some(DateInterval::Month),
        "quarter" | "3M" | "3m" => Some(DateInterval::Quarter),
        "year" | "1y" => Some(DateInterval::Year),
        "decade" | "10y" => Some(DateInterval::Decade),
        _ => None,
    }
}

/// Full specification for one report axis.
///
/// Combines field identity, aggregation role, and display options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisSpec {
    pub field: String,
    pub role: AxisRole,
    pub summary: AxisSummary,
    pub value_type: ValueType,
    pub opts: AxisOpts,
}

impl AxisSpec {
    /// Create an axis spec with defaults.
    pub fn new(field: impl Into<String>, role: AxisRole, value_type: ValueType) -> Self {
        let default_scale = match value_type {
            ValueType::Keyword | ValueType::TaxonRank => Scale::Ordinal,
            ValueType::Date => Scale::Date,
            _ => Scale::Linear,
        };
        let mut opts = AxisOpts::default();
        opts.scale = default_scale;
        Self {
            field: field.into(),
            role,
            summary: AxisSummary::default(),
            value_type,
            opts,
        }
    }

    /// Return the default scale for this value type.
    ///
    /// `AxisOpts::from_str` can override this with an explicit scale segment.
    pub fn default_scale(&self) -> Scale {
        match self.value_type {
            ValueType::Keyword | ValueType::TaxonRank => Scale::Ordinal,
            ValueType::Date => Scale::Date,
            _ => Scale::Linear,
        }
    }
}
````

---

### `crates/genomehubs-query/src/report/bounds.rs`

This file holds only the result type. The actual computation (`compute_bounds`) lives
in `crates/genomehubs-api/src/report/bounds.rs` (Phase 5) because it requires ES I/O.

```rust
use serde::{Deserialize, Serialize};

use super::axis::{DateInterval, Scale, ValueType};

/// Resolved axis bounds returned after probing ES for a field's range.
///
/// Contains everything needed to build a histogram or date-histogram aggregation:
/// the resolved domain, appropriate tick count or interval, scale mode,
/// fixed terms (for keyword axes), and display labels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundsResult {
    /// Resolved [min, max] domain (for numeric/date axes).
    pub domain: Option<[f64; 2]>,
    /// Suggested number of histogram ticks (ignored when `interval` is set).
    pub tick_count: usize,
    /// Calendar interval for date histogram (overrides `tick_count` when Some).
    pub interval: Option<DateInterval>,
    /// Scale to apply.
    pub scale: Scale,
    /// Detected value type.
    pub value_type: ValueType,
    /// Fixed term list for keyword/ordinal axes.
    pub fixed_terms: Vec<String>,
    /// Display labels (may differ from `fixed_terms` after aliasing).
    pub cat_labels: Vec<String>,
}
```

---

## Tests

All tests live in `axis.rs` under `#[cfg(test)]`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_axis_opts_has_expected_values() {
        let opts = AxisOpts::default();
        assert_eq!(opts.size, 10);
        assert!(!opts.show_other);
        assert_eq!(opts.scale, Scale::Linear);
    }

    #[test]
    fn parse_empty_opts_string_returns_default() {
        let opts = AxisOpts::from_str("");
        assert_eq!(opts.size, 10);
        assert!(opts.fixed_values.is_empty());
    }

    #[test]
    fn parse_size_only() {
        let opts = AxisOpts::from_str(";;20;");
        assert_eq!(opts.size, 20);
        assert!(!opts.show_other);
    }

    #[test]
    fn parse_size_with_show_other() {
        let opts = AxisOpts::from_str(";;5+;");
        assert_eq!(opts.size, 5);
        assert!(opts.show_other);
    }

    #[test]
    fn parse_log10_scale() {
        let opts = AxisOpts::from_str(";;20;log10");
        assert_eq!(opts.scale, Scale::Log10);
        assert_eq!(opts.size, 20);
    }

    #[test]
    fn parse_fixed_values() {
        let opts = AxisOpts::from_str("Chromosome,Scaffold;;5+;ordinal");
        assert_eq!(opts.fixed_values, vec!["Chromosome", "Scaffold"]);
        assert_eq!(opts.size, 5);
        assert!(opts.show_other);
        assert_eq!(opts.scale, Scale::Ordinal);
    }

    #[test]
    fn parse_domain() {
        let opts = AxisOpts::from_str(";0.0,100.0;10;linear");
        assert_eq!(opts.domain, Some([0.0, 100.0]));
    }

    #[test]
    fn parse_date_interval_month() {
        let opts = AxisOpts::from_str(";;12;date;count;month");
        assert_eq!(opts.interval, Some(DateInterval::Month));
    }

    #[test]
    fn date_interval_to_es_string() {
        assert_eq!(DateInterval::Month.to_es_interval(), "1M");
        assert_eq!(DateInterval::Quarter.to_es_interval(), "3M");
        assert_eq!(DateInterval::Decade.to_es_interval(), "10y");
    }

    // Proptest round-trip: serialise → deserialise must be stable
    // (add proptest/quickcheck test for AxisOpts serde if proptest feature is enabled)
}
```

---

## Verification

```bash
# Type-check only (no I/O needed)
cargo check -p genomehubs-query
cargo test -p genomehubs-query report

# Ensure WASM compile still works
wasm-pack build crates/genomehubs-query --target nodejs --features wasm -- --no-default-features
```

---

## Completion Checklist

- [ ] `crates/genomehubs-query/src/report/mod.rs` created
- [ ] `crates/genomehubs-query/src/report/axis.rs` created with all types
- [ ] `AxisOpts::from_str` parses all 6 segments correctly
- [ ] `DateInterval::to_es_interval` returns correct ES strings
- [ ] `crates/genomehubs-query/src/report/bounds.rs` created (type only)
- [ ] `pub mod report` declared in `crates/genomehubs-query/src/lib.rs`
- [ ] All unit tests pass: `cargo test -p genomehubs-query report`
- [ ] `cargo check -p genomehubs-api` still passes (no downstream breaks)
- [ ] WASM build still succeeds
