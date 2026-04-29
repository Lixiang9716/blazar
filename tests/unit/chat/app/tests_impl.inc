use super::*;
use crate::agent::tools::{ToolCompatibilityTier, ToolRegistry};
use crate::chat::users_state::{StatusMode, UserMode, UsersLayoutPolicy};
use crate::chat::view::{render_to_lines_for_test, render_to_lines_for_test_with_users_policy};
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

struct UnicodeArgumentProvider;

impl LlmProvider for UnicodeArgumentProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let has_tool_result = messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

        if has_tool_result {
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let _ = tx.send(ProviderEvent::ToolCall {
            call_id: "call-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({
                "path": "😀".repeat(20)
            })
            .to_string(),
        });
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

struct UnicodeOutputProvider;

impl LlmProvider for UnicodeOutputProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let has_tool_result = messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

        if has_tool_result {
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let _ = tx.send(ProviderEvent::ToolCall {
            call_id: "call-1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({
                "command": "printf '\\U1F600%.0s' {1..20}"
            })
            .to_string(),
        });
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

struct CapturePromptProvider {
    prompt: Arc<Mutex<Option<String>>>,
}

impl LlmProvider for CapturePromptProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let prompt = messages
            .iter()
            .rev()
            .find_map(|message| match message {
                ProviderMessage::User { content } => Some(content.clone()),
                _ => None,
            })
            .expect("provider should receive the user prompt");

        *self.prompt.lock().expect("prompt mutex poisoned") = Some(prompt);
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

const TEST_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

fn new_test_app() -> ChatApp {
    ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("test app should initialize")
}

fn create_provider_config_repo(config_body: &str, name_prefix: &str) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let unique = format!(
        "{name_prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let repo_path = base.join(unique);
    std::fs::create_dir_all(repo_path.join("config")).expect("create config dir");
    std::fs::write(repo_path.join("config/provider.json"), config_body)
        .expect("write provider config");
    repo_path
}

fn latest_plan_id(workspace: &std::path::Path) -> String {
    let plans_dir = workspace.join(".blazar").join("plans");
    let mut ids = std::fs::read_dir(plans_dir)
        .expect("plans dir should exist")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.pop().expect("at least one plan should exist")
}

#[derive(Clone)]
struct FakeRuntime {
    events: std::sync::Arc<std::sync::Mutex<VecDeque<crate::agent::protocol::AgentEvent>>>,
    cancel_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    set_model_calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    submitted_prompts: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl FakeRuntime {
    fn with_events(events: Vec<crate::agent::protocol::AgentEvent>) -> Self {
        Self {
            events: std::sync::Arc::new(std::sync::Mutex::new(events.into_iter().collect())),
            cancel_called: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            set_model_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            submitted_prompts: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn cancel_called(&self) -> bool {
        self.cancel_called.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn last_set_model(&self) -> Option<String> {
        self.set_model_calls.lock().unwrap().last().cloned()
    }

    fn submitted_prompts(&self) -> Vec<String> {
        self.submitted_prompts.lock().unwrap().clone()
    }
}

impl crate::chat::runtime_port::AgentRuntimePort for FakeRuntime {
    fn submit_turn(&self, prompt: &str) -> Result<(), String> {
        self.submitted_prompts
            .lock()
            .unwrap()
            .push(prompt.to_owned());
        Ok(())
    }

    fn set_model(&self, model: &str) -> Result<(), String> {
        self.set_model_calls.lock().unwrap().push(model.to_owned());
        Ok(())
    }

    fn refresh_acp_agents(&self) -> Result<(), String> {
        Ok(())
    }

    fn cancel(&self) {
        self.cancel_called.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn try_recv(&self) -> Option<crate::agent::protocol::AgentEvent> {
        self.events.lock().unwrap().pop_front()
    }

    #[cfg(test)]
    fn shutdown_for_test(&mut self) {}
}

#[test]
fn chat_app_tick_drains_events_from_runtime_port() {
    use crate::agent::protocol::AgentEvent;

    let fake = FakeRuntime::with_events(vec![AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"x"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: vec![],
    }]);

    let mut app = ChatApp::new_with_runtime_for_test(env!("CARGO_MANIFEST_DIR"), Box::new(fake), "echo").expect("init");
    app.tick();
    assert!(app.timeline.iter().any(|e| matches!(e.kind, EntryKind::ToolCall { .. })));
}

#[test]
fn chat_app_cancel_turn_calls_runtime_port_cancel() {
    use crate::agent::protocol::AgentEvent;

    let fake = FakeRuntime::with_events(vec![]);
    let probe = fake.clone();
    let mut app = ChatApp::new_with_runtime_for_test(env!("CARGO_MANIFEST_DIR"), Box::new(fake), "echo").expect("init");
    // Mark turn as started so app considers it streaming
    app.apply_agent_event_for_test(AgentEvent::TurnStarted { turn_id: "t1".into() });
    assert!(app.is_streaming());
    app.cancel_turn();
    assert!(probe.cancel_called());
}

#[test]
fn chat_app_set_model_calls_runtime_port_set_model() {
    let fake = FakeRuntime::with_events(vec![]);
    let probe = fake.clone();
    let mut app = ChatApp::new_with_runtime_for_test(env!("CARGO_MANIFEST_DIR"), Box::new(fake), "echo").expect("init");
    app.set_model("new-model");
    assert_eq!(probe.last_set_model().as_deref(), Some("new-model"));
}

#[test]
fn chat_app_init_uses_config_fallback_without_model_listing_cache() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-init-config-fallback",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();

    crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::Panic(
            "startup should not list provider models",
        ),
        || {
            let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
                .expect("init should succeed without model listing");

            assert!(app.model_metadata.available_models.is_empty());
            assert_eq!(app.model_metadata.config_max_tokens, Some(4096));
            assert_eq!(app.model_metadata.model_context_max_tokens, None);

            app.apply_agent_event_for_test(AgentEvent::UsageUpdated(
                crate::agent::protocol::AgentUsage {
                    prompt_tokens: 50,
                    completion_tokens: 10,
                    total_tokens: 60,
                },
            ));

            assert_eq!(
                app.users_status_snapshot().context_usage,
                Some(ContextUsage {
                    used_tokens: 60,
                    max_tokens: 4096,
                })
            );
        },
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn set_model_does_not_block_on_async_metadata_refresh() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-model-refresh",
    );
    let repo_str = repo_path.to_string_lossy().to_string();

    crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "switched-model".to_owned(),
                description: "fresh entry".to_owned(),
                context_length: Some(65536),
            }],
            delay: Duration::from_millis(600),
        },
        || {
            let (result_tx, result_rx) = std::sync::mpsc::channel();
            let repo_for_thread = repo_str.clone();
            let handle = std::thread::spawn(move || {
                let fake = FakeRuntime::with_events(vec![]);
                let mut app =
                    ChatApp::new_with_runtime_for_test(&repo_for_thread, Box::new(fake), "echo")
                        .expect("init");
                app.set_model("switched-model");
                result_tx
                    .send((
                        app.model_metadata.available_models.is_empty(),
                        app.model_metadata.model_context_max_tokens,
                        app.model_metadata.config_max_tokens,
                    ))
                    .expect("send set_model result");
            });

            let (models_empty, model_context_max_tokens, config_max_tokens) = result_rx
                .recv_timeout(Duration::from_millis(250))
                .expect("set_model should return before provider listing completes");
            handle.join().expect("set_model thread should join");

            assert!(models_empty);
            assert_eq!(model_context_max_tokens, None);
            assert_eq!(config_max_tokens, Some(4096));
        },
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn async_metadata_refresh_result_is_applied_on_tick() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-async-model-refresh",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");
    app.model_metadata.available_models = vec![crate::provider::ModelInfo {
        id: "switched-model".to_owned(),
        description: "stale entry".to_owned(),
        context_length: Some(1024),
    }];
    app.model_metadata.model_context_max_tokens = Some(1024);

    let refreshed = crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "switched-model".to_owned(),
                description: "fresh entry".to_owned(),
                context_length: Some(65536),
            }],
            delay: Duration::from_millis(50),
        },
        || {
            app.set_model("switched-model");
            assert_eq!(app.model_metadata.available_models[0].description, "stale entry");
            assert_eq!(app.model_metadata.model_context_max_tokens, Some(1024));
            wait_until(TEST_WAIT_TIMEOUT, || {
                app.tick();
                app.model_metadata.available_models
                    .iter()
                    .any(|model| model.description == "fresh entry")
                    && app.model_metadata.model_context_max_tokens == Some(65536)
            })
        },
    );

    assert!(refreshed, "tick should apply async metadata refresh result");
    assert_eq!(app.model_metadata.config_max_tokens, Some(4096));
    assert_eq!(app.model_metadata.available_models.len(), 1);
    assert_eq!(app.model_metadata.available_models[0].description, "fresh entry");
    assert_eq!(app.model_metadata.model_context_max_tokens, Some(65536));

    app.apply_agent_event_for_test(AgentEvent::UsageUpdated(
        crate::agent::protocol::AgentUsage {
            prompt_tokens: 50,
            completion_tokens: 10,
            total_tokens: 60,
        },
    ));

    assert_eq!(
        app.users_status_snapshot()
            .context_usage
            .expect("usage snapshot should be present")
            .max_tokens,
        65536
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn tick_periodically_refreshes_model_metadata_for_long_lived_sessions() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-periodic-model-refresh",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");
    app.model_name = "periodic-model".to_owned();
    app.model_metadata.interval = Duration::ZERO;

    crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "periodic-model".to_owned(),
                description: "periodic entry".to_owned(),
                context_length: Some(32768),
            }],
            delay: Duration::from_millis(50),
        },
        || {
            app.tick();
            assert!(app.model_metadata.available_models.is_empty());
            assert_eq!(app.model_metadata.model_context_max_tokens, None);

            let refreshed = wait_until(TEST_WAIT_TIMEOUT, || {
                app.tick();
                app.model_metadata.available_models
                    .iter()
                    .any(|model| model.description == "periodic entry")
                    && app.model_metadata.model_context_max_tokens == Some(32768)
            });
            assert!(
                refreshed,
                "periodic tick refresh should populate metadata for the active model"
            );
        },
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn timed_out_metadata_refresh_allows_periodic_retry() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-refresh-timeout-retry",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");
    app.model_name = "retry-model".to_owned();
    app.model_metadata.interval = Duration::ZERO;
    app.model_metadata.timeout = Duration::from_millis(10);

    crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "retry-model".to_owned(),
                description: "stalled entry".to_owned(),
                context_length: Some(1024),
            }],
            delay: Duration::from_secs(5),
        },
        || {
            app.tick();
            std::thread::sleep(Duration::from_millis(25));
        },
    );

    let retry_start = Instant::now();
    let recovered = crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "retry-model".to_owned(),
                description: "retry entry".to_owned(),
                context_length: Some(24576),
            }],
            delay: Duration::from_millis(5),
        },
        || {
            wait_until(TEST_WAIT_TIMEOUT, || {
                app.tick();
                app.model_metadata.available_models
                    .iter()
                    .any(|model| model.description == "retry entry")
                    && app.model_metadata.model_context_max_tokens == Some(24576)
            })
        },
    );

    assert!(
        recovered,
        "timed out in-flight refresh should not block a later periodic retry"
    );
    assert!(
        retry_start.elapsed() < Duration::from_secs(1),
        "retry should recover before the stalled worker finishes"
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn failed_metadata_refresh_allows_periodic_retry() {
    let repo_path = create_provider_config_repo(
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "max_tokens": 4096
}"#,
        "chat-app-refresh-failure-retry",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");
    app.model_name = "failure-retry-model".to_owned();
    app.model_metadata.interval = Duration::ZERO;
    app.model_metadata.timeout = Duration::from_millis(10);

    crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::Panic("simulated async refresh failure"),
        || {
            app.tick();
            std::thread::sleep(Duration::from_millis(25));
        },
    );

    let recovered = crate::provider::with_available_models_behavior_for_test(
        crate::provider::AvailableModelsTestBehavior::DelayedReturn {
            models: vec![crate::provider::ModelInfo {
                id: "failure-retry-model".to_owned(),
                description: "failure retry entry".to_owned(),
                context_length: Some(28672),
            }],
            delay: Duration::from_millis(5),
        },
        || {
            wait_until(TEST_WAIT_TIMEOUT, || {
                app.tick();
                app.model_metadata.available_models
                    .iter()
                    .any(|model| model.description == "failure retry entry")
                    && app.model_metadata.model_context_max_tokens == Some(28672)
            })
        },
    );

    assert!(
        recovered,
        "failed async refresh should not block a later periodic retry"
    );

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

struct RuntimeProbeCommand {
    spec: crate::chat::commands::CommandSpec,
}

impl RuntimeProbeCommand {
    fn new(name: &str) -> Self {
        Self {
            spec: crate::chat::commands::CommandSpec {
                name: name.to_owned(),
                description: "Runtime probe command".to_owned(),
                args_schema: serde_json::json!({ "type": "object" }),
            },
        }
    }
}

impl crate::chat::commands::PaletteCommand for RuntimeProbeCommand {
    fn spec(&self) -> &crate::chat::commands::CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut crate::chat::commands::orchestrator::CommandContext<'a>,
        _args: serde_json::Value,
    ) -> crate::chat::commands::types::CommandExecFuture<'a> {
        Box::pin(async move {
            if tokio::runtime::Handle::try_current().is_err() {
                return Err(crate::chat::commands::CommandError::ExecutionFailed(
                    "missing tokio runtime context".to_owned(),
                ));
            }
            ctx.app.set_composer_text("/runtime-ok");
            Ok(crate::chat::commands::CommandResult {
                summary: "runtime context available".to_owned(),
            })
        })
    }
}

