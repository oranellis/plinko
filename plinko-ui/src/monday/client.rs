//! Monday.com HTTP client — thin wrapper around the Monday.com v2 GraphQL API.

use serde_json::{Value, json};

use chrono::NaiveDate;
use plinko_shared::monday::{BoardColumn, MondayItem, MondayUser};

const API_URL: &str = "https://api.monday.com/v2";

/// Error type for Monday API calls.
#[derive(Debug, Clone)]
pub struct MondayApiError(pub String);

impl std::fmt::Display for MondayApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Blocking Monday.com API client.
pub struct MondayClient {
    token: String,
    client: reqwest::blocking::Client,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl MondayClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: reqwest::blocking::Client::new(),
        }
    }

    fn graphql(&self, query: &str) -> Result<Value, MondayApiError> {
        let body = json!({ "query": query });
        let resp = self
            .client
            .post(API_URL)
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .header("API-Version", "2024-01")
            .json(&body)
            .send()
            .map_err(|e| MondayApiError(format!("HTTP error: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| MondayApiError(format!("read error: {e}")))?;

        if !status.is_success() {
            return Err(MondayApiError(format!("HTTP {status}: {text}")));
        }

        let value: Value = serde_json::from_str(&text)
            .map_err(|e| MondayApiError(format!("JSON parse error: {e}")))?;

        if let Some(errors) = value.get("errors") {
            return Err(MondayApiError(format!("API errors: {errors}")));
        }

        Ok(value)
    }

    /// Test the connection by fetching the authenticated user's name.
    pub fn test_connection(&self) -> Result<String, MondayApiError> {
        let resp = self.graphql("query { me { name } }")?;
        let name = resp["data"]["me"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(name)
    }

    /// Fetch all columns on a board.
    pub fn fetch_columns(&self, board_id: &str) -> Result<Vec<BoardColumn>, MondayApiError> {
        let query = format!(
            r#"query {{
                boards(ids: [{board_id}]) {{
                    columns {{ id title type }}
                }}
            }}"#
        );
        let resp = self.graphql(&query)?;
        let cols = resp["data"]["boards"][0]["columns"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(cols
            .iter()
            .map(|c| BoardColumn {
                id: c["id"].as_str().unwrap_or("").to_string(),
                title: c["title"].as_str().unwrap_or("").to_string(),
                column_type: c["type"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    /// Fetch workspace users (all members visible to this token).
    pub fn fetch_users(&self) -> Result<Vec<MondayUser>, MondayApiError> {
        let query = r#"query { users { id name email } }"#;
        let resp = self.graphql(query)?;
        let users = resp["data"]["users"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(users
            .iter()
            .map(|u| MondayUser {
                id: u["id"].as_str().unwrap_or("").to_string(),
                name: u["name"].as_str().unwrap_or("").to_string(),
                email: u["email"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    /// Fetch distinct status labels used on a board's status column.
    pub fn fetch_status_labels(
        &self,
        board_id: &str,
        status_column_id: &str,
    ) -> Result<Vec<String>, MondayApiError> {
        let query = format!(
            r#"query {{
                boards(ids: [{board_id}]) {{
                    columns(ids: ["{status_column_id}"]) {{
                        settings_str
                    }}
                }}
            }}"#
        );
        let resp = self.graphql(&query)?;
        let settings_str = resp["data"]["boards"][0]["columns"][0]["settings_str"]
            .as_str()
            .unwrap_or("{}");
        let settings: Value = serde_json::from_str(settings_str).unwrap_or(Value::Null);
        let labels: Vec<String> = settings["labels"]
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        Ok(labels)
    }

    /// Fetch the board ID of the subitems board for a given parent board.
    ///
    /// Returns an empty string if the board has no items or no subitems.
    pub fn fetch_subitem_board_id(&self, board_id: &str) -> Result<String, MondayApiError> {
        let query = format!(
            r#"query {{
                boards(ids: [{board_id}]) {{
                    items_page(limit: 1) {{
                        items {{
                            subitems {{ board {{ id }} }}
                        }}
                    }}
                }}
            }}"#
        );
        let resp = self.graphql(&query)?;
        let id = resp["data"]["boards"][0]["items_page"]["items"][0]["subitems"][0]["board"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(id)
    }

    /// Fetch all items and subitems from a board.
    ///
    /// Items whose `timeline_col` value has `visualization_type: "milestone"` are
    /// flagged as milestones; all others are treated as tasks.
    ///
    /// Subitems are fetched with **all** column values (no ID filter) because
    /// subitems live on their own sub-board which may have different column IDs
    /// to the parent board.  `parse_item` matches by ID regardless.
    pub fn fetch_items(
        &self,
        board_id: &str,
        person_col: &str,
        status_col: &str,
        dep_col: &str,
        workload_col: &str,
        timeline_col: &str,
    ) -> Result<Vec<MondayItem>, MondayApiError> {
        let col_ids =
            build_col_ids_list(&[person_col, status_col, dep_col, workload_col, timeline_col]);

        // Dependency columns require the DependencyValue inline fragment to get linked item IDs.
        // The `value` field is always null for dependency-type columns in Monday API v2.
        let dep_fragment = "... on DependencyValue { linked_items { id } }";

        // Subitems: no `ids` filter so we get every column value regardless of
        // whether the subitem board uses the same column IDs as the parent board.
        let subitems_block = format!(
            r#"subitems {{
                    id name
                    column_values {{
                        id type value text
                        {dep_fragment}
                    }}
                }}"#
        );

        let query = format!(
            r#"query {{
                boards(ids: [{board_id}]) {{
                    items_page(limit: 500) {{
                        items {{
                            id name
                            column_values(ids: [{col_ids}]) {{
                                id type value text
                                {dep_fragment}
                            }}
                            {subitems_block}
                        }}
                    }}
                }}
            }}"#
        );
        let resp = self.graphql(&query)?;
        let raw_items = resp["data"]["boards"][0]["items_page"]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut result = Vec::new();
        for raw in &raw_items {
            let item_id = raw["id"].as_str().unwrap_or("").to_string();
            let name = raw["name"].as_str().unwrap_or("").to_string();
            let cv = raw["column_values"].as_array().cloned().unwrap_or_default();
            let item = parse_item(
                item_id.clone(),
                name,
                None,
                &cv,
                person_col,
                status_col,
                dep_col,
                workload_col,
                timeline_col,
            );
            result.push(item);

            if let Some(subs) = raw["subitems"].as_array() {
                for sub in subs {
                    let sub_id = sub["id"].as_str().unwrap_or("").to_string();
                    let sub_name = sub["name"].as_str().unwrap_or("").to_string();
                    let sub_cv = sub["column_values"].as_array().cloned().unwrap_or_default();
                    let sub_item = parse_item(
                        sub_id,
                        sub_name,
                        Some(item_id.clone()),
                        &sub_cv,
                        person_col,
                        status_col,
                        dep_col,
                        workload_col,
                        timeline_col,
                    );
                    result.push(sub_item);
                }
            }
        }

        Ok(result)
    }

    /// Update a timeline column on an item.
    pub fn update_timeline(
        &self,
        board_id: &str,
        item_id: &str,
        timeline_col: &str,
        from_date: &str,
        to_date: &str,
    ) -> Result<(), MondayApiError> {
        let value = format!(r#"{{\"from\":\"{from_date}\",\"to\":\"{to_date}\"}}"#);
        let query = format!(
            r#"mutation {{
                change_column_value(
                    board_id: {board_id},
                    item_id: {item_id},
                    column_id: "{timeline_col}",
                    value: "{value}"
                ) {{ id }}
            }}"#
        );
        self.graphql(&query)?;
        Ok(())
    }
}
// }}}

fn build_col_ids_list(ids: &[&str]) -> String {
    ids.iter()
        .filter(|s| !s.is_empty())
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_item(
    id: String,
    name: String,
    parent_id: Option<String>,
    col_values: &[Value],
    person_col: &str,
    status_col: &str,
    dep_col: &str,
    workload_col: &str,
    timeline_col: &str,
) -> MondayItem {
    let mut assigned_user_ids = Vec::new();
    let mut status_label = None;
    let mut dependency_item_ids = Vec::new();
    let mut workload = None;
    let mut is_milestone = false;
    let mut timeline_start: Option<NaiveDate> = None;
    let mut timeline_end: Option<NaiveDate> = None;

    for cv in col_values {
        let col_id = cv["id"].as_str().unwrap_or("");
        if col_id == person_col {
            if let Ok(v) = serde_json::from_str::<Value>(cv["value"].as_str().unwrap_or("null")) {
                if let Some(persons) = v["personsAndTeams"].as_array() {
                    for p in persons {
                        // Only map actual persons, not teams.
                        if p["kind"].as_str() != Some("person") {
                            continue;
                        }
                        if let Some(uid) = p["id"].as_u64() {
                            assigned_user_ids.push(uid.to_string());
                        }
                    }
                }
            }
        } else if col_id == status_col {
            let label = cv["text"].as_str().unwrap_or("").to_string();
            if !label.is_empty() {
                status_label = Some(label);
            }
        } else if col_id == dep_col {
            // Monday API v2: dependency columns return null for `value`.
            // Use the DependencyValue inline fragment `linked_items` field instead.
            if let Some(linked) = cv["linked_items"].as_array() {
                for link in linked {
                    if let Some(item_id) = link["id"].as_str() {
                        dependency_item_ids.push(item_id.to_string());
                    }
                }
            }
        } else if col_id == workload_col {
            if let Some(text) = cv["text"].as_str() {
                workload = text.parse::<f32>().ok();
            }
        } else if col_id == timeline_col && !timeline_col.is_empty() {
            // Parse from/to dates and milestone flag from the timeline column value.
            if let Ok(v) = serde_json::from_str::<Value>(cv["value"].as_str().unwrap_or("null")) {
                if v["visualization_type"].as_str() == Some("milestone") {
                    is_milestone = true;
                }
                if let Some(from_str) = v["from"].as_str() {
                    timeline_start = NaiveDate::parse_from_str(from_str, "%Y-%m-%d").ok();
                }
                if let Some(to_str) = v["to"].as_str() {
                    timeline_end = NaiveDate::parse_from_str(to_str, "%Y-%m-%d").ok();
                }
            }
        }
    }

    MondayItem {
        id,
        name,
        parent_id,
        assigned_user_ids,
        status_label,
        dependency_item_ids,
        workload,
        is_milestone,
        timeline_start,
        timeline_end,
    }
}
