//! Attribute types for [`SearchQuery`](super::SearchQuery).
//!
//! Covers attribute filters, return fields, taxon name classes, and rank columns.

use serde::{Deserialize, Deserializer, Serialize};

// ── Operator aliases ──────────────────────────────────────────────────────────

/// Normalize operator aliases to canonical snake_case form.
///
/// Supports both symbolic operators (`>`, `>=`, etc.) and word forms
/// (`gt`, `gte`, `ge`, etc.) for backward compatibility.
fn normalize_operator(input: &str) -> String {
    match input.to_lowercase().as_str() {
        // Greater than
        ">" | "gt" => "gt".to_string(),
        ">=" | "gte" | "ge" => "ge".to_string(),
        // Less than
        "<" | "lt" => "lt".to_string(),
        "<=" | "lte" | "le" => "le".to_string(),
        // Equality
        "=" | "==" | "eq" => "eq".to_string(),
        "!=" | "ne" => "ne".to_string(),
        // Existence
        "exists" => "exists".to_string(),
        "missing" => "missing".to_string(),
        // Pass through unknown values for serde to reject
        other => other.to_string(),
    }
}

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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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

impl<'de> Deserialize<'de> for AttributeOperator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        let normalized = normalize_operator(&input);

        match normalized.as_str() {
            "eq" => Ok(AttributeOperator::Eq),
            "ne" => Ok(AttributeOperator::Ne),
            "lt" => Ok(AttributeOperator::Lt),
            "le" => Ok(AttributeOperator::Le),
            "gt" => Ok(AttributeOperator::Gt),
            "ge" => Ok(AttributeOperator::Ge),
            "exists" => Ok(AttributeOperator::Exists),
            "missing" => Ok(AttributeOperator::Missing),
            _ => Err(serde::de::Error::unknown_variant(
                &input,
                &[
                    "eq", "ne", "lt", "le", "gt", "ge", "exists", "missing", ">", ">=", "<", "<=",
                    "=", "==", "!=", "gte", "ge", "lte", "le",
                ],
            )),
        }
    }
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

    /// Return the lowercase modifier string for use in `fields=field:modifier` URL params.
    ///
    /// Unlike [`as_str`], this always returns lowercase, which is what the API
    /// expects in the `fields` query parameter (e.g. `assembly_span:direct`).
    pub fn as_field_param_str(&self) -> &'static str {
        match self {
            Modifier::Min => "min",
            Modifier::Max => "max",
            Modifier::Median => "median",
            Modifier::Mean => "mean",
            Modifier::Sum => "sum",
            Modifier::List => "list",
            Modifier::Length => "length",
            Modifier::Direct => "direct",
            Modifier::Ancestral => "ancestral",
            Modifier::Descendant => "descendant",
            Modifier::Estimated => "estimated",
            Modifier::Missing => "missing",
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

    #[test]
    fn all_modifiers_convert_to_string() {
        // Test all modifier variants and their string representations
        assert_eq!(Modifier::Min.as_str(), "min");
        assert_eq!(Modifier::Max.as_str(), "max");
        assert_eq!(Modifier::Median.as_str(), "median");
        assert_eq!(Modifier::Mean.as_str(), "mean");
        assert_eq!(Modifier::Sum.as_str(), "sum");
        assert_eq!(Modifier::List.as_str(), "list");
        assert_eq!(Modifier::Length.as_str(), "length");
        assert_eq!(Modifier::Direct.as_str(), "Direct");
        assert_eq!(Modifier::Ancestral.as_str(), "Ancestral");
        assert_eq!(Modifier::Descendant.as_str(), "Descendant");
        assert_eq!(Modifier::Estimated.as_str(), "Estimated");
        assert_eq!(Modifier::Missing.as_str(), "Missing");
    }

    #[test]
    fn modifier_classification_covers_all_status_types() {
        // Summary modifiers
        assert!(Modifier::Min.is_summary());
        assert!(Modifier::Max.is_summary());
        assert!(Modifier::Median.is_summary());
        assert!(Modifier::Mean.is_summary());
        assert!(Modifier::Sum.is_summary());
        assert!(Modifier::List.is_summary());
        assert!(Modifier::Length.is_summary());

        // Status modifiers
        assert!(Modifier::Direct.is_status());
        assert!(Modifier::Ancestral.is_status());
        assert!(Modifier::Descendant.is_status());
        assert!(Modifier::Estimated.is_status());
        assert!(Modifier::Missing.is_status());

        // Cross-checks
        assert!(!Modifier::Min.is_status());
        assert!(!Modifier::Direct.is_summary());
    }

    #[test]
    fn operator_alias_deserialises_symbol_greater_than() {
        let op: AttributeOperator = serde_json::from_str("\">\"\n").unwrap();
        assert_eq!(op, AttributeOperator::Gt);
    }

    #[test]
    fn operator_alias_deserialises_symbol_greater_equal() {
        let op: AttributeOperator = serde_json::from_str("\">=\"").unwrap();
        assert_eq!(op, AttributeOperator::Ge);
    }

    #[test]
    fn operator_alias_deserialises_word_gte() {
        let json = r#""gte""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Ge);
    }

    #[test]
    fn operator_alias_deserialises_word_ge() {
        let json = r#""ge""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Ge);
    }

    #[test]
    fn operator_alias_deserialises_symbol_less_than() {
        let op: AttributeOperator = serde_json::from_str("\"<\"").unwrap();
        assert_eq!(op, AttributeOperator::Lt);
    }

    #[test]
    fn operator_alias_deserialises_symbol_less_equal() {
        let op: AttributeOperator = serde_json::from_str("\"<=\"").unwrap();
        assert_eq!(op, AttributeOperator::Le);
    }

    #[test]
    fn operator_alias_deserialises_word_lte() {
        let json = r#""lte""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Le);
    }

    #[test]
    fn operator_alias_deserialises_word_le() {
        let json = r#""le""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Le);
    }

    #[test]
    fn operator_alias_deserialises_symbol_equals() {
        let op: AttributeOperator = serde_json::from_str("\"=\"").unwrap();
        assert_eq!(op, AttributeOperator::Eq);
    }

    #[test]
    fn operator_alias_deserialises_symbol_double_equals() {
        let op: AttributeOperator = serde_json::from_str("\"==\"").unwrap();
        assert_eq!(op, AttributeOperator::Eq);
    }

    #[test]
    fn operator_alias_deserialises_word_eq() {
        let json = r#""eq""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Eq);
    }

    #[test]
    fn operator_alias_deserialises_symbol_not_equal() {
        let op: AttributeOperator = serde_json::from_str("\"!=\"").unwrap();
        assert_eq!(op, AttributeOperator::Ne);
    }

    #[test]
    fn operator_alias_deserialises_word_ne() {
        let json = r#""ne""#;
        let op: AttributeOperator = serde_json::from_str(json).unwrap();
        assert_eq!(op, AttributeOperator::Ne);
    }

    #[test]
    fn operator_canonical_forms_still_work() {
        // Ensure we didn't break the existing snake_case forms
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""lt""#).unwrap(),
            AttributeOperator::Lt
        );
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""gt""#).unwrap(),
            AttributeOperator::Gt
        );
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""exists""#).unwrap(),
            AttributeOperator::Exists
        );
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""missing""#).unwrap(),
            AttributeOperator::Missing
        );
    }

    #[test]
    fn operator_alias_case_insensitive() {
        // Ensure aliases are case-insensitive
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""GT""#).unwrap(),
            AttributeOperator::Gt
        );
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""GTE""#).unwrap(),
            AttributeOperator::Ge
        );
        assert_eq!(
            serde_json::from_str::<AttributeOperator>(r#""Lt""#).unwrap(),
            AttributeOperator::Lt
        );
    }

    #[test]
    fn operator_invalid_alias_fails() {
        let result = serde_json::from_str::<AttributeOperator>(r#""invalid_op""#);
        assert!(result.is_err());
    }
}