fn register_runtime_probe_command(app: &mut ChatApp, name: &str) {
    app.command_registry
        .register(Arc::new(RuntimeProbeCommand::new(name)))
        .expect("runtime probe command should register");
}

#[test]
fn chat_runtime_tool_registry_reports_bridge_compatibility_tier_for_bash() {
    let registry = ToolRegistry::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    assert_eq!(
        registry.compatibility_tier("bash"),
        Some(ToolCompatibilityTier::CompatibilityBridge)
    );
}

#[test]
fn users_region_defaults_to_auto_and_normal_status() {
    let app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    let snapshot = app.users_status_snapshot();

    assert_eq!(snapshot.mode, UserMode::Auto);
    assert_eq!(snapshot.status_mode, StatusMode::Normal);
    assert!(snapshot.context_usage.is_none());
}

#[test]
fn metadata_test_helpers_update_users_status_snapshot() {
    let mut app = new_test_app();
    app.set_pr_label_for_test(Some("PR#42 improve timeline".to_owned()));
    app.set_referenced_files_for_test(vec!["src/chat/view/mod.rs".to_owned()]);

    let snapshot = app.users_status_snapshot();
    assert_eq!(snapshot.pr_label.as_deref(), Some("PR#42 improve timeline"));
    assert_eq!(
        snapshot.referenced_files,
        vec!["src/chat/view/mod.rs".to_owned()]
    );
}

#[test]
fn agent_usage_updated_event_updates_users_context_snapshot() {
    let mut app = new_test_app();
    app.model_metadata.model_context_max_tokens = None;
    app.model_metadata.config_max_tokens = None;
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated(
        crate::agent::protocol::AgentUsage {
            prompt_tokens: 120,
            completion_tokens: 30,
            total_tokens: 150,
        },
    ));

    let snapshot = app.users_status_snapshot();
    assert_eq!(
        snapshot.context_usage,
        Some(ContextUsage {
            used_tokens: 150,
            max_tokens: 0,
        })
    );
}

#[test]
fn usage_update_prefers_model_context_length_over_config_max() {
    let mut app = new_test_app();
    app.model_metadata.model_context_max_tokens = Some(128000);
    app.model_metadata.config_max_tokens = Some(8192);
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated(
        crate::agent::protocol::AgentUsage {
            prompt_tokens: 50,
            completion_tokens: 10,
            total_tokens: 60,
        },
    ));

    assert_eq!(
        app.users_status_snapshot().context_usage,
        Some(ContextUsage {
            used_tokens: 60,
            max_tokens: 128000,
        })
    );
}

#[test]
fn usage_update_falls_back_to_config_max_when_context_unknown() {
    let mut app = new_test_app();
    app.model_metadata.model_context_max_tokens = None;
    app.model_metadata.config_max_tokens = Some(8192);
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated(
        crate::agent::protocol::AgentUsage {
            prompt_tokens: 50,
            completion_tokens: 10,
            total_tokens: 60,
        },
    ));

    assert_eq!(
        app.users_status_snapshot().context_usage,
        Some(ContextUsage {
            used_tokens: 60,
            max_tokens: 8192,
        })
    );
}

#[test]
fn tool_call_started_updates_referenced_files_from_normalized_claims() {
    let mut app = new_test_app();
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-refs".into(),
        tool_name: "write_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"src/chat/view/status.rs"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: vec![
            "process:bash#exclusive".into(),
            "fs:src/chat/view/status.rs#read-only".into(),
            "fs:src/chat/app.rs#read-write".into(),
        ],
    });

    let snapshot = app.users_status_snapshot();
    assert_eq!(
        snapshot.referenced_files,
        vec![
            "src/chat/view/status.rs".to_owned(),
            "src/chat/app.rs".to_owned()
        ]
    );
}

#[test]
fn infer_pr_label_from_branch_accepts_expected_patterns() {
    let cases = [
        ("pr-42", Some("PR#42")),
        ("feature/pr-42-fix", Some("PR#42")),
        ("pr_42", Some("PR#42")),
        ("feature/pr_42_fix", Some("PR#42")),
        ("pull/108", Some("PR#108")),
        ("feature/pr/77-improve", Some("PR#77")),
        ("release#91", Some("PR#91")),
    ];

    for (branch, expected) in cases {
        assert_eq!(super::infer_pr_label_from_branch(branch).as_deref(), expected);
    }
}

#[test]
fn infer_pr_label_from_branch_rejects_non_pr_segments() {
    let cases = [
        "feature/spr-123-ui",
        "feature/apr_55_cleanup",
        "feature/pr42-without-delimiter",
        "feature/improve-pr-42",
    ];

    for branch in cases {
        assert_eq!(
            super::infer_pr_label_from_branch(branch),
            None,
            "branch {branch} should not infer a PR label"
        );
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

#[test]
fn tick_handles_multibyte_tool_arguments_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = new_test_app();
    app.agent_runtime = Box::new(AgentRuntime::new(
        Box::new(UnicodeArgumentProvider),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    ).expect("runtime should initialize"));

    app.agent_runtime
        .submit_turn("read unicode path")
        .expect("turn submission should succeed");

    let mut panicked = false;
    let observed = wait_until(TEST_WAIT_TIMEOUT, || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));
        if result.is_err() {
            panicked = true;
            return true;
        }
        app.timeline
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::ToolCall { .. }))
    });

    assert!(
        !panicked,
        "tick should not panic on multibyte tool arguments"
    );
    assert!(observed, "tool call event was not observed within timeout");
}

#[test]
fn tick_handles_multibyte_tool_output_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = new_test_app();
    app.agent_runtime = Box::new(AgentRuntime::new(
        Box::new(UnicodeOutputProvider),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    ).expect("runtime should initialize"));

    app.agent_runtime
        .submit_turn("render unicode output")
        .expect("turn submission should succeed");

    let mut panicked = false;
    let observed = wait_until(TEST_WAIT_TIMEOUT, || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));
        if result.is_err() {
            panicked = true;
            return true;
        }
        app.timeline.iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::ToolCall {
                    status: ToolCallStatus::Success,
                    ..
                }
            )
        })
    });

    assert!(!panicked, "tick should not panic on multibyte tool output");
    assert!(
        observed,
        "tool completion event was not observed within timeout"
    );
}

#[test]
fn send_message_queues_when_agent_busy_without_immediate_timeline_insert() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    // First message dispatches normally
    app.send_message("first message");
    assert!(app.pending_messages.is_empty());

    // Simulate agent becoming busy
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t1".into(),
    });
    assert!(app.agent_state.is_busy());

    // Second and third messages should be queued, not dropped
    app.send_message("second message");
    app.send_message("third message");
    assert_eq!(app.pending_messages.len(), 2);
    assert_eq!(app.pending_messages[0].user_text, "second message");
    assert_eq!(app.pending_messages[1].user_text, "third message");
    assert_eq!(app.messages().len(), 1);
    assert_eq!(app.messages()[0].body, "first message");
    let user_messages: Vec<&str> = app
        .timeline
        .iter()
        .filter(|e| e.actor == Actor::User)
        .map(|e| e.body.as_str())
        .collect();
    assert_eq!(user_messages, vec!["first message"]);
}

#[test]
fn send_message_queues_plan_continue_as_command_when_busy() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });
    assert!(app.agent_state.is_busy());

    app.send_message("/plan --continue plan-123 --review revise");

    assert_eq!(app.pending_messages.len(), 1);
    let queued = &app.pending_messages[0];
    assert_eq!(queued.user_text, "/plan --continue plan-123 --review revise");
    match &queued.dispatch {
        super::PendingDispatch::Command { name, args } => {
            assert_eq!(name, "/plan");
            assert_eq!(
                args.get("continue_id").and_then(serde_json::Value::as_str),
                Some("plan-123")
            );
            assert_eq!(
                args.get("review").and_then(serde_json::Value::as_str),
                Some("revise")
            );
        }
        other => panic!("expected queued command dispatch, got {other:?}"),
    }
}

#[test]
fn queued_user_texts_for_render_reflects_pending_queue_fifo() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("first");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });

    app.send_message("second");
    app.send_message("third");

    assert_eq!(
        app.queued_user_texts_for_render(),
        vec!["second".to_string(), "third".to_string()]
    );
}

