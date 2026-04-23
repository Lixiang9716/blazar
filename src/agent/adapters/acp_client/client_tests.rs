use httpmock::prelude::*;
use serde_json::json;
use std::error::Error as _;

use super::*;

#[test]
fn get_agent_parses_successful_payload() {
    let server = MockServer::start();
    let get_agent = server.mock(|when, then| {
        when.method(GET).path("/agents/reviewer");
        then.status(200).json_body(json!({
            "id": "reviewer",
            "name": "ACP Reviewer",
            "description": "Reviews code changes",
            "input_schema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" }
                }
            }
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let metadata = transport
        .get_agent(&server.base_url(), "reviewer")
        .expect("agent metadata should parse");

    assert_eq!(metadata.id, "reviewer");
    assert_eq!(metadata.name, "ACP Reviewer");
    assert_eq!(metadata.description, "Reviews code changes");
    get_agent.assert();
}

#[test]
fn get_agent_maps_http_status_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/agents/missing");
        then.status(404);
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let error = transport
        .get_agent(&server.base_url(), "missing")
        .expect_err("missing agent should return an error");

    match error {
        AcpClientError::HttpStatus {
            endpoint,
            action,
            status,
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "GET /agents/missing");
            assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
        }
        other => panic!("expected HTTP status error, got {other:?}"),
    }
}

#[test]
fn list_agents_parses_successful_payload() {
    let server = MockServer::start();
    let list_agents = server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!({
            "agents": [
                {
                    "id": "reviewer",
                    "name": "ACP Reviewer",
                    "description": "Reviews code",
                    "input_schema": { "type": "object" }
                },
                {
                    "id": "planner",
                    "name": "ACP Planner",
                    "description": "Plans implementation",
                    "input_schema": { "type": "object" }
                }
            ]
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let agents = transport
        .list_agents(&server.base_url())
        .expect("agent list should parse");

    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].id, "reviewer");
    assert_eq!(agents[1].id, "planner");
    list_agents.assert();
}

#[test]
fn list_agents_maps_protocol_errors_for_non_array_payload() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!({
            "agents": { "id": "not-an-array" }
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let error = transport
        .list_agents(&server.base_url())
        .expect_err("non-array payload should fail");

    match error {
        AcpClientError::Protocol {
            endpoint,
            action,
            message,
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "GET /agents");
            assert!(
                message.contains("array of agents"),
                "unexpected protocol message: {message}"
            );
        }
        other => panic!("expected protocol error, got {other:?}"),
    }
}

