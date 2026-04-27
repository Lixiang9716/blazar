use super::common::{extract_tool_subtitle, extract_tool_subtitle_from_details};
use super::*;
use crate::agent::tools::ToolKind;
use crate::chat::model::ToolCallStatus;

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

fn first_line_marker_style(lines: &[Line<'_>]) -> Option<Style> {
    lines
        .first()
        .and_then(|line| line.spans.get(1))
        .map(|span| span.style)
}

fn first_body_span_style(lines: &[Line<'_>]) -> Option<Style> {
    lines
        .first()
        .and_then(|line| line.spans.iter().skip(2).find(|span| !span.content.is_empty()))
        .map(|span| span.style)
}

#[test]
fn render_entry_handles_user_and_empty_assistant_messages() {
    let theme = crate::chat::theme::build_theme();
    let user_lines = render_entry(&TimelineEntry::user_message("hello\nworld"), &theme, 60);
    let user_text = lines_text(&user_lines);
    assert!(user_text[0].contains("› "));
    assert!(user_text.iter().any(|line| line.contains("world")));

    let assistant_lines = render_entry(&TimelineEntry::response(""), &theme, 60);
    assert!(
        assistant_lines.is_empty(),
        "empty assistant entries should not render stray status markers"
    );
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
fn assistant_message_uses_shared_markdown_body_renderer() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::response("## heading\n```diff\n- a\n+ b\n```");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("heading"));
    assert!(text.contains("- a"));
    assert!(text.contains("+ b"));
}

#[test]
fn thinking_entry_body_uses_streaming_highlight_style() {
    let theme = crate::chat::theme::build_theme();
    let thinking = TimelineEntry::thinking("streaming reasoning text");
    let lines = render_entry(&thinking, &theme, 70);

    let text_span_style = first_body_span_style(&lines).expect("thinking body span should exist");

    assert_eq!(text_span_style, theme.marker_thinking);
}

#[test]
fn assistant_message_body_style_remains_default_after_thinking_color_change() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::response("normal assistant text");
    let lines = render_entry(&entry, &theme, 70);

    let text_span_style =
        first_body_span_style(&lines).expect("assistant body span should exist");

    assert_eq!(text_span_style, theme.body_text);
}

#[test]
fn user_message_remains_plain_text() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::user_message("**literal** not markdown");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(
        text.contains("**literal**"),
        "user message should remain plain text"
    );
}

#[test]
fn code_block_entry_uses_shared_markdown_body_renderer() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::code_block("diff", "- old\n+ new");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
}

#[test]
fn warning_entry_renders_markdown_fence_without_literal_backticks() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::warning("```diff\n- old\n+ new\n```");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
    assert!(
        !text.contains("```"),
        "warning entries should render markdown fences via shared helper"
    );
}

