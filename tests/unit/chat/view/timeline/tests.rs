use super::*;
use crate::chat::users_state::UsersLayoutPolicy;
use crate::chat::view::render::contracts::{RenderCtx, RenderUnit};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

#[test]
fn timeline_renders_banner_and_thinking_entries() {
    timeline_initial_render_includes_banner_entry();
    timeline_keeps_banner_after_first_user_message_and_renders_thinking();
}

#[test]
fn timeline_initial_render_includes_banner_entry() {
    let mut app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");

    let lines = crate::chat::view::render_to_lines_for_test(&mut app, 100, 28);
    let text = lines.join("\n");
    assert!(
        text.contains("● Describe a task to get started."),
        "initial timeline should include the banner entry"
    );
}

#[test]
fn timeline_keeps_banner_after_first_user_message_and_renders_thinking() {
    let mut app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");
    app.send_message("hello");
    app.apply_agent_event_for_test(crate::agent::protocol::AgentEvent::ThinkingDelta {
        text: "reasoning".into(),
    });

    let lines = crate::chat::view::render_to_lines_for_test(&mut app, 100, 28);
    let text = lines.join("\n");
    assert!(
        text.contains("● Describe a task to get started."),
        "banner entry should remain visible after the first user message"
    );
    assert!(
        text.contains("reasoning"),
        "thinking text should render as a timeline entry"
    );
}

#[test]
fn timeline_render_unit_preserves_existing_timeline_output() {
    fn seed_timeline(app: &mut crate::chat::app::ChatApp) {
        use crate::agent::protocol::AgentEvent;

        app.send_message("hello from render unit");
        app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
            text: "reasoning".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::TextDelta {
            text: "response body".into(),
        });
    }

    fn render_with_timeline_function(app: &mut crate::chat::app::ChatApp) -> Vec<String> {
        let backend = TestBackend::new(72, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let theme = app.theme().clone();

        terminal
            .draw(|frame| render_timeline(frame, frame.area(), app, &theme))
            .expect("timeline should render");

        terminal
            .backend()
            .buffer()
            .content()
            .chunks(72)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn render_with_timeline_unit(app: &mut crate::chat::app::ChatApp) -> Vec<String> {
        let backend = TestBackend::new(72, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let theme = app.theme().clone();
        let mut ctx = RenderCtx::new(app, theme, 0, UsersLayoutPolicy::default());
        let unit = crate::chat::view::render::units::timeline::TimelineRenderUnit;

        terminal
            .draw(|frame| {
                unit.render(frame, frame.area(), &mut ctx)
                    .expect("timeline render unit should render");
            })
            .expect("timeline should render");

        terminal
            .backend()
            .buffer()
            .content()
            .chunks(72)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    let mut direct_app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");
    seed_timeline(&mut direct_app);
    let direct_lines = render_with_timeline_function(&mut direct_app);

    let mut unit_app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");
    seed_timeline(&mut unit_app);
    let unit_lines = render_with_timeline_unit(&mut unit_app);

    assert_eq!(
        unit_lines, direct_lines,
        "timeline render unit should preserve current timeline rendering output"
    );
}

#[test]
fn split_code_fences_no_code() {
    let segments = split_code_fences("Hello world\nSecond line");
    assert_eq!(segments.len(), 1);
    assert!(matches!(&segments[0], MdSegment::Text(t) if t == "Hello world\nSecond line"));
}

#[test]
fn split_code_fences_single_block() {
    let input = "Before\n```python\nprint('hi')\n```\nAfter";
    let segments = split_code_fences(input);
    assert_eq!(segments.len(), 3);
    assert!(matches!(&segments[0], MdSegment::Text(t) if t == "Before"));
    assert!(
        matches!(&segments[1], MdSegment::Code { lang, body } if lang == "python" && body == "print('hi')")
    );
    assert!(matches!(&segments[2], MdSegment::Text(t) if t == "After"));
}

#[test]
fn split_code_fences_multiple_blocks() {
    let input = "A\n```rust\nfn main(){}\n```\nB\n```go\nfunc main(){}\n```\nC";
    let segments = split_code_fences(input);
    assert_eq!(segments.len(), 5);
    assert!(matches!(&segments[0], MdSegment::Text(t) if t == "A"));
    assert!(matches!(&segments[1], MdSegment::Code { lang, .. } if lang == "rust"));
    assert!(matches!(&segments[2], MdSegment::Text(t) if t == "B"));
    assert!(matches!(&segments[3], MdSegment::Code { lang, .. } if lang == "go"));
    assert!(matches!(&segments[4], MdSegment::Text(t) if t == "C"));
}

#[test]
fn split_code_fences_unclosed_treated_as_text() {
    let input = "Before\n```python\nprint('hi')";
    let segments = split_code_fences(input);
    // Unclosed fence falls back to text
    assert!(segments.iter().all(|s| matches!(s, MdSegment::Text(_))));
}

#[test]
fn split_code_fences_empty_body() {
    let input = "```\n```";
    let segments = split_code_fences(input);
    assert_eq!(segments.len(), 1);
    assert!(
        matches!(&segments[0], MdSegment::Code { lang, body } if lang.is_empty() && body.is_empty())
    );
}

#[test]
fn render_fenced_code_has_borders_and_bg() {
    let theme = crate::chat::theme::build_theme();
    let lines = render_fenced_code("python", "x = 1\ny = 2", &theme, 40);
    // top border + 2 code lines + bottom border = 4
    assert_eq!(lines.len(), 4);

    // First line contains language label
    let top_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(top_text.contains("python"));

    // Code lines have code_bg background
    for code_line in &lines[1..3] {
        let has_bg = code_line
            .spans
            .iter()
            .any(|s| s.style.bg == Some(theme.code_bg));
        assert!(has_bg, "code line should have code_bg background");
    }

    // Code lines are padded to width
    let code_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(UnicodeWidthStr::width(code_text.as_str()), 40);
}

#[test]
fn render_fenced_code_empty_body() {
    let theme = crate::chat::theme::build_theme();
    let lines = render_fenced_code("", "", &theme, 20);
    // top border + 1 blank bg line + bottom border = 3
    assert_eq!(lines.len(), 3);
}