#[test]
fn create_run_parses_successful_payload() {
    let server = MockServer::start();
    let create_run = server.mock(|when, then| {
        when.method(POST).path("/runs").json_body(json!({
            "agent_id": "reviewer",
            "input": { "prompt": "review this patch" }
        }));
        then.status(200).json_body(json!({
            "id": "run-123"
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let run_id = transport
        .create_run(
            &server.base_url(),
            "reviewer",
            &json!({ "prompt": "review this patch" }),
        )
        .expect("run id should parse");

    assert_eq!(run_id, "run-123");
    create_run.assert();
}

#[test]
fn create_run_maps_protocol_errors_for_missing_run_id() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({
            "status": "queued"
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let error = transport
        .create_run(
            &server.base_url(),
            "reviewer",
            &json!({ "prompt": "hello" }),
        )
        .expect_err("missing run id should fail");

    match error {
        AcpClientError::Protocol {
            endpoint,
            action,
            message,
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "POST /runs");
            assert!(
                message.contains("run id"),
                "unexpected protocol message: {message}"
            );
        }
        other => panic!("expected protocol error, got {other:?}"),
    }
}

#[test]
fn get_run_parses_successful_payload() {
    let server = MockServer::start();
    let get_run = server.mock(|when, then| {
        when.method(GET).path("/runs/run-123");
        then.status(200).json_body(json!({
            "status": "completed",
            "output": {
                "text": "analysis complete"
            }
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let status = transport
        .get_run(&server.base_url(), "run-123")
        .expect("run status should parse");

    let AcpRunStatus::Complete(result) = status else {
        panic!("completed run should map to terminal result");
    };
    assert_eq!(result.text_output(), "analysis complete");
    assert!(!result.is_error);
    get_run.assert();
}

#[test]
fn get_run_maps_http_status_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/runs/missing");
        then.status(404);
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let error = transport
        .get_run(&server.base_url(), "missing")
        .expect_err("missing run should fail");

    match error {
        AcpClientError::HttpStatus {
            endpoint,
            action,
            status,
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "GET /runs/missing");
            assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
        }
        other => panic!("expected HTTP status error, got {other:?}"),
    }
}

#[test]
fn get_run_maps_protocol_errors_for_invalid_payload() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/runs/run-123");
        then.status(200).json_body(json!({
            "status": "completed"
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let error = transport
        .get_run(&server.base_url(), "run-123")
        .expect_err("invalid payload should fail protocol validation");

    match error {
        AcpClientError::Protocol {
            endpoint,
            action,
            message,
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "GET /runs/run-123");
            assert!(
                message.contains("must contain output"),
                "unexpected protocol message: {message}"
            );
        }
        other => panic!("expected protocol error, got {other:?}"),
    }
}

#[test]
fn normalize_endpoint_strips_trailing_slashes() {
    assert_eq!(
        normalize_endpoint("http://localhost:8080/"),
        "http://localhost:8080"
    );
    assert_eq!(
        normalize_endpoint("http://localhost:8080////"),
        "http://localhost:8080"
    );
    assert_eq!(
        normalize_endpoint("http://localhost:8080/api"),
        "http://localhost:8080/api"
    );
}

#[test]
fn join_url_reports_invalid_endpoint_errors() {
    let err = join_url("://bad-endpoint", "agents").expect_err("invalid endpoint should fail");
    match err {
        AcpClientError::InvalidEndpoint { endpoint, source } => {
            assert_eq!(endpoint, "://bad-endpoint");
            assert!(!source.is_empty());
        }
        other => panic!("expected invalid endpoint error, got {other:?}"),
    }
}

#[test]
fn list_agents_accepts_top_level_array_payload() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!([
            {
                "id": "reviewer",
                "name": "ACP Reviewer",
                "description": "Reviews code",
                "input_schema": { "type": "object" }
            }
        ]));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let agents = transport
        .list_agents(&server.base_url())
        .expect("top-level array payload should parse");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "reviewer");
}

#[test]
fn create_run_accepts_run_id_alias() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/runs");
        then.status(200).json_body(json!({
            "run_id": "run-alias-123"
        }));
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let run_id = transport
        .create_run(
            &server.base_url(),
            "reviewer",
            &json!({ "prompt": "hello" }),
        )
        .expect("run_id alias should parse");
    assert_eq!(run_id, "run-alias-123");
}

#[test]
fn get_agent_reports_decode_errors_for_invalid_json_body() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/agents/reviewer");
        then.status(200)
            .header("content-type", "application/json")
            .body("{not-json");
    });

    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let err = transport
        .get_agent(&server.base_url(), "reviewer")
        .expect_err("invalid json should produce decode error");
    match err {
        AcpClientError::Decode {
            endpoint, action, ..
        } => {
            assert_eq!(endpoint, server.base_url());
            assert_eq!(action, "GET /agents/reviewer");
        }
        other => panic!("expected decode error, got {other:?}"),
    }
}

#[test]
fn error_display_and_source_surface_operational_context() {
    let invalid = AcpClientError::InvalidEndpoint {
        endpoint: "bad".into(),
        source: "reason".into(),
    };
    assert!(
        invalid
            .to_string()
            .contains("invalid ACP endpoint bad: reason")
    );
    assert!(invalid.source().is_none());

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/agents/reviewer");
        then.status(200)
            .header("content-type", "application/json")
            .body("{invalid-json");
    });
    let transport = ReqwestAcpTransport::new().expect("transport should initialize");
    let decode = transport
        .get_agent(&server.base_url(), "reviewer")
        .expect_err("decode error should be surfaced");
    assert!(decode.to_string().contains("GET /agents/reviewer against"));
    assert!(decode.source().is_some());

    let protocol = AcpClientError::Protocol {
        endpoint: "http://localhost".into(),
        action: "GET /runs/run-1".into(),
        message: "malformed payload".into(),
    };
    assert!(
        protocol
            .to_string()
            .contains("invalid ACP payload: malformed payload")
    );
    assert!(protocol.source().is_none());
}
