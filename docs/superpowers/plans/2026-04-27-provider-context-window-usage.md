# Provider Context Window Usage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Populate users-panel context window from real provider usage (OpenAI-compatible + OpenRouter), with model context length priority and config fallback.

**Architecture:** Add a provider-to-UI usage event pipeline: `ProviderEvent::Usage` → runtime relay → `AgentEvent::UsageUpdated` → `ChatApp.context_usage`. Resolve max window by model metadata (`context_length`) first, then fallback to `provider.json` `max_tokens`. Keep existing UI rendering contract (`used/max (%)`, `n/a` fallback) unchanged.

**Tech Stack:** Rust, async-openai BYOT streaming, openrouter-rs streaming, ratatui UI status row, existing unit/integration tests (`cargo test`, `just fmt-check`, `just lint`, `just test`)

---

## File Structure and Responsibilities

- Modify: `src/provider/mod.rs`
  - Add usage payload type and provider usage event variant.
  - Extend `ModelInfo` with optional context length metadata.
  - Add helper(s) to resolve model context length and config fallback max tokens.
- Modify: `src/provider/openai_compat.rs`
  - Request streaming usage (`stream_options.include_usage=true`).
  - Parse usage from stream chunks and emit `ProviderEvent::Usage`.
  - Include context length (if available) in model mapping.
- Modify: `src/provider/openrouter.rs`
  - Extract usage from final stream event and emit `ProviderEvent::Usage`.
  - Include context length in model mapping.
- Modify: `src/agent/protocol.rs`
  - Add usage payload type and `AgentEvent::UsageUpdated`.
- Modify: `src/agent/runtime/events.rs`
  - Add usage callback to `TurnObserver`; relay to `AgentEvent::UsageUpdated`.
- Modify: `src/agent/runtime/turn.rs`
  - Handle `ProviderEvent::Usage` and call observer usage callback.
- Modify: `src/chat/app.rs`
  - Add helper to resolve max context tokens for active model.
- Modify: `src/chat/app/events.rs`
  - Handle `AgentEvent::UsageUpdated` and write `self.context_usage`.
- Test: `tests/unit/chat/app/tests_impl.inc`
  - Add app-level tests for usage update and max-token resolution fallback.
- Test: `tests/unit/provider/openai_compat_tests.rs`
  - Add tests for request stream options and usage extraction.
- Test: `src/provider/openrouter.rs` (existing inline `mod tests`)
  - Add usage extraction/model mapping tests.
- Test: `tests/chat_render.rs`
  - Keep/extend users status row rendering assertions for real usage values.

---

### Task 1: Add Shared Usage Contracts and App Event Handling (TDD)

**Files:**
- Modify: `src/provider/mod.rs`
- Modify: `src/agent/protocol.rs`
- Modify: `src/agent/runtime/events.rs`
- Modify: `src/agent/runtime/turn.rs`
- Modify: `src/chat/app/events.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing app-level usage event test**

```rust
#[test]
fn agent_usage_updated_event_updates_users_context_snapshot() {
    let mut app = new_test_app();
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated {
        prompt_tokens: 120,
        completion_tokens: 30,
        total_tokens: 150,
    });

    let snapshot = app.users_status_snapshot();
    assert_eq!(
        snapshot.context_usage,
        Some(ContextUsage {
            used_tokens: 150,
            max_tokens: 0,
        })
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test agent_usage_updated_event_updates_users_context_snapshot -- --nocapture`  
Expected: FAIL because `AgentEvent::UsageUpdated` does not exist yet.

- [ ] **Step 3: Add usage payload/event contracts**

```rust
// src/provider/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub enum ProviderEvent {
    // ...
    Usage(ProviderUsage),
    TurnComplete,
    Error(String),
}

// src/agent/protocol.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub enum AgentEvent {
    // ...
    UsageUpdated {
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
    },
    TurnComplete,
    TurnFailed { /* ... */ },
}
```

- [ ] **Step 4: Relay usage through runtime observer and turn loop**

```rust
// src/agent/runtime/events.rs
pub(crate) trait TurnObserver {
    // ...
    fn on_usage(&self, usage: crate::provider::ProviderUsage);
}

impl TurnObserver for ChannelObserver<'_> {
    // ...
    fn on_usage(&self, usage: crate::provider::ProviderUsage) {
        let _ = self.tx.send(AgentEvent::UsageUpdated {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        });
    }
}

