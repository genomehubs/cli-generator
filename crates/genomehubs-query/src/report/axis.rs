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

/// Custom boundaries for histogram binning.
///
/// For numeric axes: explicit breakpoints defining bucket ranges.
/// For date axes: calendar intervals or explicit ISO 8601 timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Boundaries {
    /// Numeric boundaries: sorted list of breakpoints
    Numeric(Vec<f64>),
    /// Date boundaries: intervals, explicit timestamps, or both
    Date {
        #[serde(default)]
        intervals: Option<Vec<String>>, // ["day", "week", "month", etc.]
        #[serde(default)]
        explicit: Option<Vec<String>>, // ISO 8601 timestamps ["2020-01-01", ...]
    },
}

/// Per-axis display and aggregation options.
///
/// **Numeric axis format** (either `;` or `,` separated):
/// ```text
/// Semicolon:  min;max;count[+];scale;sort;interval
/// Comma:      min,max,count,scale,caption
/// ```
///
/// **Categorical axis format** (empty segment 1 indicates categorical):
/// ```text
/// Semicolon:  values;;count[+];scale;sort;interval
/// ```
/// Where `values` = `value1,value2@Label2,value3`
/// - Without `@` → label equals the value
/// - With `@` → use the provided label for display
///
/// **Backward-compatible cat field format** (parsed separately):
/// ```text
/// field[N]=value1,value2@Label
/// field[N+]=value1,value2@Label
/// field[N+]=value1,value2@Label
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisOpts {
    /// Minimum domain value (numeric axes)
    pub min: Option<String>,
    /// Maximum domain value (numeric axes)
    pub max: Option<String>,
    /// Fixed categorical values with optional labels
    pub fixed_values: Vec<(String, String)>, // (value, label) pairs
    pub size: usize,
    pub show_other: bool,
    pub scale: Scale,
    pub sort: SortMode,
    pub interval: Option<DateInterval>,
    /// Custom boundaries for binning (new)
    #[serde(default)]
    pub boundaries: Option<Boundaries>,
    /// Custom labels for boundary buckets (new)
    #[serde(default)]
    pub labels: Option<Vec<String>>,
}

impl Default for AxisOpts {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            fixed_values: vec![],
            size: 10,
            show_other: false,
            scale: Scale::Linear,
            sort: SortMode::Count,
            interval: None,
            boundaries: None,
            labels: None,
        }
    }
}

impl std::str::FromStr for AxisOpts {
    type Err = std::convert::Infallible;

    /// Parse axis options from a string using the standard [`FromStr`] trait.
    ///
    /// See [`AxisOpts::parse`] for format documentation.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(AxisOpts::parse(s))
    }
}