#[test]
fn dispatch_next_queued_drains_fifo_on_turn_complete() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    // Queue two messages
    app.pending_messages
        .push_back(build_pending_turn("queued-a"));
    app.pending_messages
        .push_back(build_pending_turn("queued-b"));

    // Simulate TurnComplete — should dispatch first queued message
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.pending_messages.len(), 1);
    assert_eq!(app.pending_messages[0].user_text, "queued-b");
    assert_eq!(app.messages().len(), 1);
    assert_eq!(app.messages()[0].body, "queued-a");
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "queued-a")
    );
}

#[test]
fn discover_agents_queued_while_busy_and_runs_after_turn() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("first message");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t1".into(),
    });
    assert!(app.agent_state.is_busy());

    app.send_message("/discover-agents");
    app.send_message("after");
    assert_eq!(app.pending_messages.len(), 2);
    assert_eq!(app.messages().len(), 1);
    assert_eq!(app.messages()[0].body, "first message");

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.pending_messages.len(), 1);
    assert_eq!(app.pending_messages[0].user_text, "after");
    assert!(app.messages().iter().any(|msg| msg.body == "/discover-agents"));
    assert!(!app.messages().iter().any(|msg| msg.body == "after"));

    app.apply_agent_event_for_test(AgentEvent::AcpAgentsRefreshed);
    assert!(app.pending_messages.is_empty());
    assert!(app.messages().iter().any(|msg| msg.body == "after"));
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "after")
    );
}

#[test]
fn discover_agents_refresh_failure_continues_queue() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("first message");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t1".into(),
    });
    assert!(app.agent_state.is_busy());

    app.send_message("/discover-agents");
    app.send_message("after");
    assert_eq!(app.pending_messages.len(), 2);
    assert_eq!(app.messages().len(), 1);
    assert_eq!(app.messages()[0].body, "first message");

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.pending_messages.len(), 1);
    assert_eq!(app.pending_messages[0].user_text, "after");
    assert!(app.messages().iter().any(|msg| msg.body == "/discover-agents"));
    assert!(!app.messages().iter().any(|msg| msg.body == "after"));

    app.apply_agent_event_for_test(AgentEvent::AcpAgentsRefreshFailed {
        error: "x".into(),
    });
    assert!(app.pending_messages.is_empty());
    assert!(app.messages().iter().any(|msg| msg.body == "after"));
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "after")
    );
}

#[test]
fn streaming_text_not_split_by_queued_user_input() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("streaming request");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "stream-1".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta { text: "A".into() });

    app.send_message("queued while busy");
    assert_eq!(app.pending_messages.len(), 1);
    assert!(!app.messages().iter().any(|msg| msg.body == "queued while busy"));

    app.apply_agent_event_for_test(AgentEvent::TextDelta { text: "B".into() });

    let streamed_responses: Vec<&TimelineEntry> = app
        .timeline
        .iter()
        .filter(|entry| {
            entry.actor == Actor::Assistant
                && entry.kind == EntryKind::Message
                && entry.body.contains('A')
        })
        .collect();
    assert_eq!(streamed_responses.len(), 1);
    assert_eq!(streamed_responses[0].body, "AB");
}

#[test]
fn dispatch_next_queued_drains_on_turn_failed() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.pending_messages
        .push_back(build_pending_turn("queued-after-fail"));

    // TurnFailed should also drain the queue
    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
        error: "test error".into(),
    });
    assert!(app.pending_messages.is_empty());
}

#[test]
fn runtime_submit_failure_continues_queued_dispatch() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.pending_messages
        .push_back(build_pending_turn("queued-a"));
    app.pending_messages
        .push_back(build_pending_turn("queued-b"));
    app.agent_runtime.shutdown_for_test();

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    assert!(app.pending_messages.is_empty());
    assert!(app.messages().iter().any(|msg| msg.body == "queued-a"));
    assert!(app.messages().iter().any(|msg| msg.body == "queued-b"));
    let warning_count = app
        .timeline
        .iter()
        .filter(|entry| entry.kind == EntryKind::Warning)
        .count();
    assert!(warning_count >= 2);
}

#[test]
fn discover_dispatch_failure_continues_queued_dispatch() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.pending_messages
        .push_back(build_pending_turn("/discover-agents"));
    app.pending_messages.push_back(build_pending_turn("after"));
    app.agent_runtime.shutdown_for_test();

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    assert!(app.pending_messages.is_empty());
    assert!(app.messages().iter().any(|msg| msg.body == "/discover-agents"));
    assert!(app.messages().iter().any(|msg| msg.body == "after"));
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.kind == EntryKind::Warning && entry.body.contains("Failed to refresh ACP agents"))
    );
}

#[test]
fn paste_action_inserts_into_composer_without_submitting() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    let pasted = "line one\nline two\nline three";
    app.handle_action(InputAction::Paste(pasted.to_owned()));

    let text = app.composer_text();
    assert!(text.contains("line one"));
    assert!(text.contains("line two"));
    assert!(text.contains("line three"));
    // No message was sent — timeline should only have the welcome message
    let user_msgs: Vec<_> = app
        .timeline
        .iter()
        .filter(|e| e.actor == Actor::User)
        .collect();
    assert!(user_msgs.is_empty(), "paste should not auto-submit");
}

#[test]
fn plan_command_send_message_dispatches_to_plugin_command_path() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-workspaces")
        .join(format!(
            "send-message-plan-dispatch-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be monotonic")
                .as_nanos()
        ));
    std::fs::create_dir_all(&workspace).expect("workspace should exist");
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&workspace)
        .output();

    let mut app = ChatApp::new_for_test(workspace.to_str().expect("utf8 workspace")).expect("app");
    app.send_message("/plan add minimax provider");

    let composer = app.composer_text();
    let plan_id = latest_plan_id(&workspace);
    assert_eq!(composer, "/plan");
    let plan_path = workspace
        .join(".blazar")
        .join("plans")
        .join(format!("{plan_id}.json"));
    let raw = std::fs::read_to_string(plan_path).expect("plan session json should exist");
    let saved: serde_json::Value = serde_json::from_str(&raw).expect("plan json should parse");
    assert_eq!(
        saved.get("goal").and_then(serde_json::Value::as_str),
        Some("add minimax provider")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn planning_turn_uses_planning_while_streaming_then_sets_title() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::ToggleMode);

    app.send_message("add minimax provider");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "plan-1".into(),
    });

    let streaming_lines = render_to_lines_for_test(&mut app, 90, 24);
    let streaming_text = streaming_lines.join("\n");
    assert!(
        streaming_text.contains("planning"),
        "planning turns should show planning while streaming"
    );

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
            text: "MiniMax Provider Integration\n\n1. Review current provider abstraction\n2. Add provider config\n".into(),
        });
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let assistant_entry = app
        .timeline
        .iter()
        .rev()
        .find(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
        .expect("assistant response entry should exist");

    assert_eq!(
        assistant_entry.title.as_deref(),
        Some("MiniMax Provider Integration")
    );
    assert_eq!(
        assistant_entry.body,
        "1. Review current provider abstraction\n2. Add provider config"
    );

    let completed_lines = render_to_lines_for_test(&mut app, 90, 24);
    let completed_text = completed_lines.join("\n");
    assert!(completed_text.contains("Review current provider abstraction"));
    assert!(!completed_text.contains("Blazar #2"));
}

#[test]
fn follow_up_turn_keeps_action_first_status_label() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(
        TimelineEntry::response("1. Review current provider abstraction")
            .with_title("MiniMax Provider Integration"),
    );
    app.active_turn_kind = Some(TurnKind::Chat);
    app.active_turn_title = Some("MiniMax Provider Integration".into());
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "exec-1".into(),
    });

    assert_eq!(app.status_label(), "thinking");
}

#[test]
fn picker_backspace_pops_filter_then_closes_overlay() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.picker.open();
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.picker.filter, "/h");

    app.handle_action(InputAction::Backspace);
    assert!(app.picker.is_open());
    assert_eq!(app.picker.filter, "/");

    app.handle_action(InputAction::Backspace);
    assert!(app.picker.is_open());
    assert!(app.picker.filter.is_empty());

    app.handle_action(InputAction::Backspace);
    assert!(!app.picker.is_open());
}

#[test]
fn picker_theme_selection_opens_subpicker_and_applies_theme() {
    use crate::chat::picker::PickerContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "theme".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);
    assert_eq!(app.picker.context, PickerContext::ThemeSelect);
    assert!(app.picker.is_open());

    let selected_theme = app
        .picker
        .select_current()
        .expect("theme picker should have a selection");
    app.handle_action(InputAction::Submit);

    assert_eq!(app.theme_name(), selected_theme);
    assert_eq!(app.picker.context, PickerContext::Commands);
}

#[test]
fn numeric_message_no_longer_triggers_demo_reset() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let initial_len = app.timeline.len();

    app.send_message("1");
    assert_eq!(app.timeline.len(), initial_len + 1);
    assert_eq!(
        app.timeline.last().expect("timeline should contain user message").body,
        "1"
    );
}

#[test]
fn turn_helpers_ignore_blank_input_and_validate_plan_title() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let initial_len = app.timeline.len();
    app.send_message("   ");
    assert_eq!(app.timeline.len(), initial_len);

    assert!(super::turns::extract_plan_title_and_body("").is_none());
    assert!(super::turns::extract_plan_title_and_body("###\n\nbody").is_none());
    let parsed = super::turns::extract_plan_title_and_body("Title\n\n1. step").expect("valid plan");
    assert_eq!(parsed.0, "Title");
    assert_eq!(parsed.1, "1. step");
}

#[test]
fn short_action_name_from_text_extracts_first_action_word() {
    assert_eq!(
        super::turns::short_action_name_from_text("Next step: explore scheduler flow"),
        Some("explore".to_owned())
    );
}

#[test]
fn extract_plan_action_names_reads_numbered_plan_body() {
    let body = "1. Explore codebase\n2. Patch status derivation\n3. Add tests";
    assert_eq!(
        super::turns::extract_plan_action_names(body),
        vec![
            "explore".to_owned(),
            "patch".to_owned(),
            "add".to_owned()
        ]
    );
}

#[test]
fn parse_next_step_name_line_extracts_explicit_name() {
    assert_eq!(
        super::turns::parse_next_step_name_line("next_step_name: explore\nthen continue"),
        Some("explore".to_owned())
    );
}

#[test]
fn short_action_name_is_kept_concise() {
    assert_eq!(
        super::turns::short_action_name_from_text("Next step: investigate-super-long-name"),
        Some("investigat…".to_owned())
    );
}

#[test]
fn turn_failed_events_use_error_kind_for_cancel_hint() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::Cancelled,
        error: "not-the-cancelled-literal".into(),
    });
    assert!(matches!(
        app.timeline.last().map(|entry| &entry.kind),
        Some(EntryKind::Hint)
    ));

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
        error: "cancelled".into(),
    });
    let last = app.timeline.last().expect("warning entry should exist");
    assert!(matches!(last.kind, EntryKind::Warning));
    assert!(last.body.contains("Agent error: cancelled"));
}

