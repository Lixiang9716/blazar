# Render Entry Declarative Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `render_entry` 的工具条目渲染改为声明式两阶段模型（先描述、后渲染），支持运行/成功圆点、失败 `x`，并实现按工具类型 × 内容类型的结果渲染与 `Ctrl+O` 详情展开。

**Architecture:** 保持 `TimelineEntry` 作为产品状态源，在 `render_entry/tooling` 内新增声明模型与渲染器模块。`tooling.rs` 仅编排：`TimelineEntry -> EntryDescriptor -> Line`。结果展示采用紧凑摘要（1-2 行）+ 完整详情分离，兼容并行工具调用（按 `call_id` 独立）。

**Tech Stack:** Rust, ratatui (`Line`/`Span`/`Style`), existing timeline tests (`cargo test`), repo gates (`just fmt-check`, `just lint`, `just test`)

---

## File Structure Mapping

- Create: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
  - 责任：声明模型 `EntryDescriptor`、状态映射、语义提取、结果模式判定、摘要生成
- Create: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
  - 责任：将 `EntryDescriptor` 转成 UI 行（头部 + 摘要）
- Modify: `src/chat/view/timeline/render_entry/tooling.rs`
  - 责任：桥接 descriptor/renderer；保留 `render_tool_use_entry` / `render_tool_call_entry` / `render_bash_entry` 对外入口
- Modify: `src/chat/view/timeline/render_entry.rs`
  - 责任：保持调用面稳定，仅适配模块拆分
- Modify: `src/chat/view/timeline/render_entry/common.rs`
  - 责任：复用并增强工具语义提取函数（保证 fallback 行为）
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`
  - 责任：新增状态符号、模式分发、摘要长度、并行工具调用相关测试

---

### Task 1: 建立声明模型骨架（TDD）

**Files:**
- Create: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn tool_descriptor_maps_status_and_semantic_summary() {
    let theme = crate::chat::theme::build_theme();
    let running = TimelineEntry::tool_call(
        "call-1",
        "read_file",
        ToolKind::Local,
        "reading",
        r#"{"path":"src/main.rs"}"#,
        ToolCallStatus::Running,
    );
    let lines = render_entry(&running, &theme, 70);
    let text = lines_text(&lines).join("\n");
    assert!(text.contains("●")); // running dot
    assert!(text.contains("src/main.rs")); // semantic subtitle
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test tool_descriptor_maps_status_and_semantic_summary -- --nocapture`  
Expected: FAIL with missing behavior (dot/semantic summary assertion failure or compile failure before descriptor wiring)

- [ ] **Step 3: Write minimal implementation**

```rust
// src/chat/view/timeline/render_entry/tooling/descriptor.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusVisual {
    RunningDot,
    EndedDot,
    ErrorX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultMode {
    Markdown,
    Code,
    Diff,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EntryDescriptor {
    pub status_visual: StatusVisual,
    pub title: String,
    pub subtitle: Option<String>,
    pub preview_lines: Vec<String>,
    pub result_mode: ResultMode,
    pub call_identity: Option<String>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test tool_descriptor_maps_status_and_semantic_summary -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline/render_entry/tooling/descriptor.rs \
        src/chat/view/timeline/render_entry/tooling.rs \
        tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "refactor(timeline): add tooling descriptor model"
```

---

### Task 2: 接入声明式渲染器与状态符号规则（TDD）

