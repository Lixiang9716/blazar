use httpmock::prelude::*;
use serde_json::json;

use super::*;
use crate::agent::acp_discovery::AcpAgentMetadata;

fn sample_metadata() -> AcpAgentMetadata {
    AcpAgentMetadata {
        id: "reviewer".into(),
        name: "ACP reviewer".into(),
        description: "Reviews changes".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string" }
            },
            "required": ["prompt"],
            "additionalProperties": false
        }),
    }
}

#[test]
fn acp_tool_executes_run_to_completion() {
    let server = MockServer::start();

    let create_run = server.mock(|when, then| {
        when.method(POST).path("/runs").json_body(json!({
            "agent_id": "reviewer",
            "input": {
                "prompt": "review the patch"
            }
        }));
        then.status(200).json_body(json!({
            "id": "run-1"
        }));
    });
    let get_run = server.mock(|when, then| {
        when.method(GET).path("/runs/run-1");
        then.status(200).json_body(json!({
            "id": "run-1",
            "status": "completed",
            "output": {
                "content": [
                    { "type": "text", "text": "review complete" }
                ],
                "is_error": false
            }
        }));
    });

    let tool = AcpAgentTool::new("configured_reviewer", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    let result = tool.execute(json!({
        "prompt": "review the patch"
    }));

    assert!(!result.is_error);
    assert_eq!(result.text_output(), "review complete");
    create_run.assert();
    get_run.assert();
}

#[test]
fn acp_tool_surfaces_failed_run_output() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({
            "id": "run-2"
        }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/runs/run-2");
        then.status(200).json_body(json!({
            "id": "run-2",
            "status": "failed",
            "output": {
                "content": [
                    { "type": "text", "text": "validation failed" }
                ],
                "is_error": true
            }
        }));
    });

    let tool = AcpAgentTool::new("configured_reviewer", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    let result = tool.execute(json!({
        "prompt": "review the patch"
    }));

    assert!(result.is_error);
    assert_eq!(result.text_output(), "validation failed");
}