#[test]
fn turn_failed_logs_redact_sensitive_error_text_for_app_log_path() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();
        let sensitive_error = "invalid API key sk-test-secret";

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-sensitive".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::TurnFailed {
            kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
            error: sensitive_error.into(),
        });
    });

    let logged = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("turn_failed")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-sensitive")
                && event
                    .get("error_kind")
                    .and_then(serde_json::Value::as_str)
                    == Some("ProviderFatal")
        })
        .expect("expected redacted structured turn_failed app log event");

    let message = logged
        .get("message")
        .and_then(serde_json::Value::as_str)
        .expect("structured message should be present");
    assert!(message.contains("details=redacted"));
    assert!(message.contains("kind=ProviderFatal"));
    assert!(message.contains("turn_id=turn-sensitive"));
    assert!(!message.contains("invalid API key"));
    assert!(!message.contains("sk-test-secret"));
    assert_eq!(
        message,
        super::events::turn_failed_app_log_message(
            crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
            Some("turn-sensitive"),
        )
    );
}

#[test]
fn quit_and_scroll_actions_follow_runtime_state_rules() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline_content_height.set(30);
    app.timeline_visible_height.set(10);
    app.scroll_offset = u16::MAX;

    app.handle_action(InputAction::ScrollUp);
    assert_eq!(app.scroll_offset, 17);
    app.handle_action(InputAction::ScrollDown);
    assert_eq!(app.scroll_offset, 20);

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}

#[test]
fn command_mode_scroll_routes_to_users_command_window() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.set_composer_text("/");
    assert_eq!(app.users_status_snapshot().status_mode, StatusMode::CommandList);
    assert!(
        app.inline_command_matches().len() > 6,
        "test requires the full command list to remain available"
    );

    app.timeline_content_height.set(30);
    app.timeline_visible_height.set(10);
    app.scroll_offset = 5;
    let before = app.users_command_scroll_offset();

    for _ in 0..6 {
        app.handle_action(InputAction::ScrollDown);
    }

    assert_eq!(app.users_command_scroll_offset(), before + 6);

    app.handle_action(InputAction::ScrollDown);

    assert_eq!(app.scroll_offset, 5, "timeline scroll should remain untouched");
    assert_eq!(app.users_command_scroll_offset(), before + 7);

    app.handle_action(InputAction::ScrollUp);
    assert_eq!(app.scroll_offset, 5, "timeline scroll should still remain untouched");
    assert_eq!(app.users_command_scroll_offset(), before + 6);
}

#[test]
fn normal_mode_scroll_keeps_timeline_scroll_behavior() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline_content_height.set(30);
    app.timeline_visible_height.set(10);
    app.scroll_offset = 5;

    app.handle_action(InputAction::ScrollDown);

    assert_eq!(app.scroll_offset, 8);
    assert_eq!(app.users_command_scroll_offset(), 0);
}

#[test]
fn set_model_sends_command_and_updates_timeline() {
    let mut app = new_test_app();
    app.set_model("Qwen/Qwen3-8B");

    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Model switched to"));
    assert_eq!(app.model_name(), "Qwen/Qwen3-8B");
}

#[test]
fn picker_plan_command_prefills_composer_shortcut() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-workspaces");
    std::fs::create_dir_all(&workspace).expect("test-workspaces dir should exist");
    let unique_workspace = workspace.join(format!(
        "picker-plan-prefill-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    ));
    std::fs::create_dir_all(&unique_workspace).expect("unique workspace should exist");

    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&unique_workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&unique_workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&unique_workspace)
        .output();

    let mut app =
        ChatApp::new_for_test(unique_workspace.to_str().expect("workspace path should be utf-8"))
            .expect("test app should initialize");
    app.picker.open();
    for ch in "plan".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }

    app.handle_action(InputAction::Submit);
    let composer = app.composer_text();
    assert_eq!(composer, "/plan", "expected simplified plan shortcut");
    assert!(
        !latest_plan_id(&unique_workspace).is_empty(),
        "submitting /plan command should create a plan session"
    );

    let _ = std::fs::remove_dir_all(unique_workspace);
}

#[test]
fn slash_input_switches_to_inline_command_status_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    assert!(!app.picker.is_open());
    assert_eq!(
        app.users_status_snapshot().status_mode,
        StatusMode::CommandList
    );

    let policy = UsersLayoutPolicy {
        top_height: 4,
        input_height: 1,
        model_height: 1,
        max_command_window_size: 3,
    };
    let lines = render_to_lines_for_test_with_users_policy(&mut app, 100, 18, policy);
    let users_rows = &lines[lines.len().saturating_sub(8)..];

    assert_eq!(users_rows[0].trim(), "~/blazar · main");
    // Commands are now sorted alphabetically via inventory registration
    assert_eq!(users_rows[1].trim(), "> /agents");
    assert_eq!(users_rows[2].trim(), "• /clear");
    assert_eq!(users_rows[3].trim(), "• /compact");
}

#[test]
fn slash_command_list_supports_arrow_key_selection() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.users_command_scroll_offset(), 0);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.users_command_scroll_offset(), 1);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Up,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.users_command_scroll_offset(), 0);
}

#[test]
fn slash_when_composer_not_empty_does_not_open_picker() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.set_composer_text("x");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    assert!(!app.picker.is_open());
    assert!(app.composer_text().contains('/'));
}

#[test]
fn shift_enter_inserts_newline_without_submit() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::InsertNewline);
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.composer_text(), "a\nb");
    assert!(
        !app.timeline.iter().any(|entry| {
            entry.actor == Actor::User && entry.body.contains("a\nb")
        }),
        "inserting a newline should not submit composer text"
    );
}

#[test]
fn shift_tab_toggles_user_mode() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    assert_eq!(app.users_status_snapshot().mode, UserMode::Auto);

    app.handle_action(InputAction::ToggleMode);
    assert_eq!(app.users_status_snapshot().mode, UserMode::Plan);

    app.handle_action(InputAction::ToggleMode);
    assert_eq!(app.users_status_snapshot().mode, UserMode::Auto);
}

#[test]
fn submit_in_plan_mode_uses_plan_prompt_path() {
    let captured_prompt = Arc::new(Mutex::new(None));
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime = Box::new(AgentRuntime::new(
        Box::new(CapturePromptProvider {
            prompt: captured_prompt.clone(),
        }),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    ).expect("runtime should initialize"));

    app.handle_action(InputAction::ToggleMode);
    app.set_composer_text("optimize parser");
    app.submit_composer();

    let captured = wait_until(TEST_WAIT_TIMEOUT, || {
        app.tick();
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_some()
    });
    assert!(captured, "planning prompt was not captured within timeout");

    let prompt = captured_prompt
        .lock()
        .expect("prompt mutex poisoned")
        .clone()
        .expect("provider should capture a prompt");

    assert!(prompt.contains("planning mode"));
    assert!(prompt.contains("structured response contract"));
    assert!(prompt.contains("intent: use 'plan'"));
    assert!(prompt.contains("ask concise clarifying questions first"));
    assert!(prompt.contains("User request:"));
    assert!(prompt.contains("optimize parser"));
}

#[test]
fn slash_query_submit_in_plan_mode_keeps_plan_runtime_path() {
    let captured_prompt = Arc::new(Mutex::new(None));
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime = Box::new(
        AgentRuntime::new(
            Box::new(CapturePromptProvider {
                prompt: captured_prompt.clone(),
            }),
            PathBuf::from(repo_path),
            "echo".to_owned(),
        )
        .expect("runtime should initialize"),
    );

    app.handle_action(InputAction::ToggleMode);
    app.set_composer_text("/");
    app.submit_composer();

    let captured = wait_until(TEST_WAIT_TIMEOUT, || {
        app.tick();
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_some()
    });
    assert!(
        captured,
        "slash query submit in plan mode should still dispatch planning runtime prompt"
    );

    let prompt = captured_prompt
        .lock()
        .expect("prompt mutex poisoned")
        .clone()
        .expect("provider should capture a prompt");
    assert!(prompt.contains("planning mode"));
    assert!(prompt.contains("structured response contract"));
    assert!(prompt.contains("User request:\n/"));
}

#[test]
fn execute_keyword_in_plan_mode_switches_to_chat_execution_prompt() {
    let captured_prompt = Arc::new(Mutex::new(None));
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime = Box::new(
        AgentRuntime::new(
            Box::new(CapturePromptProvider {
                prompt: captured_prompt.clone(),
            }),
            PathBuf::from(repo_path),
            "echo".to_owned(),
        )
        .expect("runtime should initialize"),
    );

    app.timeline.push(
        TimelineEntry::response("1. Add parser tests\n2. Fix command dispatch")
            .with_title("Plan command simplification"),
    );
    app.handle_action(InputAction::ToggleMode);
    app.set_composer_text("执行");
    app.submit_composer();

    let captured = wait_until(TEST_WAIT_TIMEOUT, || {
        app.tick();
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_some()
    });
    assert!(captured, "execution prompt was not captured within timeout");

    let prompt = captured_prompt
        .lock()
        .expect("prompt mutex poisoned")
        .clone()
        .expect("provider should capture a prompt");

    assert!(
        !prompt.contains("planning mode"),
        "execution keyword should switch out of planning prompt path"
    );
    assert!(
        prompt.contains("Execute the approved plan"),
        "execution prompt should include explicit execution instruction"
    );
    assert!(
        prompt.contains("Output must follow the structured response contract"),
        "execution prompt should include structured response contract requirement"
    );
    assert!(
        prompt.contains("Plan command simplification"),
        "execution prompt should include latest plan title context"
    );
    assert_eq!(
        app.users_status_snapshot().mode,
        UserMode::Auto,
        "execution keyword should switch mode back to AUTO"
    );
}

#[test]
fn execute_keyword_in_plan_mode_dispatches_after_busy_turn_drains() {
    let captured_prompt = Arc::new(Mutex::new(None));
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime = Box::new(
        AgentRuntime::new(
            Box::new(CapturePromptProvider {
                prompt: captured_prompt.clone(),
            }),
            PathBuf::from(repo_path),
            "echo".to_owned(),
        )
        .expect("runtime should initialize"),
    );

    app.timeline.push(
        TimelineEntry::response("1. Implement parser fallback")
            .with_title("Plan execution handoff"),
    );
    app.handle_action(InputAction::ToggleMode);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });

    app.set_composer_text("执行");
    app.submit_composer();
    assert!(
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_none(),
        "execution prompt should stay queued while runtime is busy"
    );

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    let captured = wait_until(TEST_WAIT_TIMEOUT, || {
        app.tick();
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_some()
    });
    assert!(
        captured,
        "queued execution prompt should dispatch once busy turn completes"
    );
}