impl AxisOpts {
    /// Parse axis options from a string.
    ///
    /// **Numeric format** (both `;` and `,` separated):
    /// - `"min;max;count;scale;sort;interval"` (semicolon-separated)
    /// - `"min,max,count,scale"` (comma-separated, legacy)
    ///
    /// **Categorical format** (distinguished by empty segment 1 with `;`):
    /// - `"value1,value2@Label;;count;scale;sort;interval"`
    ///
    /// Examples:
    /// - `";;20;log10"` → numeric, size=20, scale=log10
    /// - `"100;1000;5+;log10"` → numeric, min=100, max=1000, size=5, show_other=true
    /// - `"10,20,30;;5;ordinal"` → categorical, fixed_values=[(10,"10"), (20,"20"), (30,"30")]
    /// - `"contig,scaffold@Scaffold;;5+"` → categorical with label translation
    pub fn parse(s: &str) -> Self {
        if s.is_empty() {
            return Self::default();
        }

        let mut opts = Self::default();

        // Determine separator: check if `;` is present
        let has_semicolon = s.contains(';');

        if has_semicolon {
            let parts: Vec<&str> = s.split(';').collect();

            // Segment 0: min (or fixed_values if categorical)
            if let Some(seg) = parts.first() {
                if !seg.is_empty() {
                    // Check if this looks like numeric min or categorical values
                    // If segment 1 is empty and segment 0 has commas, it's categorical
                    if parts.len() > 1 && parts[1].is_empty() && seg.contains(',') {
                        // Categorical with values
                        parse_fixed_values(seg, &mut opts);
                    } else if let Ok(val) = seg.parse::<f64>() {
                        // Numeric minimum
                        opts.min = Some(val.to_string());
                    } else if seg.contains(',') && !seg.contains('@') {
                        // Multiple values without labels → categorical
                        parse_fixed_values(seg, &mut opts);
                    } else {
                        // Could be categorical value(s)
                        parse_fixed_values(seg, &mut opts);
                    }
                }
            }

            // Segment 1: max (or empty for categorical)
            if let Some(seg) = parts.get(1) {
                if !seg.is_empty() {
                    if let Ok(val) = seg.parse::<f64>() {
                        opts.max = Some(val.to_string());
                    }
                }
            }

            // Segment 2: size (with optional '+' for show_other)
            if let Some(seg) = parts.get(2) {
                parse_size(seg, &mut opts);
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
        } else {
            // Comma-separated format: min,max,count,scale,caption
            let parts: Vec<&str> = s.split(',').collect();

            // Segment 0: min
            if let Some(seg) = parts.first() {
                if !seg.is_empty() {
                    if let Ok(val) = seg.parse::<f64>() {
                        opts.min = Some(val.to_string());
                    }
                }
            }

            // Segment 1: max
            if let Some(seg) = parts.get(1) {
                if !seg.is_empty() {
                    if let Ok(val) = seg.parse::<f64>() {
                        opts.max = Some(val.to_string());
                    }
                }
            }

            // Segment 2: count (size)
            if let Some(seg) = parts.get(2) {
                parse_size(seg, &mut opts);
            }

            // Segment 3: scale
            if let Some(seg) = parts.get(3) {
                opts.scale = parse_scale(seg.trim());
            }

            // Segment 4: caption (ignored for now, reserved)
            // if let Some(_seg) = parts.get(4) { }
        }

        opts
    }
}

/// Parse a size segment which may end with `+` to indicate show_other.
fn parse_size(seg: &str, opts: &mut AxisOpts) {
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

/// Parse fixed values from a comma-separated string with optional `@Label` translations.
/// Format: `value1,value2@Label2,value3`
fn parse_fixed_values(seg: &str, opts: &mut AxisOpts) {
    for value_part in seg.split(',') {
        let value_part = value_part.trim();
        if value_part.is_empty() {
            continue;
        }
        // Check for `value@Label` syntax
        if let Some(at_pos) = value_part.find('@') {
            let value = value_part[..at_pos].to_string();
            let label = value_part[at_pos + 1..].to_string();
            opts.fixed_values.push((value, label));
        } else {
            // No label provided; use value as label
            opts.fixed_values
                .push((value_part.to_string(), value_part.to_string()));
        }
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
        Self {
            field: field.into(),
            role,
            summary: AxisSummary::default(),
            value_type,
            opts: AxisOpts {
                scale: default_scale,
                ..Default::default()
            },
        }
    }

    /// Return the default scale for this value type.
    ///
    /// `AxisOpts::from_str` can override this with an explicit scale segment.
    pub fn default_scale(&self) -> Scale {
        default_scale_for(effective_value_type(self.value_type, self.summary))
    }
}

/// Compute the effective value type after applying a summary function.
///
/// Some summaries change the data type of the result:
/// - `Length` counts entries in a list → always `Numeric`
/// - `Count` counts non-null values → always `Numeric`
/// - `Mean`, `Median`, `Min`, `Max` aggregate numerically → `Numeric`
/// - `Value` preserves the field's natural type unchanged
pub fn effective_value_type(field_type: ValueType, summary: AxisSummary) -> ValueType {
    match summary {
        AxisSummary::Length
        | AxisSummary::Count
        | AxisSummary::Mean
        | AxisSummary::Median
        | AxisSummary::Min
        | AxisSummary::Max => ValueType::Numeric,
        AxisSummary::Value => field_type,
    }
}

/// Return the default scale for a given value type.
///
/// Pass the *effective* type (after [`effective_value_type`]) so that a
/// `length` summary on a keyword list defaults to `linear`, not `ordinal`.
pub fn default_scale_for(effective: ValueType) -> Scale {
    match effective {
        ValueType::Keyword | ValueType::TaxonRank => Scale::Ordinal,
        ValueType::Date => Scale::Date,
        _ => Scale::Linear,
    }
}

/// One entry in the `values` list of an [`AxisInput`].
///
/// Accepts either a bare string `"Chromosome"` or an explicit
/// `{ "value": "chromosome", "label": "Chromosome-level" }` object.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AxisValueInput {
    /// A bare string used as both value and display label.
    Simple(String),
    /// An explicit value with an optional display label override.
    Labeled {
        value: String,
        label: Option<String>,
    },
}

/// One axis entry from the structured `axes` array in a POST report body.
///
/// Convert to [`AxisSpec`] via [`AxisInput::into_spec`], supplying the
/// metadata-inferred value type for the field.
#[derive(Debug, Clone, Deserialize)]
pub struct AxisInput {
    /// Attribute name or taxon rank field.
    pub field: String,
    /// Role in the report layout: `x`, `y`, `z`, or `cat`.
    pub position: AxisRole,
    /// Value type override. Inferred from metadata when absent.
    #[serde(rename = "type")]
    pub value_type: Option<ValueType>,
    /// Scale for axis rendering. Defaults based on effective value type.
    pub scale: Option<Scale>,
    /// Number of histogram bins (numeric) or top-N categories (keyword/rank).
    pub bin_count: Option<usize>,
    /// Include an "other" bucket for categories not in the top N.
    pub show_other: Option<bool>,
    /// Domain minimum clamp (string to preserve decimal precision).
    pub min: Option<String>,
    /// Domain maximum clamp.
    pub max: Option<String>,
    /// Sort mode for categorical axes.
    pub sort: Option<SortMode>,
    /// Calendar interval for date axes.
    pub interval: Option<DateInterval>,
    /// Summary statistic when a field has multiple values per record.
    pub summary: Option<AxisSummary>,
    /// Explicit category values (categorical/rank axes).
    pub values: Option<Vec<AxisValueInput>>,
}

impl AxisInput {
    /// Convert to [`AxisSpec`] using `inferred_type` when `type` is not set.
    ///
    /// The *effective* type (after applying `summary`) drives the default scale:
    /// a `length` summary on a keyword list becomes numeric, so it defaults to
    /// `linear` rather than `ordinal`. The raw `field_type` is stored in
    /// `AxisSpec.value_type` so aggregation builders can still select the correct
    /// ES field path (e.g. `attributes.keyword_value` for keyword fields).
    pub fn into_spec(self, inferred_type: ValueType) -> AxisSpec {
        let field_type = self.value_type.unwrap_or(inferred_type);
        let summary = self.summary.unwrap_or_default();
        let effective = effective_value_type(field_type, summary);
        let scale = self.scale.unwrap_or_else(|| default_scale_for(effective));

        let fixed_values = self
            .values
            .unwrap_or_default()
            .into_iter()
            .map(|v| match v {
                AxisValueInput::Simple(s) => (s.clone(), s),
                AxisValueInput::Labeled { value, label } => (value.clone(), label.unwrap_or(value)),
            })
            .collect();

        AxisSpec {
            field: self.field,
            role: self.position,
            summary,
            value_type: field_type,
            opts: AxisOpts {
                min: self.min,
                max: self.max,
                fixed_values,
                size: self.bin_count.unwrap_or(10),
                show_other: self.show_other.unwrap_or(false),
                scale,
                sort: self.sort.unwrap_or_default(),
                interval: self.interval,
                boundaries: None,
                labels: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_axis_opts_has_expected_values() {
        let opts = AxisOpts::default();
        assert_eq!(opts.size, 10);
        assert!(!opts.show_other);
        assert_eq!(opts.scale, Scale::Linear);
        assert!(opts.fixed_values.is_empty());
    }

    #[test]
    fn parse_empty_opts_string_returns_default() {
        let opts = AxisOpts::parse("");
        assert_eq!(opts.size, 10);
        assert!(opts.fixed_values.is_empty());
    }

    // ── Numeric (semicolon) tests ──

    #[test]
    fn parse_numeric_size_only() {
        let opts = AxisOpts::parse(";;20;");
        assert_eq!(opts.size, 20);
        assert!(!opts.show_other);
        assert!(opts.min.is_none());
    }

    #[test]
    fn parse_numeric_size_with_show_other() {
        let opts = AxisOpts::parse(";;5+;");
        assert_eq!(opts.size, 5);
        assert!(opts.show_other);
    }

    #[test]
    fn parse_numeric_with_log10_scale() {
        let opts = AxisOpts::parse(";;20;log10");
        assert_eq!(opts.scale, Scale::Log10);
        assert_eq!(opts.size, 20);
    }

    #[test]
    fn parse_numeric_with_min_max() {
        let opts = AxisOpts::parse("100;1000;5;linear");
        assert_eq!(opts.min, Some("100".to_string()));
        assert_eq!(opts.max, Some("1000".to_string()));
        assert_eq!(opts.size, 5);
    }

    #[test]
    fn parse_numeric_with_all_segments() {
        let opts = AxisOpts::parse("0;100;10;log10;key;month");
        assert_eq!(opts.min, Some("0".to_string()));
        assert_eq!(opts.max, Some("100".to_string()));
        assert_eq!(opts.size, 10);
        assert_eq!(opts.scale, Scale::Log10);
        assert_eq!(opts.sort, SortMode::Key);
        assert_eq!(opts.interval, Some(DateInterval::Month));
    }

    // ── Numeric (comma) tests ──

    #[test]
    fn parse_numeric_comma_format() {
        let opts = AxisOpts::parse("10,100,5,linear");
        assert_eq!(opts.min, Some("10".to_string()));
        assert_eq!(opts.max, Some("100".to_string()));
        assert_eq!(opts.size, 5);
        assert_eq!(opts.scale, Scale::Linear);
    }

    // ── Categorical (semicolon with empty segment 1) tests ──

    #[test]
    fn parse_categorical_with_fixed_values() {
        let opts = AxisOpts::parse("Chromosome,Scaffold;;5+;ordinal");
        assert_eq!(opts.fixed_values.len(), 2);
        assert_eq!(
            opts.fixed_values[0],
            ("Chromosome".to_string(), "Chromosome".to_string())
        );
        assert_eq!(
            opts.fixed_values[1],
            ("Scaffold".to_string(), "Scaffold".to_string())
        );
        assert_eq!(opts.size, 5);
        assert!(opts.show_other);
        assert_eq!(opts.scale, Scale::Ordinal);
    }

    #[test]
    fn parse_categorical_with_label_translations() {
        let opts = AxisOpts::parse("contig,scaffold@Scaffold;;5+");
        assert_eq!(opts.fixed_values.len(), 2);
        assert_eq!(
            opts.fixed_values[0],
            ("contig".to_string(), "contig".to_string())
        );
        assert_eq!(
            opts.fixed_values[1],
            ("scaffold".to_string(), "Scaffold".to_string())
        );
        assert_eq!(opts.size, 5);
        assert!(opts.show_other);
    }

    #[test]
    fn parse_categorical_multiple_labels() {
        let opts = AxisOpts::parse("contig@Contig,scaffold@Scaffold,complete@Complete;;10");
        assert_eq!(opts.fixed_values.len(), 3);
        assert_eq!(
            opts.fixed_values[0],
            ("contig".to_string(), "Contig".to_string())
        );
        assert_eq!(
            opts.fixed_values[1],
            ("scaffold".to_string(), "Scaffold".to_string())
        );
        assert_eq!(
            opts.fixed_values[2],
            ("complete".to_string(), "Complete".to_string())
        );
        assert_eq!(opts.size, 10);
    }

    // ── Scale parsing ──

    #[test]
    fn parse_all_scales() {
        let scales = vec![
            ("log", Scale::Log),
            ("log2", Scale::Log2),
            ("log10", Scale::Log10),
            ("sqrt", Scale::Sqrt),
            ("ordinal", Scale::Ordinal),
            ("date", Scale::Date),
            ("linear", Scale::Linear),
        ];
        for (scale_str, expected_scale) in scales {
            let opts = AxisOpts::parse(&format!(";;;{}", scale_str));
            assert_eq!(opts.scale, expected_scale);
        }
    }

    // ── Sort modes ──

    #[test]
    fn parse_sort_modes() {
        let opts_count = AxisOpts::parse(";;;linear;count");
        let opts_key = AxisOpts::parse(";;;linear;key");
        let opts_alpha = AxisOpts::parse(";;;linear;alpha");
        assert_eq!(opts_count.sort, SortMode::Count);
        assert_eq!(opts_key.sort, SortMode::Key);
        assert_eq!(opts_alpha.sort, SortMode::Alpha);
    }

    // ── Date intervals ──

    #[test]
    fn parse_date_intervals() {
        let intervals = vec![
            ("day", DateInterval::Day),
            ("1d", DateInterval::Day),
            ("week", DateInterval::Week),
            ("1w", DateInterval::Week),
            ("month", DateInterval::Month),
            ("1M", DateInterval::Month),
            ("quarter", DateInterval::Quarter),
            ("3M", DateInterval::Quarter),
            ("year", DateInterval::Year),
            ("1y", DateInterval::Year),
            ("decade", DateInterval::Decade),
            ("10y", DateInterval::Decade),
        ];
        for (interval_str, expected_interval) in intervals {
            let opts = AxisOpts::parse(&format!(";;;;;{}", interval_str));
            assert_eq!(
                opts.interval,
                Some(expected_interval),
                "Failed for interval: {}",
                interval_str
            );
        }
    }

    #[test]
    fn date_interval_to_es_string() {
        assert_eq!(DateInterval::Day.to_es_interval(), "1d");
        assert_eq!(DateInterval::Week.to_es_interval(), "1w");
        assert_eq!(DateInterval::Month.to_es_interval(), "1M");
        assert_eq!(DateInterval::Quarter.to_es_interval(), "3M");
        assert_eq!(DateInterval::Year.to_es_interval(), "1y");
        assert_eq!(DateInterval::Decade.to_es_interval(), "10y");
    }

    // ── AxisSpec tests ──

    #[test]
    fn axis_spec_default_scale_numeric() {
        let spec = AxisSpec::new("field", AxisRole::X, ValueType::Numeric);
        assert_eq!(spec.default_scale(), Scale::Linear);
    }

    #[test]
    fn axis_spec_default_scale_keyword() {
        let spec = AxisSpec::new("field", AxisRole::X, ValueType::Keyword);
        assert_eq!(spec.default_scale(), Scale::Ordinal);
    }

    #[test]
    fn axis_spec_default_scale_date() {
        let spec = AxisSpec::new("field", AxisRole::X, ValueType::Date);
        assert_eq!(spec.default_scale(), Scale::Date);
    }

    // ── Serde roundtrip tests ──

    #[test]
    fn serde_axis_opts_numeric_roundtrip() {
        let opts = AxisOpts::parse("10;100;5;log10;key;month");
        let json = serde_json::to_string(&opts).expect("serialize");
        let restored: AxisOpts = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.min, opts.min);
        assert_eq!(restored.max, opts.max);
        assert_eq!(restored.size, opts.size);
        assert_eq!(restored.scale, opts.scale);
        assert_eq!(restored.sort, opts.sort);
        assert_eq!(restored.interval, opts.interval);
    }

    #[test]
    fn serde_axis_opts_categorical_roundtrip() {
        let opts = AxisOpts::parse("contig@Contig,scaffold@Scaffold;;5+;ordinal;count");
        let json = serde_json::to_string(&opts).expect("serialize");
        let restored: AxisOpts = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.fixed_values, opts.fixed_values);
        assert_eq!(restored.size, opts.size);
        assert_eq!(restored.show_other, opts.show_other);
        assert_eq!(restored.scale, opts.scale);
    }

    #[test]
    fn serde_axis_spec_roundtrip() {
        let spec = AxisSpec::new("genome_size", AxisRole::X, ValueType::Numeric);
        let json = serde_json::to_string(&spec).expect("serialize");
        let restored: AxisSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.field, spec.field);
        assert_eq!(restored.role, spec.role);
        assert_eq!(restored.value_type, spec.value_type);
    }

