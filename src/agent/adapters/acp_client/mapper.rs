use serde::Deserialize;
use serde_json::Value;

use crate::agent::tools::{ContentPart, ToolResult};

#[derive(Debug, Clone, PartialEq)]
pub struct AcpAgentMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AcpRunStatus {
    Pending,
    Complete(ToolResult),
}

pub(super) fn parse_agent_metadata(value: &Value) -> Result<AcpAgentMetadata, String> {
    let wire: AgentMetadataWire =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if wire.id.trim().is_empty() {
        return Err("agent id must not be empty".into());
    }
    let name = wire
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| wire.id.clone());
    Ok(AcpAgentMetadata {
        id: wire.id,
        name,
        description: wire.description.unwrap_or_default(),
        input_schema: wire
            .input_schema
            .unwrap_or_else(|| serde_json::json!({"type": "object"})),
    })
}

pub(super) fn parse_run_status(value: &Value) -> Result<AcpRunStatus, String> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "run status must be a string".to_string())?;

    match status {
        "queued" | "running" | "pending" | "in_progress" => Ok(AcpRunStatus::Pending),
        "completed" | "succeeded" => {
            let output_value = value
                .get("output")
                .or_else(|| value.get("result"))
                .ok_or_else(|| format!("run with status '{status}' must contain output"))?;
            parse_tool_result(output_value, false).map(AcpRunStatus::Complete)
        }
        "failed" | "cancelled" | "canceled" => {
            if let Some(output_value) = value.get("output").or_else(|| value.get("result")) {
                parse_tool_result(output_value, true).map(AcpRunStatus::Complete)
            } else {
                Ok(AcpRunStatus::Complete(ToolResult::failure(format!(
                    "ACP run ended with status '{status}'"
                ))))
            }
        }
        other => Err(format!("unknown ACP run status '{other}'")),
    }
}

fn parse_tool_result(value: &Value, default_is_error: bool) -> Result<ToolResult, String> {
    let is_error = value
        .get("is_error")
        .or_else(|| value.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(default_is_error)
        || default_is_error;
    let output_truncated = value
        .get("output_truncated")
        .or_else(|| value.get("outputTruncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exit_code = value
        .get("exit_code")
        .or_else(|| value.get("exitCode"))
        .and_then(Value::as_i64)
        .map(|code| i32::try_from(code).map_err(|_| "exit code must fit within i32".to_string()))
        .transpose()?;

    let content = if let Some(content) = value.get("content") {
        parse_content_parts(content)?
    } else if let Some(text) = value.get("text").and_then(Value::as_str) {
        vec![ContentPart::text(text)]
    } else {
        return Err("tool output must contain content or text".into());
    };

    Ok(ToolResult {
        content,
        exit_code,
        is_error,
        output_truncated,
    })
}

fn parse_content_parts(value: &Value) -> Result<Vec<ContentPart>, String> {
    let Some(entries) = value.as_array() else {
        return Err("content must be an array".into());
    };

    let mut parts = Vec::with_capacity(entries.len());
    for entry in entries {
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("text");
        match kind {
            "text" => {
                let text = entry
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "text content entry must contain text".to_string())?;
                parts.push(ContentPart::text(text));
            }
            "resource" => {
                let uri = entry
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "resource content entry must contain uri".to_string())?;
                let mime_type = entry
                    .get("mime_type")
                    .or_else(|| entry.get("mimeType"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                parts.push(ContentPart::Resource {
                    uri: uri.to_string(),
                    mime_type,
                });
            }
            other => return Err(format!("unsupported ACP content part type: {other}")),
        }
    }

    Ok(parts)
}

#[derive(Debug, Deserialize)]
struct AgentMetadataWire {
    id: String,
    name: Option<String>,
    description: Option<String>,
    #[serde(default, alias = "inputSchema")]
    input_schema: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_agent_metadata_falls_back_to_id_for_blank_name() {
        let metadata = parse_agent_metadata(&json!({
            "id": "reviewer",
            "name": "   ",
            "description": "Reviews code"
        }))
        .expect("metadata should parse");

        assert_eq!(metadata.name, "reviewer");
    }

    #[test]
    fn parse_run_status_rejects_exit_code_overflow() {
        let error = parse_run_status(&json!({
            "status": "completed",
            "output": {
                "content": [{ "type": "text", "text": "done" }],
                "exit_code": i64::from(i32::MAX) + 1
            }
        }))
        .expect_err("overflowing exit code should fail");

        assert!(error.contains("exit code must fit within i32"));
    }

    #[test]
    fn parse_agent_metadata_rejects_empty_id() {
        // Line 24: empty id returns Err.
        let err = parse_agent_metadata(&json!({
            "id": "   ",
            "name": "agent"
        }))
        .expect_err("empty id should fail");
        assert!(err.contains("agent id must not be empty"));
    }

    #[test]
    fn parse_run_status_failed_without_output_generates_message() {
        // Lines 60-61: failed/cancelled status with no output field.
        let status = parse_run_status(&json!({
            "status": "cancelled"
        }))
        .expect("cancelled without output should parse");
        let AcpRunStatus::Complete(result) = status else {
            panic!("should be complete");
        };
        assert!(result.is_error);
        assert!(result.text_output().contains("cancelled"));
    }

    #[test]
    fn parse_tool_result_missing_content_and_text_errors() {
        // Line 93: output without content or text.
        let err = parse_run_status(&json!({
            "status": "completed",
            "output": { "exit_code": 0 }
        }))
        .expect_err("missing content/text should fail");
        assert!(err.contains("must contain content or text"));
    }

    #[test]
    fn parse_content_parts_rejects_non_array_content() {
        // Line 106: content is not an array.
        let err = parse_run_status(&json!({
            "status": "completed",
            "output": { "content": "not-an-array" }
        }))
        .expect_err("non-array content should fail");
        assert!(err.contains("content must be an array"));
    }

    #[test]
    fn parse_content_parts_rejects_unsupported_type() {
        // Line 135: unsupported content part type.
        let err = parse_run_status(&json!({
            "status": "completed",
            "output": {
                "content": [{ "type": "image", "url": "http://example.com" }]
            }
        }))
        .expect_err("unsupported type should fail");
        assert!(err.contains("unsupported ACP content part type: image"));
    }

    #[test]
    fn parse_run_status_rejects_unknown_status_values() {
        let error = parse_run_status(&json!({
            "status": "mystery-state"
        }))
        .expect_err("unknown statuses should be rejected explicitly");

        assert!(error.contains("mystery-state"));
    }

    #[test]
    fn parse_run_status_marks_failed_output_as_error_when_field_missing() {
        let status = parse_run_status(&json!({
            "status": "failed",
            "output": {
                "content": [{ "type": "text", "text": "validation failed" }]
            }
        }))
        .expect("failed payload should parse");

        let AcpRunStatus::Complete(result) = status else {
            panic!("failed runs should be terminal");
        };
        assert!(result.is_error);
        assert_eq!(result.text_output(), "validation failed");
    }

    #[test]
    fn parse_run_status_keeps_cancelled_output_marked_as_error_when_false() {
        let status = parse_run_status(&json!({
            "status": "cancelled",
            "output": {
                "content": [{ "type": "text", "text": "cancelled by user" }],
                "is_error": false
            }
        }))
        .expect("cancelled payload should parse");

        let AcpRunStatus::Complete(result) = status else {
            panic!("cancelled runs should be terminal");
        };
        assert!(result.is_error);
        assert_eq!(result.text_output(), "cancelled by user");
    }
}