#[test]
fn actionable_plan_turn_auto_executes_without_manual_execute_keyword() {
    let fake = FakeRuntime::with_events(vec![]);
    let probe = fake.clone();
    let mut app = ChatApp::new_with_runtime_for_test(
        env!("CARGO_MANIFEST_DIR"),
        Box::new(fake),
        "echo",
    )
    .expect("init");
    app.user_mode = UserMode::Plan;
    app.active_turn_kind = Some(TurnKind::Plan);
    app.timeline.push(TimelineEntry::response(
        r#"<assistant_response>
<intent>plan</intent>
<summary>Plan command simplification</summary>
<tool_summary>Checked parser scope.</tool_summary>
<nextstep>1. Add parser tests
2. Refactor structured stream handling</nextstep>
<needs_user_input>false</needs_user_input>
<question></question>
<status>ok</status>
<error></error>
</assistant_response>"#,
    ));

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let prompts = probe.submitted_prompts();
    assert_eq!(prompts.len(), 1, "plan completion should auto-dispatch execute turn");
    assert!(prompts[0].contains("Execute the approved plan"));
    assert!(prompts[0].contains("Plan command simplification"));
    assert_eq!(
        app.users_status_snapshot().mode,
        UserMode::Auto,
        "auto-execution should switch mode back to AUTO"
    );
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.body.contains("Auto-executing the approved steps")),
        "timeline should show explicit auto-execution action"
    );
}

#[test]
fn blocked_plan_turn_does_not_auto_execute() {
    let fake = FakeRuntime::with_events(vec![]);
    let probe = fake.clone();
    let mut app = ChatApp::new_with_runtime_for_test(
        env!("CARGO_MANIFEST_DIR"),
        Box::new(fake),
        "echo",
    )
    .expect("init");
    app.user_mode = UserMode::Plan;
    app.active_turn_kind = Some(TurnKind::Plan);
    app.timeline.push(TimelineEntry::response(
        r#"<assistant_response>
<intent>clarify</intent>
<summary>Need a model choice first.</summary>
<tool_summary>Checked provider config.</tool_summary>
<nextstep>1. Confirm model
2. Continue implementation</nextstep>
<needs_user_input>true</needs_user_input>
<question>Use deepseek-v4-pro or qwen3-8b?</question>
<status>blocked</status>
<error>Waiting for user decision.</error>
</assistant_response>"#,
    ));

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    assert!(
        probe.submitted_prompts().is_empty(),
        "blocked plan turns should wait for user input and must not auto-execute"
    );
    assert_eq!(app.users_status_snapshot().mode, UserMode::Plan);
}

#[test]
fn slash_query_matching_normalizes_crlf_and_whitespace() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.set_composer_text("/help\r\n   ");

    assert_eq!(app.users_status_snapshot().status_mode, StatusMode::CommandList);
    assert_eq!(app.normalized_slash_query(), "/help");
    assert!(
        app.inline_command_matches().iter().any(|name| name == "/help"),
        "CR/LF + whitespace query should still match /help after normalization"
    );
}

#[test]
fn slash_submit_uses_selected_inline_command_match() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.set_composer_text("/");

    assert_eq!(app.users_status_snapshot().status_mode, StatusMode::CommandList);
    assert_eq!(app.users_command_scroll_offset(), 0);
    let help_index = app
        .inline_command_matches()
        .iter()
        .position(|name| name == "/help")
        .expect("/help command should be available");
    for _ in 0..help_index {
        app.scroll_users_command_window(1);
    }

    app.submit_composer();

    assert!(
        app.timeline.iter().any(|entry| {
            entry.body
                .contains("Command reference and keyboard shortcuts available")
        }),
        "submitting a slash query should dispatch the currently selected command"
    );
}

#[test]
fn status_label_uses_action_name_only_for_thinking() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t1".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: explore runtime flow".into(),
    });

    assert_eq!(app.status_label(), "explore");
}

#[test]
fn status_label_keeps_executing_label_unchanged() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t2".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: explore runtime flow".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"echo hi"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });

    assert_eq!(app.status_label(), "executing bash");
}

#[test]
fn status_label_clears_thinking_action_name_between_turns() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-stale-1".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: explore runtime flow".into(),
    });
    assert_eq!(app.status_label(), "explore");

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-stale-2".into(),
    });

    assert_eq!(app.status_label(), "thinking");

    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: inspect failure path".into(),
    });
    assert_eq!(app.status_label(), "inspect");

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderTransient,
        error: "network".into(),
    });

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-stale-3".into(),
    });

    assert_eq!(app.status_label(), "thinking");
}

#[test]
fn thinking_delta_keeps_explicit_name_precedence_across_chunks() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-explicit".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "next_step_name: inspect\nI will ".into(),
    });
    assert_eq!(app.status_label(), "inspect");

    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "explore runtime flow".into(),
    });

    assert_eq!(app.status_label(), "inspect");
}

#[test]
fn status_label_transitions_across_action_states() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    assert_eq!(app.status_label(), "ready");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-1".into(),
    });
    assert_eq!(app.status_label(), "thinking");

    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "next_step_name: explore\nI will inspect the parser flow.".into(),
    });
    assert_eq!(app.status_label(), "explore");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"echo hi"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    assert_eq!(app.status_label(), "executing bash");

    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "done".into(),
        is_error: false,
    });
    assert_eq!(app.status_label(), "explore");

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderTransient,
        error: "network".into(),
    });
    assert_eq!(app.status_label(), "error: network");

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.status_label(), "ready");

    app.active_turn_kind = Some(TurnKind::Plan);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "plan-1".into(),
    });
    assert_eq!(app.status_label(), "planning");
}

#[test]
fn submit_composer_sends_message_and_clears_input() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.set_composer_text("hello from composer");
    app.submit_composer();

    assert_eq!(app.composer_text(), "");
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body.contains("hello from composer"))
    );
}

#[test]
fn discover_agents_command_adds_hint_without_starting_turn() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("/discover-agents");

    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/discover-agents")
    );
    let hint = app.timeline.last().expect("discover hint should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(
        hint.body.contains("Discovering ACP agents"),
        "expected immediate discover-agents hint, got {:?}",
        hint.body
    );
    assert!(
        !app.is_streaming(),
        "discover-agents should not start a chat turn"
    );
}

#[test]
fn picker_discover_agents_command_adds_hint() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "discover-agents".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }

    app.handle_action(InputAction::Submit);

    let hint = app.timeline.last().expect("discover hint should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Discovering ACP agents"));
}

#[test]
fn picker_model_command_opens_model_subpicker_context() {
    use crate::chat::picker::PickerContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "model".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);

    assert_eq!(app.picker.context, PickerContext::ModelSelect);
    assert!(app.picker.is_open());
    assert_eq!(app.picker.title, "Select Model");
}

#[test]
fn quit_during_streaming_requests_cancel_without_exiting() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "streaming-turn".into(),
    });
    assert!(app.is_streaming());

    app.handle_action(InputAction::Quit);
    assert!(!app.should_quit());
}

#[test]
fn set_model_with_valid_config_switches_runtime_and_adds_hint() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let unique = format!(
        "chat-app-model-success-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let repo_path = base.join(unique);
    std::fs::create_dir_all(repo_path.join("config")).expect("create config dir");
    std::fs::write(
        repo_path.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B"
}"#,
    )
    .expect("write provider config");

    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_for_test(&repo_str).expect("test app should initialize");
    app.set_model("Qwen/Qwen3-32B");

    assert_eq!(app.model_name(), "Qwen/Qwen3-32B");
    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Model switched"));

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn private_text_helpers_handle_unicode_and_home_paths() {
    let home = std::env::var("HOME").expect("HOME should exist");
    let inside_home = format!("{home}/projects/blazar");
    assert_eq!(shorten_home(&inside_home), "~/projects/blazar");
    assert_eq!(shorten_home("/opt/work"), "/opt/work");

    assert_eq!(preview_text("你好世界hello", 2), "你好");
    assert_eq!(
        summarize_tool_arguments(&"x".repeat(70)).chars().count(),
        60
    );
    assert_eq!(summarize_tool_output("first\nsecond"), "first");
}

#[test]
fn picker_submit_regular_command_and_navigation_actions_work() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.picker.open();

    let selected_before = app.picker.selected_index();
    app.handle_action(InputAction::ScrollDown);
    app.handle_action(InputAction::PickerDown);
    app.handle_action(InputAction::ScrollUp);
    app.handle_action(InputAction::PickerUp);
    assert_eq!(app.picker.selected_index(), selected_before);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert!(app.picker.filter.is_empty());

    for ch in "help".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.body.contains("Command reference"))
    );
}

#[test]
fn picker_model_selection_trims_checkmark_suffix_before_switch() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let unique = format!(
        "chat-app-model-trim-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let repo_path = base.join(unique);
    std::fs::create_dir_all(repo_path.join("config")).expect("create config dir");
    std::fs::write(
        repo_path.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B"
}"#,
    )
    .expect("write provider config");

    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_for_test(&repo_str).expect("test app should initialize");
    app.model_name = "Qwen/Qwen3-8B".into();
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "model".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit); // open model picker
    app.handle_action(InputAction::Submit); // choose first model item (contains ✓ suffix)

    assert_eq!(app.model_name(), "Qwen/Qwen3-8B");
    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(!hint.body.contains('✓'));

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn execute_palette_command_sync_provides_tokio_runtime_outside_tokio_context() {
    let mut app = new_test_app();
    register_runtime_probe_command(&mut app, "/runtime-probe-sync");

    let result = app.execute_palette_command_sync("/runtime-probe-sync", serde_json::json!({}));

    assert!(result.is_ok(), "runtime probe should execute successfully");
    assert_eq!(app.composer_text(), "/runtime-ok");
}

#[tokio::test(flavor = "current_thread")]
async fn execute_palette_command_sync_is_safe_inside_tokio_runtime() {
    let mut app = new_test_app();
    register_runtime_probe_command(&mut app, "/runtime-probe-async");

    let result = app.execute_palette_command_sync("/runtime-probe-async", serde_json::json!({}));

    assert!(result.is_ok(), "runtime probe should not panic in tokio runtime");
    assert_eq!(app.composer_text(), "/runtime-ok");
}

#[test]
fn input_backspace_and_toggle_details_actions_update_local_state() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Backspace);
    assert_eq!(app.composer_text(), "a");

    assert!(!app.show_details());
    app.handle_action(InputAction::ToggleDetails);
    assert!(app.show_details());
}

#[test]
fn event_handlers_append_to_existing_thinking_and_response_entries() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.clear();
    app.timeline.push(TimelineEntry::thinking("seed"));

    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: " +more".into(),
    });
    assert_eq!(app.timeline.len(), 1);
    assert!(app.timeline[0].body.contains("seed +more"));
    assert!(app.timeline[0].details.contains(" +more"));

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "answer".into(),
    });
    assert_eq!(app.timeline.len(), 2);
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: " tail".into(),
    });
    assert!(app.timeline[1].body.ends_with(" tail"));
}

#[test]
fn tool_call_completed_without_matching_start_is_ignored_safely() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let initial_len = app.timeline.len();
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "missing".into(),
        output: "ignored".into(),
        is_error: false,
    });
    assert_eq!(app.timeline.len(), initial_len);
}

