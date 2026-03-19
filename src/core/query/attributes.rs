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

    #[test]
    fn attribute_set_default_has_empty_collections() {
        let set = AttributeSet::default();
        assert!(set.attributes.is_empty());
        assert!(set.fields.is_empty());
        assert!(set.names.is_empty());
        assert!(set.ranks.is_empty());
    }

    #[test]
    fn field_deserialises_from_yaml() {
        let yaml = "name: gc_percentage\nmodifier: [max, direct]";
        let field: Field = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(field.name, "gc_percentage");
        assert_eq!(field.modifier, vec![Modifier::Max, Modifier::Direct]);
    }

    #[test]
    fn attribute_value_single_serialises() {
        let val = AttributeValue::Single("3G".to_string());
        let json = serde_json::to_value(&val).unwrap();
        assert_eq!(json, serde_json::json!("3G"));
    }

    #[test]
    fn attribute_value_list_serialises() {
        let val = AttributeValue::List(vec!["DTOL".to_string(), "CANBP".to_string()]);
        let json = serde_json::to_value(&val).unwrap();
        assert_eq!(json, serde_json::json!(["DTOL", "CANBP"]));
    }

    #[test]
    fn attribute_operator_missing_operator_as_str() {
        // Missing operator should have empty string
        assert_eq!(AttributeOperator::Missing.as_str(), "");
    }

    #[test]
    fn attribute_operator_exists_operator_as_str() {
        // Exists operator should have empty string
        assert_eq!(AttributeOperator::Exists.as_str(), "");
    }

    #[test]
    fn attribute_with_no_operator_or_value() {
        let yaml = "name: assembly_level";
        let attr: Attribute = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(attr.name, "assembly_level");
        assert_eq!(attr.operator, None);
        assert_eq!(attr.value, None);
        assert!(attr.modifier.is_empty());
    }

    #[test]
    fn attribute_value_list_empty() {
        let val = AttributeValue::List(vec![]);
        assert!(val.as_strs().is_empty());
    }

    #[test]
    fn attribute_set_can_hold_complex_attributes() {
        let set = AttributeSet {
            attributes: vec![Attribute {
                name: "genome_size".to_string(),
                operator: Some(AttributeOperator::Lt),
                value: Some(AttributeValue::Single("3G".to_string())),
                modifier: vec![Modifier::Min],
            }],
            fields: vec![Field {
                name: "gc_percentage".to_string(),
                modifier: vec![Modifier::Max],
            }],
            names: vec!["scientific_name".to_string()],
            ranks: vec!["species".to_string()],
        };

        assert_eq!(set.attributes.len(), 1);
        assert_eq!(set.fields.len(), 1);
        assert_eq!(set.names.len(), 1);
        assert_eq!(set.ranks.len(), 1);
    }

    #[test]
    fn attribute_operator_all_variants_have_string_representation() {
        // Ensure all operators have a valid string representation
        let ops = vec![
            (AttributeOperator::Eq, "="),
            (AttributeOperator::Ne, "!="),
            (AttributeOperator::Lt, "<"),
            (AttributeOperator::Le, "<="),
            (AttributeOperator::Gt, ">"),
            (AttributeOperator::Ge, ">="),
            (AttributeOperator::Exists, ""),
            (AttributeOperator::Missing, ""),
        ];
        for (op, expected) in ops {
            assert_eq!(op.as_str(), expected);
        }
    }

    #[test]
    fn modifier_serde_roundtrip() {
        let yaml = "[min, max, ancestral, direct]";
        let modifiers: Vec<Modifier> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            modifiers,
            vec![
                Modifier::Min,
                Modifier::Max,
                Modifier::Ancestral,
                Modifier::Direct
            ]
        );
    }

    #[test]
    fn attribute_value_none_list_serialises() {
        let val = AttributeValue::List(vec![]);
        let json = serde_json::to_value(&val).unwrap();
        assert_eq!(json, serde_json::json!([]));
    }

    #[test]
    fn attribute_with_exists_operator() {
        let attr = Attribute {
            name: "assembly_level".to_string(),
            operator: Some(AttributeOperator::Exists),
            value: None,
            modifier: vec![],
        };
        assert!(attr.operator.is_some());
        assert_eq!(attr.operator.unwrap().as_str(), "");
    }

    #[test]
    fn attribute_set_serialises_to_json() {
        let set = AttributeSet {
            attributes: vec![Attribute {
                name: "test".to_string(),
                operator: Some(AttributeOperator::Eq),
                value: Some(AttributeValue::Single("value".to_string())),
                modifier: vec![],
            }],
            ..Default::default()
        };
        let json = serde_json::to_value(&set).unwrap();
        assert!(json.get("attributes").is_some());
    }

    #[test]
    fn field_with_multiple_modifiers() {
        let field = Field {
            name: "genome_size".to_string(),
            modifier: vec![Modifier::Min, Modifier::Max, Modifier::Direct],
        };
        assert_eq!(field.modifier.len(), 3);
    }

    #[test]
    fn attribute_operator_equality() {
        assert_eq!(AttributeOperator::Eq, AttributeOperator::Eq);
        assert_ne!(AttributeOperator::Eq, AttributeOperator::Ne);
        assert_ne!(AttributeOperator::Lt, AttributeOperator::Gt);
    }
}