// src/agent/runtime/turn.rs
match event {
    ProviderEvent::Usage(usage) => observer.on_usage(usage),
    ProviderEvent::TurnComplete => { /* unchanged */ }
    // ...
}
```

- [ ] **Step 5: Handle usage event in ChatApp**

```rust
// src/chat/app/events.rs
AgentEvent::UsageUpdated {
    prompt_tokens: _,
    completion_tokens: _,
    total_tokens,
} => {
    self.context_usage = Some(ContextUsage {
        used_tokens: total_tokens,
        max_tokens: self.context_usage.map(|c| c.max_tokens).unwrap_or(0),
    });
    self.scroll_offset = u16::MAX;
}
```

- [ ] **Step 6: Run focused tests to verify pass**

Run: `cargo test agent_usage_updated_event_updates_users_context_snapshot -- --nocapture`  
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/provider/mod.rs src/agent/protocol.rs src/agent/runtime/events.rs src/agent/runtime/turn.rs src/chat/app/events.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(context): add usage event pipeline from provider to chat app"
```

---

### Task 2: OpenAI-Compatible Streaming Usage Emission (TDD)

**Files:**
- Modify: `src/provider/openai_compat.rs`
- Test: `tests/unit/provider/openai_compat_tests.rs`

- [ ] **Step 1: Write failing tests for stream usage option and chunk usage parse**

```rust
#[test]
fn build_request_sets_stream_include_usage() {
    let provider = test_provider();
    let req = provider.build_request_for_test(&sample_messages(), &[]);
    assert_eq!(req["stream_options"]["include_usage"], serde_json::json!(true));
}

#[test]
fn usage_chunk_emits_provider_usage_event() {
    let chunk: StreamChunk = serde_json::from_value(serde_json::json!({
        "choices": [],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "total_tokens": 120
        }
    }))
    .expect("chunk should parse");
    let usage = extract_usage_from_chunk(&chunk).expect("usage should exist");
    assert_eq!(usage.total_tokens, 120);
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test build_request_sets_stream_include_usage usage_chunk_emits_provider_usage_event -- --nocapture`  
Expected: FAIL because `stream_options`/usage extraction is not wired.

- [ ] **Step 3: Add stream usage fields and extraction helper**

```rust
// src/provider/openai_compat.rs
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamChunk {
    #[serde(default)]
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub usage: Option<CompletionUsage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

fn extract_usage_from_chunk(chunk: &StreamChunk) -> Option<crate::provider::ProviderUsage> {
    chunk.usage.as_ref().map(|u| crate::provider::ProviderUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    })
}
```

- [ ] **Step 4: Add `stream_options.include_usage=true` and emit usage events**

```rust
// build_request(...)
obj.insert("stream".into(), json!(true));
obj.insert(
    "stream_options".into(),
    json!({ "include_usage": true }),
);

// stream loop
if let Some(usage) = extract_usage_from_chunk(&chunk) {
    let _ = tx.send(ProviderEvent::Usage(usage));
}
```

- [ ] **Step 5: Run focused provider tests**

Run: `cargo test openai_compat_tests:: -- --nocapture`  
Expected: PASS for newly added usage tests and existing openai_compat tests.

- [ ] **Step 6: Commit**

```bash
git add src/provider/openai_compat.rs tests/unit/provider/openai_compat_tests.rs
git commit -m "feat(context): emit streaming usage from openai-compatible provider"
```

---

### Task 3: OpenRouter Usage Emission + Model Context Metadata (TDD)

**Files:**
- Modify: `src/provider/mod.rs`
- Modify: `src/provider/openrouter.rs`
- Modify: `src/provider/openai_compat.rs`
- Test: `src/provider/openrouter.rs` (inline tests)
- Test: `tests/unit/provider/openai_compat_tests.rs`

- [ ] **Step 1: Write failing metadata/usage tests**

```rust
#[test]
fn model_info_carries_context_length_when_available() {
    let info = ModelInfo {
        id: "openai/gpt-4o-mini".into(),
        description: "gpt-4o-mini".into(),
        context_length: Some(128000),
    };
    assert_eq!(info.context_length, Some(128000));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test model_info_carries_context_length_when_available -- --nocapture`  
Expected: FAIL because `ModelInfo` lacks `context_length`.

- [ ] **Step 3: Extend `ModelInfo` and provider model mapping**

```rust
// src/provider/mod.rs
pub struct ModelInfo {
    pub id: String,
    pub description: String,
    pub context_length: Option<u32>,
}

// src/provider/openrouter.rs
.map(|m| ModelInfo {
    description: m.name.clone(),
    id: m.id,
    context_length: m.context_length.map(|v| v as u32),
})

// src/provider/openai_compat.rs
.map(|m| super::ModelInfo {
    description: m.id.clone(),
    id: m.id,
    context_length: None,
})
```

- [ ] **Step 4: Emit OpenRouter usage at stream completion**

```rust
// src/provider/openrouter.rs
match event {
    StreamEvent::Done { tool_calls, usage, .. } => {
        if let Some(usage) = usage {
            let _ = tx.send(ProviderEvent::Usage(crate::provider::ProviderUsage {
                prompt_tokens: usage.prompt_tokens as u32,
                completion_tokens: usage.completion_tokens as u32,
                total_tokens: usage.total_tokens as u32,
            }));
        }
        // existing tool-call + TurnComplete logic
    }
    // ...
}
```