#[test]
fn tool_events_log_include_turn_and_tool_name() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-tool-observe".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
            call_id: "call-log".into(),
            tool_name: "bash".into(),
            kind: crate::agent::tools::ToolKind::Local,
            arguments: r#"{"command":"echo hi"}"#.into(),
            batch_id: 0,
            replay_index: 0,
            normalized_claims: Vec::new(),
        });
    });

    let tool_started = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("tool_call_started")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-tool-observe")
                && event
                    .get("tool_name")
                    .and_then(serde_json::Value::as_str)
                    == Some("bash")
        })
        .expect("expected structured tool_call_started event with turn_id+tool_name");

    assert!(
        tool_started
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|message| message.contains("tool=bash")),
        "tool event log message should include tool identifier for auditability"
    );
    assert!(
        tool_started
            .get("turn_id")
            .and_then(serde_json::Value::as_str)
            == Some("turn-tool-observe"),
        "tool event logs should include turn_id"
    );
    assert!(
        tool_started
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            == Some("bash"),
        "tool event logs should include turn_id and tool_name fields"
    );
    assert_eq!(
        tool_started
            .get("call_id")
            .and_then(serde_json::Value::as_str),
        Some("call-log")
    );
    assert_eq!(
        tool_started
            .get("event_seq")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        tool_started
            .get("queue_depth")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

#[test]
fn tool_call_completed_logs_include_turn_and_resolved_tool_name() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-tool-complete".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
            call_id: "call-complete-log".into(),
            tool_name: "bash".into(),
            kind: crate::agent::tools::ToolKind::Local,
            arguments: r#"{"command":"echo hi"}"#.into(),
            batch_id: 0,
            replay_index: 0,
            normalized_claims: Vec::new(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
            call_id: "call-complete-log".into(),
            output: "ok".into(),
            is_error: false,
        });
    });

    let tool_completed = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("tool_call_completed")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-tool-complete")
                && event
                    .get("tool_name")
                    .and_then(serde_json::Value::as_str)
                    == Some("bash")
        })
        .expect("expected structured tool_call_completed event with turn_id+tool_name");

    assert_eq!(
        tool_completed
            .get("turn_id")
            .and_then(serde_json::Value::as_str),
        Some("turn-tool-complete"),
    );
    assert_eq!(
        tool_completed
            .get("tool_name")
            .and_then(serde_json::Value::as_str),
        Some("bash"),
    );
    assert_eq!(
        tool_completed
            .get("call_id")
            .and_then(serde_json::Value::as_str),
        Some("call-complete-log")
    );
    assert_eq!(
        tool_completed
            .get("event_seq")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
}

#[test]
fn errored_tool_call_completed_logs_include_error_kind() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-tool-error-kind".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
            call_id: "call-error-kind".into(),
            tool_name: "bash".into(),
            kind: crate::agent::tools::ToolKind::Local,
            arguments: r#"{"command":"exit 1"}"#.into(),
            batch_id: 0,
            replay_index: 0,
            normalized_claims: Vec::new(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
            call_id: "call-error-kind".into(),
            output: "exit status 1".into(),
            is_error: true,
        });
    });

    let tool_completed = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("tool_call_completed")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-tool-error-kind")
                && event
                    .get("tool_name")
                    .and_then(serde_json::Value::as_str)
                    == Some("bash")
        })
        .expect("expected errored tool_call_completed event");

    assert_eq!(
        tool_completed
            .get("error_kind")
            .and_then(serde_json::Value::as_str),
        Some("ToolExecution"),
        "errored tool completion logs should include error_kind for diagnostics"
    );
}

#[test]
fn turn_failure_warning_details_include_debug_evidence_bundle() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-debug-bundle".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-debug-bundle".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"cargo test"}"#.into(),
        batch_id: 0,
        replay_index: 2,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-debug-bundle".into(),
        output: "failed".into(),
        is_error: true,
    });
    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderTransient,
        error: "network timeout".into(),
    });

    let warning = app.timeline.last().expect("warning entry should exist");
    assert!(matches!(warning.kind, EntryKind::Warning));
    assert!(warning.details.contains("debug evidence"));
    assert!(warning.details.contains("turn_id=turn-debug-bundle"));
    assert!(warning.details.contains("tool_call_started"));
    assert!(warning.details.contains("tool_call_completed"));
    assert!(warning.details.contains("turn_failed"));
}

#[test]
fn render_status_bar_includes_compact_debug_summary() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-debug-status".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-debug-status".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"cargo test"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });

    let rendered = render_to_lines_for_test(&mut app, 120, 30).join("\n");

    assert!(rendered.contains("dbg"));
    assert!(rendered.contains("turn-debug-status"));
    assert!(rendered.contains("call-debug-status"));
    assert!(rendered.contains("q0"));
}

#[test]
fn render_status_bar_shows_action_first_labels() {
    let mut app = new_test_app();
    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-action-labels".into(),
    });

    let thinking = render_to_lines_for_test(&mut app, 120, 30).join("\n");
    assert!(thinking.contains("thinking"));

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-action-labels".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"echo hi"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });

    let executing = render_to_lines_for_test(&mut app, 120, 30).join("\n");
    assert!(executing.contains("executing bash"));

    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-action-labels".into(),
        output: "ok".into(),
        is_error: false,
    });

    let resumed = render_to_lines_for_test(&mut app, 120, 30).join("\n");
    assert!(resumed.contains("thinking"));
    assert!(!resumed.contains("streaming…"));
}

#[test]
fn picker_commands_context_supports_non_slash_labels() {
    use crate::chat::picker::{ModalPicker, PickerContext, PickerItem};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.picker = ModalPicker::with_context(
        "Commands",
        vec![PickerItem::new("custom command", "desc")],
        PickerContext::Commands,
    );
    app.picker.open();

    app.handle_action(InputAction::Submit);
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "custom command")
    );
}

#[test]
fn tool_call_completed_sets_error_status_and_summary() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-err".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"exit 1"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-err".into(),
        output: "first line\nsecond line".into(),
        is_error: true,
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(matches!(
        entry.kind,
        EntryKind::ToolCall {
            status: ToolCallStatus::Error,
            ..
        }
    ));
    assert_eq!(entry.body, "first line");
    assert!(entry.details.contains("second line"));
}

#[test]
fn agent_tool_calls_render_acp_badge() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-acp".into(),
        tool_name: "configured_reviewer".into(),
        kind: crate::agent::tools::ToolKind::Agent { is_acp: true },
        arguments: r#"{"prompt":"review the patch"}"#.into(),
        batch_id: 0,
        replay_index: 2,
        normalized_claims: Vec::new(),
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(matches!(
        entry.kind,
        EntryKind::ToolCall {
            kind: crate::agent::tools::ToolKind::Agent { is_acp: true },
            ..
        }
    ));

    let rendered = render_to_lines_for_test(&mut app, 100, 35).join("\n");
    assert!(rendered.contains("configured_reviewer"));
    assert!(rendered.contains("(ACP)"));
}

#[test]
fn chat_timeline_preserves_capability_metadata_for_tool_entries() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-meta".into(),
        tool_name: "write_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"src/lib.rs"}"#.into(),
        batch_id: 7,
        replay_index: 2,
        normalized_claims: vec![
            "fs:src/lib.rs#read-write".into(),
            "process:bash#exclusive".into(),
        ],
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(entry.details.contains(r#"{"path":"src/lib.rs"}"#));
    assert!(entry.details.contains("batch_id=7"));
    assert!(entry.details.contains("replay_index=2"));
    assert!(
        entry
            .details
            .contains("normalized_claims=fs:src/lib.rs#read-write,process:bash#exclusive")
    );
}

#[test]
fn chat_timeline_renders_sorted_claims_for_stable_metadata() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-meta-unstable".into(),
        tool_name: "write_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"src/lib.rs"}"#.into(),
        batch_id: 8,
        replay_index: 1,
        normalized_claims: vec![
            "process:bash#exclusive".into(),
            "fs:src/lib.rs#read-write".into(),
        ],
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(
        entry
            .details
            .contains("normalized_claims=fs:src/lib.rs#read-write,process:bash#exclusive")
    );
}

#[test]
fn turn_complete_plan_with_non_extractable_body_leaves_entry_untitled() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline
        .push(TimelineEntry::response("###").with_title("Existing"));
    app.active_turn_kind = Some(TurnKind::Plan);
    app.active_turn_title = Some("Temp title".into());

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.title.as_deref(), Some("Existing"));
    assert_eq!(last.body, "###");
    assert!(app.active_turn_kind.is_none());
    assert!(app.active_turn_title.is_none());
}

#[test]
fn turn_complete_unwraps_structured_response_for_chat_entries() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(TimelineEntry::response(
        r#"<assistant_response>
<intent>clarify</intent>
<summary>Need one environment choice.</summary>
<tool_summary>Read workspace config.</tool_summary>
<nextstep>Confirm which provider model to use.</nextstep>
<needs_user_input>true</needs_user_input>
<question>Use deepseek-v4-pro or qwen3-8b?</question>
<status>blocked</status>
<error>Waiting for user confirmation.</error>
</assistant_response>"#,
    ));
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(!last.body.contains("<assistant_response>"));
    assert!(last.body.contains("Need one environment choice."));
    assert!(!last.body.contains("Read workspace config."));
    assert!(!last.body.contains("Confirm which provider model to use."));
    assert!(last.body.contains("Question: Use deepseek-v4-pro or qwen3-8b?"));
    assert!(last.body.contains("Blocked reason: Waiting for user confirmation."));
    assert!(last.details.contains("intent=clarify"));
    assert!(last.details.contains("status=blocked"));
}

#[test]
fn turn_complete_unwraps_structured_response_for_plan_entries() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(TimelineEntry::response(
        r#"<assistant_response>
<intent>plan</intent>
<summary>Plan command simplification</summary>
<tool_summary></tool_summary>
<nextstep>1. Add parser tests
2. Handle structured response rendering</nextstep>
<needs_user_input>false</needs_user_input>
<question></question>
<status>ok</status>
<error></error>
</assistant_response>"#,
    ));
    app.active_turn_kind = Some(TurnKind::Plan);

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.title.as_deref(), Some("Plan command simplification"));
    assert_eq!(
        last.body,
        "1. Add parser tests\n2. Handle structured response rendering"
    );
    assert!(!last.body.contains("<assistant_response>"));
}

#[test]
fn turn_complete_unwraps_latest_structured_response_when_retries_are_concatenated() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(TimelineEntry::response(
        r#"Interim stream text that should be discarded.
<assistant_response>
<intent>clarify</intent>
<summary>Bad contract summary</summary>
<tool_summary>bad</tool_summary>
<nextstep>bad</nextstep>
<needs_user_input>maybe</needs_user_input>
<question></question>
<status>ok</status>
<error></error>
</assistant_response>
<assistant_response>
<intent>execute</intent>
<summary>Applied maintainability refactor.</summary>
<tool_summary>Updated events parser and tests.</tool_summary>
<nextstep>Run full validation and continue implementation.</nextstep>
<needs_user_input>false</needs_user_input>
<question></question>
<status>ok</status>
<error></error>
</assistant_response>"#,
    ));
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert!(!last.body.contains("<assistant_response>"));
    assert!(last.body.contains("Applied maintainability refactor."));
    assert!(!last.body.contains("Updated events parser and tests."));
    assert!(!last.body.contains("Run full validation and continue implementation."));
    assert!(!last.body.contains("Bad contract summary"));
    assert!(last.details.contains("intent=execute"));
    assert!(last.details.contains("status=ok"));
}