#[test]
fn markdown_body_helper_renders_diff_fence_lines() {
    let theme = crate::chat::theme::build_theme();
    let lines = super::markdown_body::render_markdown_block(
        "```diff\n- old\n+ new\n```",
        &theme,
        60,
        vec![Span::raw("  "), Span::raw("● ")],
        vec![Span::raw("    ")],
    );
    let text = lines_text(&lines).join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
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
        r#"{"path":"Cargo.toml"}"#,
        "reading",
        r#"{"path":"Cargo.toml"}"#,
        ToolCallStatus::Running,
    );
    let running_lines = render_entry(&running, &theme, 70);
    let running_text = lines_text(&running_lines).join("\n");
    assert!(running_text.contains("read_file"));
    assert!(running_text.contains("Cargo.toml"));
    assert_eq!(running_text.matches('●').count(), 1);
    assert_eq!(first_line_marker_style(&running_lines), Some(theme.spinner));

    let success = TimelineEntry::tool_call(
        "c2",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo test"}"#,
        "done",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );
    let success_lines = render_entry(&success, &theme, 70);
    let success_text = lines_text(&success_lines).join("\n");
    assert!(success_text.contains("bash"));
    assert!(success_text.contains("cargo test"));
    assert_eq!(success_text.matches('●').count(), 1);
    assert_eq!(
        first_line_marker_style(&success_lines),
        Some(theme.diff_add)
    );

    let error = TimelineEntry::tool_call(
        "c3",
        "grep",
        ToolKind::Local,
        r#"{"pattern":"TODO"}"#,
        "failed",
        r#"{"pattern":"TODO"}"#,
        ToolCallStatus::Error,
    );
    let error_lines = render_entry(&error, &theme, 70);
    let error_text = lines_text(&error_lines).join("\n");
    assert!(error_text.contains("grep"));
    assert!(error_text.contains("TODO"));
    assert!(
        !error_text.contains("grep x"),
        "error marker should not trail the title"
    );
    assert_eq!(
        first_line_marker_style(&error_lines),
        Some(theme.marker_warning)
    );

    let acp_agent = TimelineEntry::tool_call(
        "c4",
        "configured_reviewer",
        ToolKind::Agent { is_acp: true },
        r#"{"prompt":"check this diff"}"#,
        "reviewing",
        r#"{"prompt":"check this diff"}"#,
        ToolCallStatus::Running,
    );
    let acp_agent_text = lines_text(&render_entry(&acp_agent, &theme, 70)).join("\n");
    assert!(acp_agent_text.contains("configured_reviewer"));
    assert!(acp_agent_text.contains("(ACP)"));
}

#[test]
fn tool_use_body_renders_markdown_but_header_stays_structured() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_use("Edit", "src/main.rs", 2, 1, "```diff\n- old\n+ new\n```");
    let lines = render_entry(&entry, &theme, 70);
    let text = lines_text(&lines).join("\n");
    assert!(text.contains("Edit"));
    assert!(text.contains("src/main.rs"));
    assert!(text.contains("+2"));
    assert!(text.contains("-1"));
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
    assert!(
        !text.contains("```"),
        "tool use body should render markdown fences via shared helper"
    );
}

#[test]
fn tool_use_body_preserves_plain_line_boundaries() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_use("Read", "Cargo.toml", 0, 0, "line-a\nline-b");
    let lines = lines_text(&render_entry(&entry, &theme, 120));

    let line_a_idx = lines
        .iter()
        .position(|line| line.contains("line-a"))
        .expect("first tool-use line should render");
    let line_b_idx = lines
        .iter()
        .position(|line| line.contains("line-b"))
        .expect("second tool-use line should render");

    assert_ne!(
        line_a_idx, line_b_idx,
        "line-oriented tool-use content should preserve source line boundaries"
    );
}

#[test]
fn tool_descriptor_maps_status_and_semantic_summary() {
    let running = TimelineEntry::tool_call(
        "call-1",
        "read_file",
        ToolKind::Local,
        r#"{"path":"src/main.rs"}"#,
        "reading",
        r#"{"path":"src/main.rs"}"#,
        ToolCallStatus::Running,
    );
    let descriptor = super::tooling::tool_descriptor(&running).unwrap();

    assert_eq!(
        descriptor.status_visual,
        super::tooling::descriptor::StatusVisual::RunningDot
    );
    assert_eq!(descriptor.subtitle.as_deref(), Some("src/main.rs"));

    let success = TimelineEntry::tool_call(
        "call-2",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo test"}"#,
        "done",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );
    let success_descriptor = super::tooling::tool_descriptor(&success).unwrap();
    assert_eq!(
        success_descriptor.status_visual,
        super::tooling::descriptor::StatusVisual::EndedDot
    );

    let error = TimelineEntry::tool_call(
        "call-3",
        "grep",
        ToolKind::Local,
        r#"{"pattern":"TODO"}"#,
        "failed",
        r#"{"pattern":"TODO"}"#,
        ToolCallStatus::Error,
    );
    let error_descriptor = super::tooling::tool_descriptor(&error).unwrap();
    assert_eq!(
        error_descriptor.status_visual,
        super::tooling::descriptor::StatusVisual::ErrorX
    );
}