    // ── effective_value_type ──

    #[test]
    fn effective_type_preserves_keyword_for_value_summary() {
        assert_eq!(
            effective_value_type(ValueType::Keyword, AxisSummary::Value),
            ValueType::Keyword
        );
    }

    #[test]
    fn effective_type_coerces_keyword_to_numeric_for_length() {
        assert_eq!(
            effective_value_type(ValueType::Keyword, AxisSummary::Length),
            ValueType::Numeric
        );
    }

    #[test]
    fn effective_type_coerces_any_type_for_count_and_aggregates() {
        for summary in [
            AxisSummary::Count,
            AxisSummary::Mean,
            AxisSummary::Median,
            AxisSummary::Min,
            AxisSummary::Max,
        ] {
            assert_eq!(
                effective_value_type(ValueType::Keyword, summary),
                ValueType::Numeric,
                "failed for summary {:?}",
                summary
            );
        }
    }

    // ── default_scale_for ──

    #[test]
    fn default_scale_for_numeric_is_linear() {
        assert_eq!(default_scale_for(ValueType::Numeric), Scale::Linear);
    }

    #[test]
    fn default_scale_for_keyword_is_ordinal() {
        assert_eq!(default_scale_for(ValueType::Keyword), Scale::Ordinal);
    }

    #[test]
    fn default_scale_for_date_is_date() {
        assert_eq!(default_scale_for(ValueType::Date), Scale::Date);
    }