#[test]
fn turn_complete_unwraps_structured_response_split_across_assistant_chunks() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response>\n<intent>execute</intent>\n<summary>Split response normalized.</summary>\n<tool_summary>Updated parser.</tool_summary>\n<nextstep>Run full validation.</nextstep>\n".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "thinking...".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<needs_user_input>false</needs_user_input>\n<question></question>\n<status>ok</status>\n<error></error>\n</assistant_response>".into(),
    });

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let assistant_messages = app
        .timeline
        .iter()
        .filter(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
        .collect::<Vec<_>>();
    assert_eq!(
        assistant_messages.len(),
        1,
        "turn should collapse split assistant chunks into one message entry"
    );

    let last = assistant_messages.last().expect("assistant entry should exist");
    assert!(!last.body.contains("<assistant_response>"));
    assert!(last.body.contains("Split response normalized."));
    assert!(!last.body.contains("Updated parser."));
    assert!(!last.body.contains("Run full validation."));
}

#[test]
fn text_delta_streaming_parses_structured_response_before_turn_complete() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-stream-xml".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><intent>execute</intent><summary>Streaming summary.</summary>"
            .into(),
    });

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(!last.body.contains("<assistant_response>"));
    assert!(
        last.body.contains("Streaming summary."),
        "summary should be rendered before TurnComplete"
    );
}

#[test]
fn text_delta_structured_stream_keeps_single_assistant_entry_across_thinking_chunks() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-stream-stable".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><intent>execute</intent><summary>Stable stream.</summary>"
            .into(),
    });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "thinking...".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<tool_summary>Updated parser.</tool_summary><nextstep>Continue.</nextstep><needs_user_input>false</needs_user_input><question></question><status>ok</status><error></error></assistant_response>".into(),
    });

    let assistant_messages = app
        .timeline
        .iter()
        .filter(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
        .collect::<Vec<_>>();
    assert_eq!(
        assistant_messages.len(),
        1,
        "structured streaming should keep a single assistant message entry while chunks arrive"
    );
    let last = assistant_messages.last().expect("assistant entry should exist");
    assert!(last.body.contains("Stable stream."));
    assert!(!last.body.contains("Updated parser."));
}

#[test]
fn text_delta_structured_stream_renders_partial_summary_without_closing_tag() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-partial-summary".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><intent>execute</intent><summary>Streaming summary in progress"
            .into(),
    });

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(last.body.contains("Streaming summary in progress"));
    assert!(!last.body.contains('<'));
}

#[test]
fn contract_delta_side_channel_suppresses_markup_text_deltas() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-contract-side-channel".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::AssistantContractDelta {
        delta: crate::agent::protocol::AssistantContractDelta {
            intent: Some("execute".into()),
            summary: Some("Contract side-channel summary.".into()),
            ..Default::default()
        },
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><summary>leaked markup</summary></assistant_response>".into(),
    });

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(last.body.contains("Contract side-channel summary."));
    assert!(!last.body.contains("<assistant_response>"));
    assert!(!last.body.contains("leaked markup"));
}

#[test]
fn turn_complete_prefers_side_channel_contract_over_conflicting_raw_markup() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-side-channel-precedence".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::AssistantContractDelta {
        delta: crate::agent::protocol::AssistantContractDelta {
            intent: Some("execute".into()),
            summary: Some("Summary from side channel.".into()),
            tool_summary: Some("Tool summary from side channel.".into()),
            nextstep: Some("Next step from side channel.".into()),
            needs_user_input: Some(false),
            status: Some("ok".into()),
            complete: true,
            ..Default::default()
        },
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><intent>execute</intent><summary>Conflicting raw summary</summary><tool_summary>Conflicting raw tool summary</tool_summary><nextstep>Conflicting raw step</nextstep><needs_user_input>false</needs_user_input><question></question><status>ok</status><error></error></assistant_response>".into(),
    });

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert!(last.body.contains("Summary from side channel."));
    assert!(!last.body.contains("Tool summary from side channel."));
    assert!(!last.body.contains("Next step from side channel."));
    assert!(!last.body.contains("Conflicting raw summary"));
    assert!(!last.body.contains("Conflicting raw tool summary"));
    assert!(!last.body.contains("Conflicting raw step"));
}

#[test]
fn non_echo_raw_markup_uses_safety_strip_not_legacy_contract_parse() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.model_name = "gpt-5".to_owned();
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-non-echo-strip".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><summary>Raw summary</summary><tool_summary>Raw tool</tool_summary><nextstep>Raw next</nextstep></assistant_response>".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert!(!last.body.contains("<assistant_response>"));
    assert!(last.body.contains("Raw summary"));
    assert!(!last.body.contains("Raw tool"));
    assert!(!last.body.contains("Raw next"));
    assert!(!last.body.contains("Tool summary: Raw tool"));
    assert!(!last.body.contains("Next step: Raw next"));
}

#[test]
fn non_echo_plan_raw_markup_keeps_nextstep_text_without_label() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.model_name = "gpt-5".to_owned();
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-non-echo-plan-strip".into(),
    });
    app.active_turn_kind = Some(TurnKind::Plan);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><summary>Plan title</summary><tool_summary>Raw tool</tool_summary><nextstep>1. Step one\n2. Step two</nextstep></assistant_response>".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert!(last.body.contains("1. Step one"));
    assert!(last.body.contains("2. Step two"));
    assert!(!last.body.contains("Raw tool"));
    assert!(!last.body.contains("Next step:"));
    assert!(!last.body.contains('<'));
}

#[test]
fn non_echo_mixed_markup_and_plain_text_preserves_plain_text_on_turn_complete() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.model_name = "gpt-5".to_owned();
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "turn-non-echo-mixed-stream".into(),
    });
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<assistant_response><summary>Hello</summary>".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: " plain world ".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "<nextstep>Next</nextstep></assistant_response>".into(),
    });
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert!(last.body.contains("Hello"));
    assert!(last.body.contains("plain world"));
    assert!(!last.body.contains("Next"));
    assert!(!last.body.contains('<'));
}

#[test]
fn turn_complete_unwraps_lenient_structured_response_when_error_tag_missing() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(TimelineEntry::response(
        r#"<assistant_response>
<intent>clarify</intent>
<summary>Need narrower maintainability scope.</summary>
<tool_summary>Checked modified files list.</tool_summary>
<nextstep>Focus on chat/app and agent/runtime first.</nextstep>
<needs_user_input>true</needs_user_input>
<question>Should we only fix current modified files?</question>
<status>blocked</status>
</assistant_response>"#,
    ));
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(!last.body.contains("<assistant_response>"));
    assert!(last.body.contains("Need narrower maintainability scope."));
    assert!(!last.body.contains("Checked modified files list."));
    assert!(!last.body.contains("Focus on chat/app and agent/runtime first."));
    assert!(last.body.contains("Question: Should we only fix current modified files?"));
    assert!(last.details.contains("status=blocked"));
}

#[test]
fn turn_complete_sanitizes_internal_contract_tags_when_contract_is_unparseable() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(TimelineEntry::response(
        "<assistant_response><bogus>internal note</bogus></assistant_response>",
    ));
    app.active_turn_kind = Some(TurnKind::Chat);

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.actor, Actor::Assistant);
    assert_eq!(last.kind, EntryKind::Message);
    assert!(!last.body.contains('<'));
    assert!(!last.body.contains('>'));
    assert!(last.body.contains("internal note"));
}

#[test]
fn build_pending_turn_parses_runtime_and_local_command_kinds() {
    let chat = super::turns::build_pending_turn("  hello ");
    assert_eq!(chat.user_text, "hello");
    assert_eq!(
        chat.dispatch,
        super::PendingDispatch::Runtime {
            runtime_prompt: "hello".to_owned(),
            kind: TurnKind::Chat,
        }
    );
    assert!(!chat.timeline_inserted);

    let plan_command_text = super::turns::build_pending_turn("/plan   improve tests");
    match &plan_command_text.dispatch {
        super::PendingDispatch::Runtime {
            runtime_prompt,
            kind,
        } => {
            assert_eq!(*kind, TurnKind::Chat);
            assert_eq!(runtime_prompt, "/plan   improve tests");
        }
        other => panic!("expected runtime dispatch, got {other:?}"),
    }
    assert!(!plan_command_text.timeline_inserted);

    let discover = super::turns::build_pending_turn("/discover-agents");
    assert_eq!(discover.user_text, "/discover-agents");
    assert_eq!(discover.dispatch, super::PendingDispatch::DiscoverAgents);
    assert!(!discover.timeline_inserted);
}

#[test]
fn composer_plan_continue_with_review_flag_maps_to_command_args() {
    let registry =
        crate::chat::commands::CommandRegistry::with_builtins().expect("bootstrap built-ins");
    let (name, args) =
        parse_composer_command_dispatch("/plan --continue plan-123 --review revise", &registry)
            .expect("composer parser should dispatch /plan continuation");

    assert_eq!(name, "/plan");
    assert_eq!(
        args.get("continue_id").and_then(serde_json::Value::as_str),
        Some("plan-123")
    );
    assert_eq!(
        args.get("review").and_then(serde_json::Value::as_str),
        Some("revise")
    );
    assert!(
        args.get("goal").is_none(),
        "review continuation should not fabricate a goal argument"
    );
}

#[test]
fn composer_exact_plan_maps_to_empty_command_args() {
    let registry =
        crate::chat::commands::CommandRegistry::with_builtins().expect("bootstrap built-ins");
    let (name, args) = parse_composer_command_dispatch("/plan", &registry)
        .expect("exact /plan should dispatch command path");

    assert_eq!(name, "/plan");
    assert_eq!(args, serde_json::json!({}));
}

#[test]
fn composer_plan_continue_with_goal_flag_maps_to_command_args() {
    let registry =
        crate::chat::commands::CommandRegistry::with_builtins().expect("bootstrap built-ins");
    let (name, args) = parse_composer_command_dispatch(
        "/plan --continue plan-456 --goal finish composer dispatch",
        &registry,
    )
    .expect("composer parser should dispatch /plan continuation");

    assert_eq!(name, "/plan");
    assert_eq!(
        args.get("continue_id").and_then(serde_json::Value::as_str),
        Some("plan-456")
    );
    assert_eq!(
        args.get("goal").and_then(serde_json::Value::as_str),
        Some("finish composer dispatch")
    );
    assert!(
        args.get("review").is_none(),
        "goal continuation should not fabricate a review argument"
    );
}

#[test]
fn agent_runtime_implements_runtime_port_trait() {
    fn assert_impl<T: crate::chat::runtime_port::AgentRuntimePort>() {}
    // compile-time check that AgentRuntime implements the runtime port trait
    assert_impl::<crate::agent::runtime::AgentRuntime>();
}

// ── model_metadata coverage: stale handles, timeout, mismatched request id ──

