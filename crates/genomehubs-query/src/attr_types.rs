//! Attribute type metadata shared by the query builder and API.
//!
//! These are pure data types. Fetching the metadata from Elasticsearch lives
//! in the API crate, so this module stays free of HTTP dependencies.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Lightweight representation of an attribute metadata document from ES `_search` `_source`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMeta {
    pub group: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub summary: Option<Value>,
    #[serde(default)]
    pub default_summary: Option<String>,
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub synonyms: Vec<String>,
    #[serde(default)]
    pub processed_type: Option<String>,
    #[serde(default)]
    pub processed_summary: Option<String>,
    #[serde(default)]
    pub processed_simple: Option<String>,
}

/// Attribute metadata keyed by group, then by attribute name.
pub type TypesMap = HashMap<String, HashMap<String, TypeMeta>>;

/// Synonym lookup keyed by group, then by synonym, yielding the canonical attribute name.
pub type SynonymsMap = HashMap<String, HashMap<String, String>>;