    // ── AxisInput::into_spec ──

    #[test]
    fn axis_input_numeric_field_defaults_to_linear() {
        let input: AxisInput =
            serde_json::from_str(r#"{"field":"genome_size","position":"x"}"#).unwrap();
        let spec = input.into_spec(ValueType::Numeric);
        assert_eq!(spec.opts.scale, Scale::Linear);
        assert_eq!(spec.role, AxisRole::X);
    }

    #[test]
    fn axis_input_keyword_field_defaults_to_ordinal() {
        let input: AxisInput =
            serde_json::from_str(r#"{"field":"assembly_level","position":"cat"}"#).unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        assert_eq!(spec.opts.scale, Scale::Ordinal);
        assert_eq!(spec.role, AxisRole::Cat);
    }

    #[test]
    fn axis_input_length_summary_on_keyword_gives_linear_scale() {
        let input: AxisInput =
            serde_json::from_str(r#"{"field":"common_names","position":"x","summary":"length"}"#)
                .unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        // effective type is Numeric → default scale is Linear, not Ordinal
        assert_eq!(spec.opts.scale, Scale::Linear);
        // stored value_type is still Keyword for correct ES field path selection
        assert_eq!(spec.value_type, ValueType::Keyword);
    }

    #[test]
    fn axis_input_explicit_scale_overrides_summary_default() {
        let input: AxisInput = serde_json::from_str(
            r#"{"field":"common_names","position":"x","summary":"length","scale":"log10"}"#,
        )
        .unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        assert_eq!(spec.opts.scale, Scale::Log10);
    }

    #[test]
    fn axis_input_bin_count_and_show_other() {
        let input: AxisInput = serde_json::from_str(
            r#"{"field":"assembly_level","position":"cat","bin_count":3,"show_other":true}"#,
        )
        .unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        assert_eq!(spec.opts.size, 3);
        assert!(spec.opts.show_other);
    }

    #[test]
    fn axis_input_simple_values_list() {
        let input: AxisInput = serde_json::from_str(
            r#"{"field":"assembly_level","position":"cat","values":["Chromosome","Scaffold"]}"#,
        )
        .unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        assert_eq!(spec.opts.fixed_values.len(), 2);
        assert_eq!(
            spec.opts.fixed_values[0],
            ("Chromosome".to_string(), "Chromosome".to_string())
        );
    }

    #[test]
    fn axis_input_labeled_values_list() {
        let input: AxisInput = serde_json::from_str(
            r#"{"field":"assembly_level","position":"cat",
               "values":[{"value":"chromosome","label":"Chromosome-level"},{"value":"scaffold"}]}"#,
        )
        .unwrap();
        let spec = input.into_spec(ValueType::Keyword);
        assert_eq!(
            spec.opts.fixed_values[0],
            ("chromosome".to_string(), "Chromosome-level".to_string())
        );
        assert_eq!(
            spec.opts.fixed_values[1],
            ("scaffold".to_string(), "scaffold".to_string())
        );
    }
}