#[test]
fn tool_call_render_hides_call_id_and_keeps_parameter_context() {
    let theme = crate::chat::theme::build_theme();
    let first = TimelineEntry::tool_call(
        "call-a",
        "bash",
        ToolKind::Local,
        r#"{"command":"echo a"}"#,
        "done",
        r#"{"command":"echo a"}"#,
        ToolCallStatus::Success,
    );
    let second = TimelineEntry::tool_call(
        "call-b",
        "bash",
        ToolKind::Local,
        r#"{"command":"echo a"}"#,
        "done",
        r#"{"command":"echo a"}"#,
        ToolCallStatus::Success,
    );

    let first_text = lines_text(&render_entry(&first, &theme, 70)).join("\n");
    let second_text = lines_text(&render_entry(&second, &theme, 70)).join("\n");

    assert!(
        !first_text.contains("call-a") && !second_text.contains("call-b"),
        "tool call id should not be rendered in timeline rows"
    );
    assert!(
        first_text.contains("echo a") && second_text.contains("echo a"),
        "tool call rows should keep argument context"
    );
}

#[test]
fn tool_result_preview_is_capped_to_two_lines() {
    let entry = TimelineEntry::tool_call(
        "c-preview",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo test"}"#,
        "line-1\nline-2\nline-3\nline-4",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );

    let descriptor = super::tooling::tool_descriptor(&entry).unwrap();

    assert_eq!(
        descriptor.preview_lines,
        vec!["line-1".to_string(), "line-2".to_string()]
    );
}

#[test]
fn tool_result_mode_detects_diff_markdown_code_plain() {
    use super::tooling::descriptor::ResultMode;

    let diff_entry = TimelineEntry::tool_call(
        "c-diff",
        "edit_file",
        ToolKind::Local,
        r#"{"path":"src/main.rs"}"#,
        "diff --git a/src/main.rs b/src/main.rs\n@@ -1 +1 @@\n-old\n+new",
        r#"{"path":"src/main.rs"}"#,
        ToolCallStatus::Success,
    );
    let markdown_entry = TimelineEntry::tool_call(
        "c-md",
        "notes",
        ToolKind::Local,
        r#"{"path":"notes.md"}"#,
        "# Title\n- item",
        r#"{"path":"notes.md"}"#,
        ToolCallStatus::Success,
    );
    let code_entry = TimelineEntry::tool_call(
        "c-code",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo fmt"}"#,
        "```rust\nfn main() {}\n```",
        r#"{"command":"cargo fmt"}"#,
        ToolCallStatus::Success,
    );
    let plain_entry = TimelineEntry::tool_call(
        "c-plain",
        "read_file",
        ToolKind::Local,
        r#"{"path":"Cargo.toml"}"#,
        "just plain text",
        r#"{"path":"Cargo.toml"}"#,
        ToolCallStatus::Success,
    );

    assert_eq!(
        super::tooling::tool_descriptor(&diff_entry)
            .unwrap()
            .result_mode,
        ResultMode::Diff
    );
    assert_eq!(
        super::tooling::tool_descriptor(&markdown_entry)
            .unwrap()
            .result_mode,
        ResultMode::Markdown
    );
    assert_eq!(
        super::tooling::tool_descriptor(&code_entry)
            .unwrap()
            .result_mode,
        ResultMode::Code
    );
    assert_eq!(
        super::tooling::tool_descriptor(&plain_entry)
            .unwrap()
            .result_mode,
        ResultMode::Plain
    );
}

