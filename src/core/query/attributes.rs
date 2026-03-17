//! Attribute types for [`SearchQuery`](super::SearchQuery).
//!
//! Covers attribute filters, return fields, taxon name classes, and rank columns.

use serde::{Deserialize, Serialize};

// ── AttributeSet ──────────────────────────────────────────────────────────────

/// The full set of attribute-related query parameters.
///
/// Corresponds to the `process_attributes` artifact in the GoaT MCP server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttributeSet {
    /// Attribute filter conditions (e.g. `genome_size < 3G`).
    #[serde(default)]
    pub attributes: Vec<Attribute>,
    /// Columns to return in search results.
    #[serde(default)]
    pub fields: Vec<Field>,
    /// Taxon name classes to include (maps to `&names=`, NOT `&fields=`).
    ///
    /// Valid values are site-configured; GoaT defaults:
    /// `scientific_name`, `common_name`, `synonym`, `tolid_prefix`, `authority`.
    /// Supports filter suffixes like `"common_name:*bat*"`.
    #[serde(default)]
    pub names: Vec<String>,
    /// Taxonomic rank columns to include in results (maps to `&ranks=`).
    ///
    /// Use `--ranks` on the CLI to add ancestor rank columns to the output.
    /// Distinct from `Identifiers::rank` which filters *which* rank is returned
    /// (gap-analysis item 4).
    #[serde(default)]
    pub ranks: Vec<String>,
}

// ── Attribute ─────────────────────────────────────────────────────────────────

/// A single attribute filter or presence test.
///
/// The `name` field may be a synonym; validation normalises it to the
/// canonical API name using the generated synonym → canonical lookup table
/// in `field_meta.rs`.
///
/// # Examples
/// ```yaml
/// # Numeric comparison with modifiers
/// name: genome_size
/// operator: lt
/// value: "3G"
/// modifier: [min, direct]
///
/// # Existence test
/// name: assembly_level
/// operator: exists
///
/// # Enum membership
/// name: long_list
/// operator: eq
/// value: [DTOL, CANBP]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    /// Attribute name; may be a synonym.
    pub name: String,
    /// Comparison operator; `None` means an existence test.
    #[serde(default)]
    pub operator: Option<AttributeOperator>,
    /// Value to compare against; `None` when using `exists` / `missing`.
    #[serde(default)]
    pub value: Option<AttributeValue>,
    /// Summary and/or status modifiers.
    ///
    /// Summary modifiers (`min`, `max`, …) wrap the field name as
    /// `summary(field)` in the query string.  Status modifiers (`direct`,
    /// `ancestral`, …) are converted to `&excludeXxx[N]=field` URL params.
    #[serde(default)]
    pub modifier: Vec<Modifier>,
}

// ── Field ─────────────────────────────────────────────────────────────────────

/// A single column to return in search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// Field name; may be a synonym, normalised during validation.
    pub name: String,
    /// Optional modifiers (e.g. `min`, `direct`) for this return column.
    #[serde(default)]
    pub modifier: Vec<Modifier>,
}

// ── AttributeOperator ─────────────────────────────────────────────────────────

/// Comparison operator for an attribute filter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttributeOperator {
    /// `=`  (equality / set membership)
    Eq,
    /// `!=` (inequality / exclusion)
    Ne,
    /// `<`  (not valid for keyword-type fields)
    Lt,
    /// `<=` (not valid for keyword-type fields)
    Le,
    /// `>`  (not valid for keyword-type fields)
    Gt,
    /// `>=` (not valid for keyword-type fields)
    Ge,
    /// Test for presence of any value (no `value` field needed).
    Exists,
    /// Test for absence of any value (no `value` field needed).
    Missing,
}

impl AttributeOperator {
    /// Return the raw operator string used in query fragments.
    pub fn as_str(&self) -> &'static str {
        match self {
            AttributeOperator::Eq => "=",
            AttributeOperator::Ne => "!=",
            AttributeOperator::Lt => "<",
            AttributeOperator::Le => "<=",
            AttributeOperator::Gt => ">",
            AttributeOperator::Ge => ">=",
            AttributeOperator::Exists => "",
            AttributeOperator::Missing => "",
        }
    }
}

