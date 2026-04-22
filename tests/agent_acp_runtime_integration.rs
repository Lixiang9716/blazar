use blazar::agent::protocol::AgentEvent;
use blazar::agent::runtime::AgentRuntime;
use blazar::agent::tools::ToolSpec;
use blazar::config::load_agents_config_from_path;
use blazar::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use httpmock::prelude::*;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn fresh_workspace(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-workspaces")
        .join(format!("blazar-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("workspace should be created");
    path
}

fn write_agents_config(workspace: &Path, contents: &str) {
    let config_dir = workspace.join("config");
    fs::create_dir_all(&config_dir).expect("config dir should exist");
    fs::write(config_dir.join("agents.toml"), contents).expect("agents config should be written");
}

fn collect_events(runtime: &AgentRuntime, timeout: Duration) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Some(event) = runtime.try_recv() {
            let done = matches!(
                event,
                AgentEvent::TurnComplete | AgentEvent::TurnFailed { .. }
            );
            events.push(event);
            if done {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    events
}

struct CapturingProvider {
    seen_tools: Arc<Mutex<Vec<ToolSpec>>>,
}

impl LlmProvider for CapturingProvider {
    fn stream_turn(
        &self,
        _model: &str,
        _messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        *self.seen_tools.lock().expect("lock should not be poisoned") = tools.to_vec();
        let _ = tx.send(ProviderEvent::TextDelta("ok".into()));
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

#[test]
fn agents_config_parses_configured_agents_and_discovery_endpoints() {
    let workspace = fresh_workspace("acp-config");
    let config_path = workspace.join("config/agents.toml");
    write_agents_config(
        &workspace,
        r#"
            [[agents]]
            name = "configured_reviewer"
            endpoint = "http://127.0.0.1:9100"
            agent_id = "reviewer"
            enabled = true

            [[agents]]
            name = "disabled_helper"
            endpoint = "http://127.0.0.1:9200"
            agent_id = "helper"
            enabled = false

            [discovery]
            endpoints = ["http://127.0.0.1:9100", "http://127.0.0.1:9300"]
        "#,
    );

    let config = load_agents_config_from_path(&config_path).expect("ACP config should parse");

    assert_eq!(config.agents.len(), 2);
    assert_eq!(config.agents[0].name, "configured_reviewer");
    assert_eq!(config.agents[0].agent_id, "reviewer");
    assert_eq!(
        config.discovery.endpoints,
        vec![
            "http://127.0.0.1:9100".to_string(),
            "http://127.0.0.1:9300".to_string()
        ]
    );
}

#[test]
fn runtime_registers_configured_acp_tools_before_discovered_agents() {
    let workspace = fresh_workspace("acp-runtime");
    let server = MockServer::start();

    let _configured = server.mock(|when, then| {
        when.method(GET).path("/agents/reviewer");
        then.status(200).json_body(json!({
            "id": "reviewer",
            "name": "ACP reviewer",
            "description": "Reviews risky changes",
            "input_schema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }
        }));
    });

    let _discovery = server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!({
            "agents": [
                {
                    "id": "reviewer",
                    "name": "ACP reviewer",
                    "description": "Reviews risky changes",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "prompt": { "type": "string" }
                        },
                        "required": ["prompt"],
                        "additionalProperties": false
                    }
                },
                {
                    "id": "searcher",
                    "name": "discovered_searcher",
                    "description": "Searches the repository",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                }
            ]
        }));
    });

    write_agents_config(
        &workspace,
        &format!(
            r#"
                [[agents]]
                name = "configured_reviewer"
                endpoint = "{endpoint}"
                agent_id = "reviewer"
                enabled = true

                [discovery]
                endpoints = ["{endpoint}"]
            "#,
            endpoint = server.base_url()
        ),
    );

    let seen_tools = Arc::new(Mutex::new(Vec::new()));
    let runtime = AgentRuntime::new(
        Box::new(CapturingProvider {
            seen_tools: Arc::clone(&seen_tools),
        }),
        workspace,
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    runtime
        .submit_turn("list tools")
        .expect("turn should submit");
    let events = collect_events(&runtime, Duration::from_secs(2));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete))
    );

    let tools = seen_tools
        .lock()
        .expect("lock should not be poisoned")
        .clone();
    let acp_names = tools
        .iter()
        .filter(|spec| spec.name == "configured_reviewer" || spec.name == "discovered_searcher")
        .map(|spec| spec.name.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        acp_names,
        vec![
            "configured_reviewer".to_string(),
            "discovered_searcher".to_string()
        ]
    );
}

