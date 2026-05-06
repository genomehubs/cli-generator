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
        match self.value_type {
            ValueType::Keyword | ValueType::TaxonRank => Scale::Ordinal,
            ValueType::Date => Scale::Date,
            _ => Scale::Linear,
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
}