#[test]
fn tool_result_mode_does_not_treat_edit_substring_as_diff() {
    use super::tooling::descriptor::ResultMode;

    let entry = TimelineEntry::tool_call(
        "c-non-diff",
        "credit_lookup",
        ToolKind::Local,
        r#"{"query":"credit"}"#,
        "plain result text",
        r#"{"query":"credit"}"#,
        ToolCallStatus::Success,
    );

    assert_eq!(
        super::tooling::tool_descriptor(&entry).unwrap().result_mode,
        ResultMode::Plain
    );
}

#[test]
fn tool_call_status_visual_uses_dot_for_running_and_success_x_for_error() {
    let theme = crate::chat::theme::build_theme();

    let (marker, style) = super::tooling::renderer::status_marker(
        super::tooling::descriptor::StatusVisual::RunningDot,
        &theme,
    );
    assert_eq!(marker, "●");
    assert_eq!(style, theme.spinner);

    let (marker, style) = super::tooling::renderer::status_marker(
        super::tooling::descriptor::StatusVisual::EndedDot,
        &theme,
    );
    assert_eq!(marker, "●");
    assert_eq!(style, theme.diff_add);

    let (marker, style) = super::tooling::renderer::status_marker(
        super::tooling::descriptor::StatusVisual::ErrorX,
        &theme,
    );
    assert_eq!(marker, "x");
    assert_eq!(style, theme.marker_warning);
}

#[test]
fn tool_descriptor_returns_none_for_non_tool_call_entries() {
    let message = TimelineEntry::response("hello");
    assert!(super::tooling::tool_descriptor(&message).is_none());

    let bash = TimelineEntry::bash("echo hi", "ok");
    assert!(super::tooling::tool_descriptor(&bash).is_none());
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
    assert!(
        !thinking_text.is_empty() && thinking_text.contains("…"),
        "thinking entries should render through the dedicated thinking renderer"
    );

    let code = TimelineEntry::code_block("rust", "fn main() {}".to_string());
    let code_text = lines_text(&render_entry(&code, &theme, 50)).join("\n");
    assert!(code_text.contains("rust"));
    assert!(code_text.contains("fn main() {}"));
}

#[test]
fn bash_body_renders_markdown_without_literal_fences() {
    let theme = crate::chat::theme::build_theme();
    let bash_entry = TimelineEntry::bash("echo hello", "```diff\n- old\n+ new\n```");
    let bash_text = lines_text(&render_entry(&bash_entry, &theme, 70)).join("\n");

    assert!(bash_text.contains("$ echo hello"));
    assert!(bash_text.contains("- old"));
    assert!(bash_text.contains("+ new"));
    assert!(
        !bash_text.contains("```"),
        "bash body should render markdown fences via shared helper"
    );
}

#[test]
fn bash_body_preserves_plain_line_boundaries() {
    let theme = crate::chat::theme::build_theme();
    let bash_entry = TimelineEntry::bash("echo hello", "alpha\nbeta");
    let lines = lines_text(&render_entry(&bash_entry, &theme, 120));

    let alpha_idx = lines
        .iter()
        .position(|line| line.contains("alpha"))
        .expect("alpha line should be rendered");
    let beta_idx = lines
        .iter()
        .position(|line| line.contains("beta"))
        .expect("beta line should be rendered");

    assert_ne!(
        alpha_idx, beta_idx,
        "line-oriented bash output should keep each source line on a separate rendered line"
    );
}

