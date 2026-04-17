//! Response parsers for the genomehubs search API.
//!
//! Each parser accepts a raw JSON string, extracts the relevant fields, and
//! returns a well-typed struct or a JSON string suitable for FFI boundaries.
//!
//! All functions are pure — no I/O, no panics.  Error cases return a
//! descriptive string rather than propagating through `anyhow` or `thiserror`
//! so that both WASM (`wasm_bindgen`) and PyO3 callers get a plain string they
//! can surface directly to users.

use serde::Deserialize;

// ── ResponseStatus ────────────────────────────────────────────────────────────

/// The `status` block present in every genomehubs search/count API response.
///
/// ```json
/// { "status": { "hits": 42, "ok": true, "error": null } }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseStatus {
    /// Total number of records matching the query.
    pub hits: u64,
    /// Whether the API reported success.
    pub ok: bool,
    /// Error message returned by the API, if any.
    pub error: Option<String>,
}

/// Minimal serde view of just the `status` block we need.
#[derive(Deserialize)]
struct ApiStatus {
    hits: Option<serde_json::Value>,
    ok: Option<bool>,
    error: Option<serde_json::Value>,
}

/// Minimal serde view of the outer response envelope.
#[derive(Deserialize)]
struct ApiResponse {
    status: Option<ApiStatus>,
}

/// Parse the `status` block from a raw genomehubs API JSON response.
///
/// The `hits` field accepts both integer and string encodings (the API
/// occasionally returns `"42"` rather than `42`).
///
/// Returns `Ok(ResponseStatus)` on success.  The only failure case is
/// completely unparseable JSON — a missing or null `status` block is treated
/// as `{ hits: 0, ok: false, error: Some("missing status block") }` rather
/// than an error, because partial/error responses still contain useful context.
///
/// # Example
/// ```
/// use genomehubs_query::parse::parse_response_status;
///
/// let json = r#"{"status":{"hits":42,"ok":true}}"#;
/// let s = parse_response_status(json).unwrap();
/// assert_eq!(s.hits, 42);
/// assert!(s.ok);
/// assert!(s.error.is_none());
/// ```
pub fn parse_response_status(raw: &str) -> Result<ResponseStatus, String> {
    let envelope: ApiResponse =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let status = match envelope.status {
        Some(s) => s,
        None => {
            return Ok(ResponseStatus {
                hits: 0,
                ok: false,
                error: Some("missing status block in API response".to_string()),
            });
        }
    };

    let hits = parse_hits(status.hits.as_ref());
    let ok = status.ok.unwrap_or(false);
    let error = status.error.and_then(|v| match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    });

    Ok(ResponseStatus { hits, ok, error })
}

/// Coerce `hits` from either a JSON number or a JSON string to `u64`.
fn parse_hits(value: Option<&serde_json::Value>) -> u64 {
    match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Serialise a [`ResponseStatus`] to a compact JSON string for FFI boundaries.
///
/// Returns `{"hits":N,"ok":true|false,"error":null|"msg"}`.
pub fn response_status_to_json(status: &ResponseStatus) -> String {
    match &status.error {
        None => format!(
            r#"{{"hits":{},"ok":{},"error":null}}"#,
            status.hits, status.ok
        ),
        Some(msg) => {
            let escaped = msg.replace('\\', r"\\").replace('"', r#"\""#);
            format!(
                r#"{{"hits":{},"ok":{},"error":"{}"}}"#,
                status.hits, status.ok, escaped
            )
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer_hits() {
        let json = r#"{"status":{"hits":42,"ok":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 42);
        assert!(s.ok);
        assert!(s.error.is_none());
    }

    #[test]
    fn parses_string_hits() {
        let json = r#"{"status":{"hits":"123","ok":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 123);
    }

    #[test]
    fn zero_hits_on_null_hits() {
        let json = r#"{"status":{"hits":null,"ok":true}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
    }

    #[test]
    fn missing_status_block() {
        let json = r#"{"results":[]}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
        assert!(!s.ok);
        assert!(s.error.is_some());
    }

    #[test]
    fn captures_api_error() {
        let json = r#"{"status":{"hits":0,"ok":false,"error":"query parse error"}}"#;
        let s = parse_response_status(json).unwrap();
        assert_eq!(s.hits, 0);
        assert!(!s.ok);
        assert_eq!(s.error.as_deref(), Some("query parse error"));
    }

    #[test]
    fn invalid_json_returns_err() {
        assert!(parse_response_status("not json").is_err());
    }

    #[test]
    fn to_json_round_trips() {
        let status = ResponseStatus {
            hits: 5,
            ok: true,
            error: None,
        };
        let json = response_status_to_json(&status);
        // response_status_to_json produces the inner status object (for FFI).
        // Verify the serialised form has the correct fields.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["hits"], 5);
        assert_eq!(v["ok"], true);
        assert!(v["error"].is_null());
    }

    #[test]
    fn to_json_round_trips_with_error() {
        let status = ResponseStatus {
            hits: 0,
            ok: false,
            error: Some("bad request".to_string()),
        };
        let json = response_status_to_json(&status);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["error"], "bad request");
    }
}