// ── AttributeValue ────────────────────────────────────────────────────────────

/// Attribute filter value: a single string or a list for set membership tests.
///
/// Size suffix strings (e.g. `"3G"`, `"500M"`, `"1K"`) are expanded to byte
/// counts during validation, before URL encoding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AttributeValue {
    /// Single scalar value, e.g. `"chromosome"` or `"3G"`.
    Single(String),
    /// Multiple values for set membership (`in` / `not in`) tests.
    List(Vec<String>),
}

impl AttributeValue {
    /// Return a single-item slice or the full list as a `Vec<&str>`.
    pub fn as_strs(&self) -> Vec<&str> {
        match self {
            AttributeValue::Single(s) => vec![s.as_str()],
            AttributeValue::List(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

// ── Modifier ──────────────────────────────────────────────────────────────────

/// A modifier applied to an attribute name or field.
///
/// **Summary modifiers** aggregate values across traversal and keep the field
/// name wrapped as `summary(field)` in the query string.
///
/// **Status modifiers** control which values are included/excluded based on
/// their provenance in the taxonomy tree, converting to
/// `&excludeXxx[N]=field` URL params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    // ── summary ───────────────────────────────────────────────────────────────
    Min,
    Max,
    Median,
    Mean,
    Sum,
    List,
    Length,
    // ── status ────────────────────────────────────────────────────────────────
    Direct,
    Ancestral,
    Descendant,
    Estimated,
    Missing,
}

impl Modifier {
    /// Return `true` if this modifier is a status modifier (maps to an exclude param).
    pub fn is_status(&self) -> bool {
        matches!(
            self,
            Modifier::Direct
                | Modifier::Ancestral
                | Modifier::Descendant
                | Modifier::Estimated
                | Modifier::Missing
        )
    }

    /// Return `true` if this modifier is a summary modifier (wraps field name).
    pub fn is_summary(&self) -> bool {
        !self.is_status()
    }

    /// Return the API name for use in `summary(field)` wrapping or exclude params.
    pub fn as_str(&self) -> &'static str {
        match self {
            Modifier::Min => "min",
            Modifier::Max => "max",
            Modifier::Median => "median",
            Modifier::Mean => "mean",
            Modifier::Sum => "sum",
            Modifier::List => "list",
            Modifier::Length => "length",
            Modifier::Direct => "Direct",
            Modifier::Ancestral => "Ancestral",
            Modifier::Descendant => "Descendant",
            Modifier::Estimated => "Estimated",
            Modifier::Missing => "Missing",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_classification() {
        assert!(Modifier::Min.is_summary());
        assert!(Modifier::Max.is_summary());
        assert!(Modifier::Direct.is_status());
        assert!(Modifier::Ancestral.is_status());
        assert!(!Modifier::Min.is_status());
        assert!(!Modifier::Direct.is_summary());
    }

    #[test]
    fn attribute_value_as_strs() {
        let single = AttributeValue::Single("3G".to_string());
        assert_eq!(single.as_strs(), vec!["3G"]);

        let list = AttributeValue::List(vec!["DTOL".to_string(), "CANBP".to_string()]);
        assert_eq!(list.as_strs(), vec!["DTOL", "CANBP"]);
    }

    #[test]
    fn attribute_operator_strings() {
        assert_eq!(AttributeOperator::Eq.as_str(), "=");
        assert_eq!(AttributeOperator::Lt.as_str(), "<");
        assert_eq!(AttributeOperator::Ge.as_str(), ">=");
    }

    #[test]
    fn attribute_deserialises_from_yaml() {
        let yaml =
            "name: genome_size\noperator: lt\nvalue: \"3000000000\"\nmodifier: [min, direct]";
        let attr: Attribute = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(attr.name, "genome_size");
        assert_eq!(attr.operator, Some(AttributeOperator::Lt));
        assert_eq!(attr.modifier, vec![Modifier::Min, Modifier::Direct]);
    }
}
