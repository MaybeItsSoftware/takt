//! The bodies a sync cycle sends and receives (docs/sync.md, "Wire
//! protocol"): `POST /v1/push` and `GET /v1/changes`. The server decodes a
//! push and encodes a page with these; the Rust core, which every client calls,
//! encodes a push and decodes a page with the same structs, so the two ends
//! cannot drift apart.
//!
//! A value is a JSON `null`, number or string, as SQLite stores it. The server
//! keeps it as JSON (`serde_json::Value`) and never looks inside; the core
//! turns it into a column value.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `POST /v1/push`'s body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushRequest {
    pub changes: Vec<WireChange>,
}

/// One row's change in a push.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireChange {
    pub table: String,
    pub id: String,
    pub op: WireOp,
    pub hlc: String,
    /// The columns of an upsert. Left out of a delete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Map<String, Value>>,
    /// The shared space the row belongs to (docs/shared-lists-plan.md), or
    /// nothing for the account's own. A client that predates sharing never
    /// sends it, and every row it pushes is its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WireOp {
    Upsert,
    Delete,
}

/// `POST /v1/push`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResponse {
    pub accepted: usize,
    pub cursor: i64,
}

/// `GET /v1/changes`'s answer: one page of the account's feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesResponse {
    pub rows: Vec<ChangedRow>,
    pub cursor: i64,
    pub has_more: bool,
    /// Every shared space the account is in, as of this page. Absent from a
    /// server that predates sharing, which a client reads as "no change"
    /// rather than "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spaces: Option<Vec<WireSpace>>,
}

/// A shared list, as its members see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSpace {
    pub id: String,
    /// The shared list's row id.
    pub list_id: String,
    /// The list's name when it was shared.
    #[serde(default)]
    pub name: String,
    /// The caller's role: "owner" or "member".
    pub role: String,
    #[serde(default)]
    pub members: Vec<WireMember>,
}

/// One of a shared list's members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMember {
    pub account_id: String,
    #[serde(default)]
    pub email: Option<String>,
    /// "owner" or "member".
    pub role: String,
    /// The account asking.
    #[serde(default)]
    pub you: bool,
}

/// A row as the server holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedRow {
    pub table: String,
    pub id: String,
    pub deleted: bool,
    #[serde(default)]
    pub values: Map<String, Value>,
    /// The newest of the row's column stamps and its delete stamp.
    #[serde(default)]
    pub hlc: Option<String>,
    /// The shared space the row is in, or nothing for the account's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
}

/// Every error the server sends: `{"error": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_delete_leaves_its_values_out_and_an_upsert_keeps_them() {
        let request = PushRequest {
            changes: vec![
                WireChange {
                    table: "tasks".into(),
                    id: "a".into(),
                    op: WireOp::Upsert,
                    hlc: "0000000000001-0000-d".into(),
                    values: Some(Map::from_iter([("title".into(), json!("Buy milk"))])),
                    space: None,
                },
                WireChange {
                    table: "tasks".into(),
                    id: "b".into(),
                    op: WireOp::Delete,
                    hlc: "0000000000002-0000-d".into(),
                    values: None,
                    space: None,
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "changes": [
                { "table": "tasks", "id": "a", "op": "upsert", "hlc": "0000000000001-0000-d",
                  "values": { "title": "Buy milk" } },
                { "table": "tasks", "id": "b", "op": "delete", "hlc": "0000000000002-0000-d" },
            ] })
        );
        let back: PushRequest =
            serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
        assert_eq!(back, request);
    }

    #[test]
    fn a_page_is_camel_case_and_reads_back() {
        let page: ChangesResponse = serde_json::from_str(
            r#"{"rows":[{"table":"tasks","id":"a","deleted":true,"values":{},"hlc":null}],"cursor":7,"hasMore":true}"#,
        )
        .unwrap();
        assert!(page.has_more);
        assert_eq!(page.cursor, 7);
        assert_eq!(page.rows[0].hlc, None);
        assert_eq!(
            serde_json::to_value(&page).unwrap()["hasMore"],
            json!(true),
            "the server writes hasMore"
        );
        // A row without values or a stamp still reads.
        let bare: ChangedRow =
            serde_json::from_str(r#"{"table":"t","id":"i","deleted":false}"#).unwrap();
        assert!(bare.values.is_empty());
    }
}