**Files:**
- Create: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn tool_call_status_visual_uses_dot_for_running_and_success_x_for_error() {
    let theme = crate::chat::theme::build_theme();
    let running = TimelineEntry::tool_call("c1", "bash", ToolKind::Local, "running", "{}", ToolCallStatus::Running);
    let success = TimelineEntry::tool_call("c2", "bash", ToolKind::Local, "done", "{}", ToolCallStatus::Success);
    let error = TimelineEntry::tool_call("c3", "bash", ToolKind::Local, "failed", "{}", ToolCallStatus::Error);

    let rt = lines_text(&render_entry(&running, &theme, 70)).join("\n");
    let st = lines_text(&render_entry(&success, &theme, 70)).join("\n");
    let et = lines_text(&render_entry(&error, &theme, 70)).join("\n");

    assert!(rt.contains("●"));
    assert!(st.contains("●"));
    assert!(et.contains("x"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test tool_call_status_visual_uses_dot_for_running_and_success_x_for_error -- --nocapture`  
Expected: FAIL (`x` not present or old status marker still rendered)

- [ ] **Step 3: Write minimal implementation**

```rust
// src/chat/view/timeline/render_entry/tooling/renderer.rs
fn status_marker(descriptor: &EntryDescriptor, theme: &ChatTheme) -> (String, Style) {
    match descriptor.status_visual {
        StatusVisual::RunningDot => ("●".to_owned(), theme.spinner),
        StatusVisual::EndedDot => ("●".to_owned(), theme.diff_add),
        StatusVisual::ErrorX => ("x".to_owned(), theme.marker_warning),
    }
}
```

```rust
// src/chat/view/timeline/render_entry/tooling.rs
pub(super) fn render_tool_call_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let descriptor = descriptor::build_tool_call_descriptor(entry);
    renderer::render_tool_descriptor(&descriptor, theme, marker_style)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test tool_call_status_visual_uses_dot_for_running_and_success_x_for_error -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline/render_entry/tooling/renderer.rs \
        src/chat/view/timeline/render_entry/tooling.rs \
        tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "refactor(timeline): render tool entries from descriptor"
```

---

### Task 3: 结果模式分发与 1-2 行摘要（TDD）

**Files:**
- Modify: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn tool_result_preview_is_capped_to_two_lines() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_call(
        "c-preview",
        "bash",
        ToolKind::Local,
        "line-1\nline-2\nline-3\nline-4",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );
    let rendered = render_entry(&entry, &theme, 80);
    let text = lines_text(&rendered);
    let preview_count = text.iter().filter(|line| line.contains("line-")).count();
    assert!(preview_count <= 2);
}

#[test]
fn tool_result_mode_detects_diff_markdown_code_plain() {
    let theme = crate::chat::theme::build_theme();

    let diff_entry = TimelineEntry::tool_call(
        "c-diff",
        "edit_file",
        ToolKind::Local,
        "diff --git a/src/main.rs b/src/main.rs\n@@ -1 +1 @@\n-old\n+new",
        r#"{"path":"src/main.rs"}"#,
        ToolCallStatus::Success,
    );
    let md_entry = TimelineEntry::tool_call(
        "c-md",
        "agent",
        ToolKind::Local,
        "# Title\n- item",
        r#"{"prompt":"summarize"}"#,
        ToolCallStatus::Success,
    );
    let code_entry = TimelineEntry::tool_call(
        "c-code",
        "bash",
        ToolKind::Local,
        "```rust\nfn main() {}\n```",
        r#"{"command":"cargo fmt"}"#,
        ToolCallStatus::Success,
    );
    let plain_entry = TimelineEntry::tool_call(
        "c-plain",
        "read_file",
        ToolKind::Local,
        "just plain text",
        r#"{"path":"Cargo.toml"}"#,
        ToolCallStatus::Success,
    );

    let diff_text = lines_text(&render_entry(&diff_entry, &theme, 80)).join("\n");
    let md_text = lines_text(&render_entry(&md_entry, &theme, 80)).join("\n");
    let code_text = lines_text(&render_entry(&code_entry, &theme, 80)).join("\n");
    let plain_text = lines_text(&render_entry(&plain_entry, &theme, 80)).join("\n");

    assert!(diff_text.contains("@@"));
    assert!(md_text.contains("Title"));
    assert!(code_text.contains("fn main() {}"));
    assert!(plain_text.contains("just plain text"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test tool_result_preview_is_capped_to_two_lines tool_result_mode_detects_diff_markdown_code_plain -- --nocapture`  
Expected: FAIL (当前实现会展示超过 2 行，且未统一模式分发)

- [ ] **Step 3: Write minimal implementation**

```rust
const MAX_PREVIEW_LINES: usize = 2;

fn infer_result_mode(tool_name: &str, text: &str) -> ResultMode {
    if tool_name == "edit_file" || text.contains("@@") || text.contains("diff --git") {
        return ResultMode::Diff;
    }
    if text.contains("```") {
        return ResultMode::Code;
    }
    if text.contains("# ") || text.contains("- ") {
        return ResultMode::Markdown;
    }
    ResultMode::Plain
}

fn build_preview_lines(text: &str) -> Vec<String> {
    text.lines().take(MAX_PREVIEW_LINES).map(ToOwned::to_owned).collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test tool_result_preview_is_capped_to_two_lines tool_result_mode_detects_diff_markdown_code_plain -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline/render_entry/tooling/descriptor.rs \
        src/chat/view/timeline/render_entry/tooling/renderer.rs \
        tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "feat(timeline): add tool result mode dispatch and compact previews"
```

---

### Task 4: 并行工具调用隔离（按 call_id）与详情展开一致性（TDD）

**Files:**
- Modify: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`
- Test: `tests/chat_tool_timeline.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parallel_tool_calls_with_same_name_keep_distinct_identity() {
    let theme = crate::chat::theme::build_theme();
    let a = TimelineEntry::tool_call("call-a", "bash", ToolKind::Local, "done-a", r#"{"command":"echo a"}"#, ToolCallStatus::Success);
    let b = TimelineEntry::tool_call("call-b", "bash", ToolKind::Local, "done-b", r#"{"command":"echo b"}"#, ToolCallStatus::Running);

    let at = lines_text(&render_entry(&a, &theme, 70)).join("\n");
    let bt = lines_text(&render_entry(&b, &theme, 70)).join("\n");

    assert!(at.contains("echo a"));
    assert!(bt.contains("echo b"));
    assert_ne!(at, bt);
}
```

```rust
#[test]
fn ctrl_o_details_path_keeps_full_tool_result_text() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "line-1\nline-2\nline-3".into(),
        is_error: false,
    });

    app.handle_action(InputAction::ToggleDetails);
    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(lines.iter().any(|line| line.contains("line-3")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test parallel_tool_calls_with_same_name_keep_distinct_identity ctrl_o_details_path_keeps_full_tool_result_text -- --nocapture`  
Expected: FAIL (并行辨识信息不足或详情/摘要边界不稳定)

- [ ] **Step 3: Write minimal implementation**

```rust
// descriptor.rs
if let EntryKind::ToolCall { call_id, .. } = &entry.kind {
    descriptor.call_identity = Some(call_id.clone());
}

// renderer.rs (仅在需要区分时附带 identity)
if let Some(call_id) = &descriptor.call_identity {
    header.push(Span::styled(format!(" [{call_id}]"), theme.dim_text));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test parallel_tool_calls_with_same_name_keep_distinct_identity ctrl_o_details_path_keeps_full_tool_result_text -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline/render_entry/tooling/descriptor.rs \
        src/chat/view/timeline/render_entry/tooling/renderer.rs \
        tests/unit/chat/view/timeline/render_entry/tests.rs \
        tests/chat_tool_timeline.rs
git commit -m "feat(timeline): isolate parallel tool-call rendering by call id"
```

---

### Task 5: 全量回归与文档同步

**Files:**
- Modify: `docs/superpowers/specs/2026-04-23-render-entry-declarative-tooling-design.md` (若实现细节与 spec 有小偏差则同步)
- Modify: `docs/superpowers/plans/2026-04-23-render-entry-declarative-tooling.md` (勾选执行记录，仅在执行时更新)

- [ ] **Step 1: Run focused render-entry tests**

Run: `cargo test render_entry_ -- --nocapture`  
Expected: PASS

- [ ] **Step 2: Run repository gates**

Run: `just fmt-check && just lint && just test`  
Expected: all PASS

- [ ] **Step 3: Final docs consistency pass**

```markdown
- 确认状态规则文案为：running/success=圆点，error=x
- 确认默认摘要为 1-2 行
- 确认并行 call_id 隔离已在 spec 与测试中体现
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-04-23-render-entry-declarative-tooling-design.md \
        docs/superpowers/plans/2026-04-23-render-entry-declarative-tooling.md
git commit -m "docs: align declarative tooling spec and execution plan"
```
