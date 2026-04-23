use super::common::extract_tool_subtitle;
use super::*;
use crate::agent::tools::ToolKind;

fn lines_text(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn render_entry_handles_user_and_empty_assistant_messages() {
    let theme = crate::chat::theme::build_theme();
    let user_lines = render_entry(&TimelineEntry::user_message("hello\nworld"), &theme, 60);
    let user_text = lines_text(&user_lines);
    assert!(user_text[0].contains("› "));
    assert!(user_text.iter().any(|line| line.contains("world")));

    let assistant_lines = render_entry(&TimelineEntry::response(""), &theme, 60);
    let assistant_text = lines_text(&assistant_lines).join("\n");
    assert!(assistant_text.contains("● "));
}

#[test]
fn render_entry_renders_markdown_and_fenced_code_segments() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::response("Intro\n```rust\nlet x = 1;\n```\nDone");
    let rendered = render_entry(&entry, &theme, 70);
    let text = lines_text(&rendered).join("\n");

    assert!(text.contains("Intro"));
    assert!(text.contains("rust"));
    assert!(text.contains("let x = 1;"));
    assert!(text.contains("Done"));
}

#[test]
fn render_entry_renders_tool_use_and_tool_call_statuses() {
    let theme = crate::chat::theme::build_theme();
    let tool_use = TimelineEntry::tool_use("Edit", "src/main.rs", 3, 1, "updated");
    let tool_text = lines_text(&render_entry(&tool_use, &theme, 70)).join("\n");
    assert!(tool_text.contains("Edit"));
    assert!(tool_text.contains("src/main.rs"));
    assert!(tool_text.contains("+3"));
    assert!(tool_text.contains("-1"));
    assert!(tool_text.contains("updated"));

    let running = TimelineEntry::tool_call(
        "c1",
        "read_file",
        ToolKind::Local,
        "reading",
        r#"{"path":"Cargo.toml"}"#,
        ToolCallStatus::Running,
    );
    let running_text = lines_text(&render_entry(&running, &theme, 70)).join("\n");
    assert!(running_text.contains("…"));
    assert!(running_text.contains("Cargo.toml"));

    let success = TimelineEntry::tool_call(
        "c2",
        "bash",
        ToolKind::Local,
        "done",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );
    let success_text = lines_text(&render_entry(&success, &theme, 70)).join("\n");
    assert!(success_text.contains("✓"));
    assert!(success_text.contains("cargo test"));

    let error = TimelineEntry::tool_call(
        "c3",
        "grep",
        ToolKind::Local,
        "failed",
        r#"{"pattern":"TODO"}"#,
        ToolCallStatus::Error,
    );
    let error_text = lines_text(&render_entry(&error, &theme, 70)).join("\n");
    assert!(error_text.contains("✗"));
    assert!(error_text.contains("TODO"));

    let acp_agent = TimelineEntry::tool_call(
        "c4",
        "configured_reviewer",
        ToolKind::Agent { is_acp: true },
        "reviewing",
        r#"{"prompt":"check this diff"}"#,
        ToolCallStatus::Running,
    );
    let acp_agent_text = lines_text(&render_entry(&acp_agent, &theme, 70)).join("\n");
    assert!(acp_agent_text.contains("configured_reviewer"));
    assert!(acp_agent_text.contains("(ACP)"));
}

#[test]
fn tool_descriptor_maps_status_and_semantic_summary() {
    let running = TimelineEntry::tool_call(
        "call-1",
        "read_file",
        ToolKind::Local,
        "reading",
        r#"{"path":"src/main.rs"}"#,
        ToolCallStatus::Running,
    );
    let descriptor = super::tooling::tool_descriptor(&running);

    assert_eq!(
        descriptor.status_visual,
        super::tooling::descriptor::StatusVisual::RunningDot
    );
    assert_eq!(descriptor.subtitle.as_deref(), Some("src/main.rs"));
}

#[test]
fn render_entry_renders_bash_warning_hint_thinking_and_code_block() {
    let theme = crate::chat::theme::build_theme();

    let body = (0..12)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let bash_entry = TimelineEntry::bash("echo hello", body);
    let bash_text = lines_text(&render_entry(&bash_entry, &theme, 70)).join("\n");
    assert!(bash_text.contains("$ echo hello"));
    assert!(bash_text.contains("lines hidden"));
    assert!(bash_text.contains("line-11"));
    assert!(!bash_text.contains("line-0"));

    let warning = TimelineEntry::warning("warn line 1\nwarn line 2");
    let warning_text = lines_text(&render_entry(&warning, &theme, 70)).join("\n");
    assert!(warning_text.contains("! "));
    assert!(warning_text.contains("warn line 2"));

    let hint = TimelineEntry::hint("hint line 1\nhint line 2");
    let hint_text = lines_text(&render_entry(&hint, &theme, 70)).join("\n");
    assert!(hint_text.contains("💡"));
    assert!(hint_text.contains("hint line 2"));

    let thinking = TimelineEntry::thinking(
        "this is a long thinking paragraph that should wrap into many lines for collapse testing. \
             it keeps describing alternative approaches and safety checks so the rendered block \
             must be truncated in compact timeline mode.",
    );
    let thinking_text = lines_text(&render_entry(&thinking, &theme, 18)).join("\n");
    assert!(thinking_text.contains("🧠 Thinking"));
    assert!(thinking_text.contains("Ctrl+O"));

    let code = TimelineEntry::code_block("rust", "fn main() {}".to_string());
    let code_text = lines_text(&render_entry(&code, &theme, 50)).join("\n");
    assert!(code_text.contains("rust"));
    assert!(code_text.contains("fn main() {}"));
}

#[test]
fn extract_tool_subtitle_handles_known_keys_fallbacks_and_truncation() {
    assert_eq!(
        extract_tool_subtitle("read_file", r#"{"path":"src/main.rs"}"#),
        "src/main.rs"
    );
    assert_eq!(
        extract_tool_subtitle("bash", r#"{"command":"cargo test"}"#),
        "cargo test"
    );
    assert_eq!(
        extract_tool_subtitle("unknown", r#"{"file":"src/lib.rs"}"#),
        "src/lib.rs"
    );
    assert_eq!(extract_tool_subtitle("unknown", "{bad json"), "");

    let long = "x".repeat(90);
    let subtitle = extract_tool_subtitle("read_file", &format!(r#"{{"path":"{long}"}}"#));
    assert!(subtitle.ends_with('…'));
    assert_eq!(subtitle.chars().count(), 78);
}