#[test]
fn tool_call_preview_preserves_plain_line_boundaries() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_call(
        "c-line-preserve",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo test"}"#,
        "line-1\nline-2\nline-3",
        r#"{"command":"cargo test"}"#,
        ToolCallStatus::Success,
    );
    let lines = lines_text(&render_entry(&entry, &theme, 120));

    let line1_idx = lines
        .iter()
        .position(|line| line.contains("line-1"))
        .expect("first preview line should be rendered");
    let line2_idx = lines
        .iter()
        .position(|line| line.contains("line-2"))
        .expect("second preview line should be rendered");

    assert_ne!(
        line1_idx, line2_idx,
        "line-oriented tool previews should keep line breaks instead of flattening into one line"
    );
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
    assert_eq!(
        extract_tool_subtitle("unknown", "{bad json"),
        "invalid args"
    );
    assert_eq!(
        extract_tool_subtitle_from_details(
            "read_file",
            "{\"path\":\"src/main.rs\"}\nbatch_id=1 replay_index=0 normalized_claims=<none>"
        ),
        "src/main.rs"
    );
    assert_eq!(
        extract_tool_subtitle_from_details(
            "read_file",
            "package\nname = \"blazar\"\n\nbatch_id=1 replay_index=0 normalized_claims=<none>"
        ),
        ""
    );
    assert_eq!(
        extract_tool_subtitle_from_details(
            "read_file",
            "{bad json\nbatch_id=1 replay_index=0 normalized_claims=<none>"
        ),
        "invalid args"
    );

    let long = "x".repeat(90);
    let subtitle = extract_tool_subtitle("read_file", &format!(r#"{{"path":"{long}"}}"#));
    assert!(subtitle.ends_with('…'));
    assert_eq!(subtitle.chars().count(), 78);
}

#[test]
fn tool_descriptor_uses_full_details_for_two_line_preview() {
    let entry = TimelineEntry::tool_call(
        "c-preview-details",
        "bash",
        ToolKind::Local,
        r#"{"command":"bash preview"}"#,
        "summary only",
        "line-1\nline-2\nline-3\n\nbatch_id=1 replay_index=0 normalized_claims=<none>",
        ToolCallStatus::Success,
    );

    let descriptor = super::tooling::tool_descriptor(&entry).unwrap();

    assert_eq!(
        descriptor.preview_lines,
        vec!["line-1".to_string(), "line-2".to_string()]
    );
}

#[test]
fn tool_descriptor_infers_mode_from_full_details_when_summary_is_plain() {
    use super::tooling::descriptor::ResultMode;

    let diff_entry = TimelineEntry::tool_call(
        "c-diff-details",
        "bash",
        ToolKind::Local,
        r#"{"command":"diff"}"#,
        "summary only",
        "diff --git a/src/main.rs b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n\nbatch_id=1 replay_index=0 normalized_claims=<none>",
        ToolCallStatus::Success,
    );
    let markdown_entry = TimelineEntry::tool_call(
        "c-md-details",
        "bash",
        ToolKind::Local,
        r#"{"command":"markdown"}"#,
        "summary only",
        "# Title\n- item\n\nbatch_id=1 replay_index=0 normalized_claims=<none>",
        ToolCallStatus::Success,
    );
    let code_entry = TimelineEntry::tool_call(
        "c-code-details",
        "bash",
        ToolKind::Local,
        r#"{"command":"code"}"#,
        "summary only",
        "```rust\nfn main() {}\n```\n\nbatch_id=1 replay_index=0 normalized_claims=<none>",
        ToolCallStatus::Success,
    );
    let plain_entry = TimelineEntry::tool_call(
        "c-plain-details",
        "bash",
        ToolKind::Local,
        r#"{"command":"plain"}"#,
        "summary only",
        "plain result text\nnext line\n\nbatch_id=1 replay_index=0 normalized_claims=<none>",
        ToolCallStatus::Success,
    );

    assert_eq!(
        super::tooling::tool_descriptor(&diff_entry)
            .unwrap()
            .result_mode,
        ResultMode::Diff
    );
    assert_eq!(
        super::tooling::tool_descriptor(&markdown_entry)
            .unwrap()
            .result_mode,
        ResultMode::Markdown
    );
    assert_eq!(
        super::tooling::tool_descriptor(&code_entry)
            .unwrap()
            .result_mode,
        ResultMode::Code
    );
    assert_eq!(
        super::tooling::tool_descriptor(&plain_entry)
            .unwrap()
            .result_mode,
        ResultMode::Plain
    );
}
