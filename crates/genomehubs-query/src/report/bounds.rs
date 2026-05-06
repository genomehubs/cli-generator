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

impl BoundsResult {
    /// Create a new BoundsResult for a numeric axis.
    pub fn numeric(domain: [f64; 2], tick_count: usize, scale: Scale) -> Self {
        Self {
            domain: Some(domain),
            tick_count,
            interval: None,
            scale,
            value_type: ValueType::Numeric,
            fixed_terms: vec![],
            cat_labels: vec![],
        }
    }

    /// Create a new BoundsResult for a keyword/categorical axis.
    pub fn categorical(fixed_terms: Vec<String>, cat_labels: Vec<String>) -> Self {
        Self {
            domain: None,
            tick_count: fixed_terms.len().max(1),
            interval: None,
            scale: super::axis::Scale::Ordinal,
            value_type: ValueType::Keyword,
            fixed_terms,
            cat_labels,
        }
    }

    /// Create a new BoundsResult for a date axis.
    pub fn date(domain: [f64; 2], interval: Option<DateInterval>, tick_count: usize) -> Self {
        Self {
            domain: Some(domain),
            tick_count,
            interval,
            scale: Scale::Date,
            value_type: ValueType::Date,
            fixed_terms: vec![],
            cat_labels: vec![],
        }
    }
}