#[test]
fn schedule_refresh_skips_when_already_in_flight() {
    let repo_path = create_provider_config_repo(
        r#"{ "api_key": "k", "base_url": "https://example.com/v1", "model": "m", "max_tokens": 100 }"#,
        "meta-in-flight",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");

    // Force in_flight = true, then call on_model_changed which calls schedule_refresh.
    app.model_metadata.set_in_flight_for_test(true);
    let id_before = app.model_metadata.active_request_id_for_test();
    app.model_metadata
        .on_model_changed(std::path::Path::new(&repo_str), "m");
    // active_request_id should NOT have changed because schedule_refresh returned early.
    assert_eq!(
        app.model_metadata.active_request_id_for_test(),
        id_before,
        "schedule_refresh must early-return when in_flight is true"
    );

    std::fs::remove_dir_all(&repo_path).ok();
}

#[test]
fn schedule_refresh_clears_finished_stalled_handle() {
    let repo_path = create_provider_config_repo(
        r#"{ "api_key": "k", "base_url": "https://example.com/v1", "model": "m", "max_tokens": 100 }"#,
        "meta-stalled-finished",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");

    // Install a finished stalled handle and mark retry_exhausted.
    app.model_metadata.set_stalled_handle_finished_for_test();
    app.model_metadata.retry_exhausted = true;
    assert!(app.model_metadata.has_stalled_handle_for_test());

    // Calling schedule_refresh (via on_model_changed) should detect the finished
    // stalled handle, clear it, and reset retry_exhausted.
    app.model_metadata
        .on_model_changed(std::path::Path::new(&repo_str), "m");

    assert!(
        !app.model_metadata.has_stalled_handle_for_test(),
        "finished stalled handle should have been cleared"
    );
    assert!(
        !app.model_metadata.retry_exhausted,
        "retry_exhausted should be reset after stalled handle finishes"
    );

    std::fs::remove_dir_all(&repo_path).ok();
}

#[test]
fn schedule_refresh_returns_early_when_retry_exhausted_and_stalled() {
    let repo_path = create_provider_config_repo(
        r#"{ "api_key": "k", "base_url": "https://example.com/v1", "model": "m", "max_tokens": 100 }"#,
        "meta-retry-exhausted",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");

    // Install a still-running stalled handle and mark retry_exhausted.
    app.model_metadata.set_stalled_handle_running_for_test();
    app.model_metadata.retry_exhausted = true;

    let id_before = app.model_metadata.active_request_id_for_test();
    app.model_metadata
        .on_model_changed(std::path::Path::new(&repo_str), "m");
    assert_eq!(
        app.model_metadata.active_request_id_for_test(),
        id_before,
        "schedule_refresh must skip when retry_exhausted and stalled handle still running"
    );

    std::fs::remove_dir_all(&repo_path).ok();
}

#[test]
fn expire_stale_refresh_sets_retry_exhausted_when_two_stalled() {
    let repo_path = create_provider_config_repo(
        r#"{ "api_key": "k", "base_url": "https://example.com/v1", "model": "m", "max_tokens": 100 }"#,
        "meta-double-stall",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");

    // First: install a running stalled handle (simulates a previous timeout).
    app.model_metadata.set_stalled_handle_running_for_test();
    // Second: force current refresh into timed-out state with its own running handle.
    app.model_metadata.force_timeout_for_test();

    let ws = std::path::Path::new(&repo_str);
    // tick → expire_stale_refresh should see both handles running → set retry_exhausted.
    app.model_metadata.tick(ws, "m");

    assert!(
        app.model_metadata.retry_exhausted,
        "retry_exhausted must be true when two stalled handles are running"
    );
    assert!(
        !app.model_metadata.in_flight_for_test(),
        "in_flight should be cleared after expiration"
    );

    std::fs::remove_dir_all(&repo_path).ok();
}

#[test]
fn apply_completed_skips_mismatched_request_id() {
    let repo_path = create_provider_config_repo(
        r#"{ "api_key": "k", "base_url": "https://example.com/v1", "model": "m", "max_tokens": 100 }"#,
        "meta-mismatch-id",
    );
    let fake = FakeRuntime::with_events(vec![]);
    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_with_runtime_for_test(&repo_str, Box::new(fake), "echo")
        .expect("init");

    // Inject a result with a request_id that doesn't match the active one.
    let current_id = app.model_metadata.active_request_id_for_test();
    app.model_metadata
        .inject_result_for_test(current_id.wrapping_add(42));

    // tick should discard the mismatched result — available_models stays empty.
    let ws = std::path::Path::new(&repo_str);
    app.model_metadata.tick(ws, "m");

    assert!(
        app.model_metadata.available_models.is_empty(),
        "result with wrong request_id must be skipped"
    );

    std::fs::remove_dir_all(&repo_path).ok();
}

#[test]
fn turn_failed_app_log_message_covers_all_error_kinds() {
    use crate::agent::runtime::RuntimeErrorKind;

    let msg = super::events::turn_failed_app_log_message(
        RuntimeErrorKind::ProtocolInvalidPayload,
        Some("t1"),
    );
    assert!(msg.contains("ProtocolInvalidPayload"));

    let msg = super::events::turn_failed_app_log_message(RuntimeErrorKind::ToolExecution, None);
    assert!(msg.contains("ToolExecution"));

    let msg =
        super::events::turn_failed_app_log_message(RuntimeErrorKind::Cancelled, Some("t2"));
    assert!(msg.contains("Cancelled"));
}

#[test]
fn set_context_usage_for_test_stores_values() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    assert!(app.context_usage.is_none());
    app.set_context_usage_for_test(500, 4096);
    let usage = app.context_usage.expect("usage should be set");
    assert_eq!(usage.used_tokens, 500);
    assert_eq!(usage.max_tokens, 4096);
}

#[test]
fn request_quit_sets_should_quit_flag() {
    let mut app = new_test_app();
    assert!(!app.should_quit());
    app.request_quit();
    assert!(app.should_quit());
}

#[test]
fn clear_conversation_resets_messages_and_keeps_banner() {
    let mut app = new_test_app();
    
    // Send a message to populate messages and timeline
    app.send_message("test message");
    app.tick();
    
    assert!(!app.messages().is_empty(), "messages should be populated");
    assert!(app.has_user_sent(), "has_user_sent should be true");
    
    let banner_count_before = app.timeline().iter()
        .filter(|e| matches!(e.kind, crate::chat::model::EntryKind::Banner))
        .count();
    
    let total_entries_before = app.timeline().len();
    assert!(total_entries_before > banner_count_before, "should have non-banner entries");
    
    // Clear conversation
    app.clear_conversation();
    
    assert!(app.messages().is_empty(), "messages should be cleared");
    assert!(!app.has_user_sent(), "has_user_sent should be reset");
    assert_eq!(app.scroll_offset(), u16::MAX, "scroll should be at sentinel");
    
    let banner_count_after = app.timeline().iter()
        .filter(|e| matches!(e.kind, crate::chat::model::EntryKind::Banner))
        .count();
    
    assert_eq!(banner_count_after, banner_count_before, "banners should be preserved");
    assert_eq!(app.timeline().len(), banner_count_before, "only banners should remain");
}

#[test]
fn push_system_hint_adds_hint_and_scrolls() {
    let mut app = new_test_app();
    
    let initial_count = app.timeline().len();
    
    app.push_system_hint("test hint message");
    
    assert_eq!(app.timeline().len(), initial_count + 1, "should add one entry");
    assert_eq!(app.scroll_offset(), u16::MAX, "should auto-scroll");
    
    let last = app.timeline().last().expect("should have entries");
    assert!(matches!(last.kind, crate::chat::model::EntryKind::Hint));
    assert_eq!(last.body, "test hint message");
    assert_eq!(last.actor, crate::chat::model::Actor::System);
}

#[test]
fn workspace_root_returns_path() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    
    let workspace = app.workspace_root();
    assert_eq!(workspace, std::path::Path::new(repo_path));
}

#[test]
fn last_assistant_message_returns_none_when_no_messages() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    
    assert!(app.last_assistant_message().is_none());
}

#[test]
fn last_assistant_message_returns_most_recent() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    
    // Add some timeline entries
    app.timeline.push(crate::chat::model::TimelineEntry::response("first response"));
    app.timeline.push(crate::chat::model::TimelineEntry::hint("system hint"));
    app.timeline.push(crate::chat::model::TimelineEntry::response("second response"));
    
    let last = app.last_assistant_message();
    assert_eq!(last, Some("second response".to_string()));
}

#[test]
fn export_conversation_json_includes_key_fields() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    
    // Add a message
    app.messages.push(crate::chat::model::ChatMessage {
        author: crate::chat::model::Author::User,
        body: "test user message".to_string(),
    });
    app.messages.push(crate::chat::model::ChatMessage {
        author: crate::chat::model::Author::Spirit,
        body: "test assistant message".to_string(),
    });
    
    let json = app.export_conversation_json();
    
    assert!(json.get("workspace").is_some());
    assert!(json.get("branch").is_some());
    assert!(json.get("model").is_some());
    assert!(json.get("messages").is_some());
    
    let messages = json["messages"].as_array().expect("messages should be array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["author"], "user");
    assert_eq!(messages[0]["body"], "test user message");
    assert_eq!(messages[1]["author"], "assistant");
    assert_eq!(messages[1]["body"], "test assistant message");
}

#[test]
fn compact_turn_finalizes_with_title_prefix() {
    use crate::agent::protocol::AgentEvent;

    let fake = FakeRuntime::with_events(vec![
        AgentEvent::TurnStarted {
            turn_id: "turn-1".into(),
        },
        AgentEvent::TextDelta {
            text: "Conversation Summary\n\nSummarized 5 messages covering workspace setup and initial configuration."
                .into(),
        },
        AgentEvent::TurnComplete,
    ]);

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_with_runtime_for_test(repo_path, Box::new(fake), "echo")
        .expect("test app should initialize");

    app.send_message("/compact");
    app.tick();
    app.tick();
    app.tick();

    let last_response = app
        .timeline()
        .iter()
        .rev()
        .find(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message);

    assert!(last_response.is_some(), "should have assistant response");
    let response = last_response.unwrap();

    assert!(response.title.is_some(), "compact should extract title");
    let title = response.title.as_ref().unwrap();
    assert!(
        title.starts_with("📦"),
        "compact title should have box emoji prefix, got: {title}"
    );
    assert!(
        title.contains("Conversation Summary"),
        "should contain extracted title, got: {title}"
    );

    assert!(
        response.body.contains("Summarized 5 messages"),
        "body should contain summary text"
    );
}

#[test]
fn compact_streaming_shows_compacting_status() {
    use crate::agent::protocol::AgentEvent;

    let fake = FakeRuntime::with_events(vec![
        AgentEvent::TurnStarted {
            turn_id: "turn-1".into(),
        },
        AgentEvent::TextDelta {
            text: "Building summary...".into(),
        },
    ]);

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_with_runtime_for_test(repo_path, Box::new(fake), "echo")
        .expect("test app should initialize");

    app.send_message("/compact");
    app.tick();
    app.tick();

    let status = app.derive_active_turn_status_label();
    assert!(
        status.contains("compacting"),
        "status should show 'compacting' while streaming, got: {status}"
    );
}