#[test]
fn runtime_keeps_successful_discoveries_when_other_endpoints_fail() {
    let workspace = fresh_workspace("acp-discovery-partial-failure");
    let server = MockServer::start();
    let _discovery = server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!({
            "agents": [
                {
                    "id": "searcher",
                    "name": "discovered_searcher",
                    "description": "Searches the repository",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                }
            ]
        }));
    });

    write_agents_config(
        &workspace,
        &format!(
            r#"
            [discovery]
            endpoints = ["not-a-valid-url", "{endpoint}"]
        "#,
            endpoint = server.base_url()
        ),
    );

    let seen_tools = Arc::new(Mutex::new(Vec::new()));
    let runtime = AgentRuntime::new(
        Box::new(CapturingProvider {
            seen_tools: Arc::clone(&seen_tools),
        }),
        workspace,
        "echo".to_owned(),
    )
    .expect("runtime should initialize despite one failing discovery endpoint");

    runtime
        .submit_turn("list tools")
        .expect("turn should submit");
    let events = collect_events(&runtime, Duration::from_secs(2));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete))
    );

    let tools = seen_tools
        .lock()
        .expect("lock should not be poisoned")
        .clone();
    assert!(
        tools.iter().any(|spec| spec.name == "discovered_searcher"),
        "successful discoveries should still register even when another endpoint fails"
    );
}

#[test]
fn runtime_allows_startup_when_all_discovery_endpoints_fail() {
    let workspace = fresh_workspace("acp-discovery-all-fail");
    write_agents_config(
        &workspace,
        r#"
            [discovery]
            endpoints = ["not-a-valid-url"]
        "#,
    );

    let runtime = AgentRuntime::new(
        Box::new(CapturingProvider {
            seen_tools: Arc::new(Mutex::new(Vec::new())),
        }),
        workspace,
        "echo".to_owned(),
    );

    assert!(
        runtime.is_ok(),
        "discovery failures alone should not be fatal for startup"
    );
}

#[test]
fn runtime_rejects_configured_acp_tool_name_collision_with_builtin() {
    let workspace = fresh_workspace("acp-built-in-collision");
    write_agents_config(
        &workspace,
        r#"
            [[agents]]
            name = "read_file"
            endpoint = "not-a-valid-url"
            agent_id = "reviewer"
            enabled = true
        "#,
    );

    let error = match AgentRuntime::new(
        Box::new(CapturingProvider {
            seen_tools: Arc::new(Mutex::new(Vec::new())),
        }),
        workspace,
        "echo".to_owned(),
    ) {
        Ok(_) => panic!("built-in ACP tool name collisions must be rejected"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ACP tool name collides with built-in tool: read_file")
    );
}

#[test]
fn runtime_skips_discovered_acp_tool_name_collision_with_builtin() {
    let workspace = fresh_workspace("acp-built-in-discovery-collision");
    let server = MockServer::start();

    let _discovery = server.mock(|when, then| {
        when.method(GET).path("/agents");
        then.status(200).json_body(json!({
            "agents": [
                {
                    "id": "shadow",
                    "name": "write_file",
                    "description": "Conflicts with built-in tool",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "prompt": { "type": "string" }
                        },
                        "required": ["prompt"],
                        "additionalProperties": false
                    }
                },
                {
                    "id": "searcher",
                    "name": "discovered_searcher",
                    "description": "Searches the repository",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                }
            ]
        }));
    });

    write_agents_config(
        &workspace,
        &format!(
            r#"
                [discovery]
                endpoints = ["{endpoint}"]
            "#,
            endpoint = server.base_url()
        ),
    );

    let seen_tools = Arc::new(Mutex::new(Vec::new()));
    let runtime = AgentRuntime::new(
        Box::new(CapturingProvider {
            seen_tools: Arc::clone(&seen_tools),
        }),
        workspace,
        "echo".to_owned(),
    )
    .expect("discovery collisions with built-ins should not be fatal");

    runtime
        .submit_turn("list tools")
        .expect("turn should submit");
    let events = collect_events(&runtime, Duration::from_secs(2));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete))
    );

    let tools = seen_tools
        .lock()
        .expect("lock should not be poisoned")
        .clone();
    assert_eq!(
        tools
            .iter()
            .filter(|spec| spec.name == "write_file")
            .count(),
        1,
        "built-in tool should remain unique after discovery"
    );
    assert!(tools.iter().any(|spec| spec.name == "discovered_searcher"));
}