- [ ] **Step 5: Run focused tests**

Run: `cargo test openrouter::tests:: -- --nocapture && cargo test model_info_carries_context_length_when_available -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/provider/mod.rs src/provider/openrouter.rs src/provider/openai_compat.rs tests/unit/provider/openai_compat_tests.rs
git commit -m "feat(context): add provider model context metadata and openrouter usage events"
```

---

### Task 4: Resolve Max Window (Metadata First, Config Fallback) and Persist in App (TDD)

**Files:**
- Modify: `src/provider/mod.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/events.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing max-resolution tests**

```rust
#[test]
fn usage_update_prefers_model_context_length_over_config_max() {
    let mut app = new_test_app();
    app.model_context_max_tokens = Some(128000);
    app.config_max_tokens = Some(8192);
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated {
        prompt_tokens: 50,
        completion_tokens: 10,
        total_tokens: 60,
    });
    assert_eq!(app.users_status_snapshot().context_usage.unwrap().max_tokens, 128000);
}

#[test]
fn usage_update_falls_back_to_config_max_when_context_unknown() {
    let mut app = new_test_app();
    app.model_context_max_tokens = None;
    app.config_max_tokens = Some(8192);
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated {
        prompt_tokens: 50,
        completion_tokens: 10,
        total_tokens: 60,
    });
    assert_eq!(app.users_status_snapshot().context_usage.unwrap().max_tokens, 8192);
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test usage_update_prefers_model_context_length_over_config_max usage_update_falls_back_to_config_max_when_context_unknown -- --nocapture`  
Expected: FAIL before resolver wiring.

- [ ] **Step 3: Add resolver helpers and app fields**

```rust
// src/provider/mod.rs
pub fn resolve_model_context_length(repo_root: &str, model_id: &str) -> Option<u32> {
    available_models(repo_root)
        .into_iter()
        .find(|m| m.id == model_id)
        .and_then(|m| m.context_length)
}

pub fn configured_max_tokens(repo_root: &str) -> Option<u32> {
    openai_compat::OpenAiConfig::load(repo_root).ok().map(|cfg| cfg.max_tokens)
}

// src/chat/app.rs
model_context_max_tokens: Option<u32>,
config_max_tokens: Option<u32>,
```

- [ ] **Step 4: Initialize/refresh max-token sources and write usage max**

```rust
// ChatApp::new
let config_max_tokens = crate::provider::configured_max_tokens(repo_path);
let model_context_max_tokens = crate::provider::resolve_model_context_length(repo_path, &model_name);

// ChatApp::set_model success branch
self.model_context_max_tokens =
    crate::provider::resolve_model_context_length(&self.workspace_root.to_string_lossy(), model);

// app/events.rs usage handler
let resolved_max = self
    .model_context_max_tokens
    .or(self.config_max_tokens)
    .unwrap_or(0);
self.context_usage = Some(ContextUsage {
    used_tokens: total_tokens,
    max_tokens: resolved_max,
});
```

- [ ] **Step 5: Run focused app tests**

Run: `cargo test usage_update_prefers_model_context_length_over_config_max usage_update_falls_back_to_config_max_when_context_unknown -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/provider/mod.rs src/chat/app.rs src/chat/app/events.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(context): resolve max context tokens via metadata with config fallback"
```

---

### Task 5: UI Regression + Full Verification

**Files:**
- Modify: `tests/chat_render.rs` (if assertion updates needed)
- Modify: `tests/unit/chat/app/tests_impl.inc` (additional state assertions if needed)

- [ ] **Step 1: Add/adjust users-row rendering test for real usage**

```rust
#[test]
fn mode_row_renders_provider_usage_ratio() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.model_context_max_tokens = Some(8000);
    app.apply_agent_event_for_test(AgentEvent::UsageUpdated {
        prompt_tokens: 900,
        completion_tokens: 100,
        total_tokens: 1000,
    });
    let lines = render_to_lines_for_test(&mut app, 120, 24);
    let users_rows = &lines[lines.len().saturating_sub(5)..];
    assert!(users_rows[4].contains("1000/8000 (12%)"));
}
```

- [ ] **Step 2: Run focused render/app/provider suites**

Run: `cargo test --test chat_render && cargo test openai_compat_tests:: && cargo test usage_update_ -- --nocapture`  
Expected: PASS.

- [ ] **Step 3: Run repository quality gates**

Run: `just fmt-check && just lint && just test`  
Expected: all PASS.

- [ ] **Step 4: Commit final regression adjustments (if any)**

```bash
git add tests/chat_render.rs tests/unit/chat/app/tests_impl.inc tests/unit/provider/openai_compat_tests.rs src/provider/openrouter.rs
git commit -m "test(context): cover provider usage-driven context window rendering"
```
