use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ToolResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ToolResponse<T> {
    pub(crate) fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub(crate) fn error(error: impl ToString) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct EmptyInput {}

#[cfg(test)]
mod tests {
    use workspace_api::{BoardPropertyValueDetail, CreateEntryInput, ProjectSummary};

    use super::*;

    #[test]
    fn omitted_wire_defaults_are_applied_to_the_shared_command() {
        let input: CreateEntryInput = serde_json::from_value(serde_json::json!({
            "list_id": 7,
            "title": "Ship"
        }))
        .expect("MCP create entry input should deserialize");

        assert_eq!(input.list_id, 7);
        assert_eq!(input.title, "Ship");
        assert_eq!(input.description, "");
        assert_eq!(input.due_on, None);
    }

    #[test]
    fn tagged_property_values_preserve_the_mcp_json_contract() {
        let value = BoardPropertyValueDetail::Select(42);
        assert_eq!(
            serde_json::to_value(value).expect("property value should serialize"),
            serde_json::json!({ "kind": "select", "value": 42 })
        );
    }

    #[test]
    fn shared_results_are_serialized_in_the_wire_envelope() {
        let detail = ProjectSummary {
            id: 9,
            name: "Delivery".to_string(),
            position: 2,
            board_count: 3,
        };
        let response = ToolResponse::success(detail);

        assert_eq!(
            serde_json::to_value(response).expect("tool response should serialize"),
            serde_json::json!({
                "success": true,
                "data": {
                    "id": 9,
                    "name": "Delivery",
                    "position": 2,
                    "board_count": 3
                },
                "error": null
            })
        );
    }
}
