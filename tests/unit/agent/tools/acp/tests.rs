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
fn acp_tool_kind_returns_agent_acp() {
    let server = MockServer::start();
    let tool = AcpAgentTool::new("test_tool", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    assert_eq!(tool.kind(), ToolKind::Agent { is_acp: true });
}

#[test]
fn acp_tool_returns_failure_when_create_run_fails() {
    let server = MockServer::start();

    // create_run returns a server error
    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(500);
    });

    let tool = AcpAgentTool::new("configured_reviewer", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    let result = tool.execute(json!({ "prompt": "hello" }));

    assert!(result.is_error);
    assert!(
        result.text_output().contains("agent unreachable"),
        "expected 'agent unreachable' in: {}",
        result.text_output()
    );
}

#[test]
fn acp_tool_returns_failure_when_poll_run_errors() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({ "id": "run-err" }));
    });
    // get_run returns a server error so poll_run_to_completion fails
    server.mock(|when, then| {
        when.method(GET).path("/runs/run-err");
        then.status(500);
    });

    let tool = AcpAgentTool::new("configured_reviewer", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    let result = tool.execute(json!({ "prompt": "hello" }));

    assert!(result.is_error);
    assert!(
        result.text_output().contains("ACP run failed"),
        "expected 'ACP run failed' in: {}",
        result.text_output()
    );
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

#[test]
fn acp_tool_marks_failed_run_without_error_flag_as_error() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({
            "id": "run-3"
        }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/runs/run-3");
        then.status(200).json_body(json!({
            "id": "run-3",
            "status": "failed",
            "output": {
                "content": [
                    { "type": "text", "text": "validation failed" }
                ]
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

#[test]
fn acp_tool_summarizes_resource_only_output() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({
            "id": "run-4"
        }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/runs/run-4");
        then.status(200).json_body(json!({
            "id": "run-4",
            "status": "completed",
            "output": {
                "content": [
                    {
                        "type": "resource",
                        "uri": "file://workspace/report.json",
                        "mime_type": "application/json"
                    }
                ]
            }
        }));
    });

    let tool = AcpAgentTool::new("configured_reviewer", server.base_url(), sample_metadata())
        .expect("tool transport should initialize");
    let result = tool.execute(json!({
        "prompt": "review the patch"
    }));

    assert!(!result.is_error);
    assert_eq!(
        result.text_output(),
        "[resource] file://workspace/report.json (application/json)"
    );
}
