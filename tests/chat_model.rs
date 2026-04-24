use blazar::agent::tools::ToolKind;
use blazar::chat::model::{Actor, EntryKind, TimelineEntry, ToolCallStatus};

#[test]
fn timeline_entry_constructors_assign_expected_fields() {
    let response = TimelineEntry::response("assistant");
    assert_eq!(response.actor, Actor::Assistant);
    assert_eq!(response.kind, EntryKind::Message);

    let user = TimelineEntry::user_message("user");
    assert_eq!(user.actor, Actor::User);
    assert_eq!(user.kind, EntryKind::Message);

    let tool_use = TimelineEntry::tool_use("Edit", "src/main.rs", 2, 1, "updated");
    assert_eq!(tool_use.actor, Actor::Tool);
    assert!(matches!(
        tool_use.kind,
        EntryKind::ToolUse {
            additions: 2,
            deletions: 1,
            ..
        }
    ));

    let bash = TimelineEntry::bash("cargo test", "ok");
    assert!(matches!(bash.kind, EntryKind::Bash { .. }));

    let tool_call = TimelineEntry::tool_call(
        "call-1",
        "read_file",
        ToolKind::Local,
        r#"{"path":"Cargo.toml"}"#,
        "body",
        r#"{"path":"Cargo.toml"}"#,
        ToolCallStatus::Running,
    );
    assert!(matches!(
        tool_call.kind,
        EntryKind::ToolCall {
            status: ToolCallStatus::Running,
            ..
        }
    ));

    let thinking = TimelineEntry::thinking("reasoning");
    assert_eq!(thinking.kind, EntryKind::Thinking);

    let code_block = TimelineEntry::code_block("rust", "fn main() {}");
    assert!(matches!(code_block.kind, EntryKind::CodeBlock { .. }));

    let warning = TimelineEntry::warning("warn");
    assert_eq!(warning.actor, Actor::System);
    assert_eq!(warning.kind, EntryKind::Warning);

    let hint = TimelineEntry::hint("hint");
    assert_eq!(hint.actor, Actor::System);
    assert_eq!(hint.kind, EntryKind::Hint);
}

#[test]
fn timeline_entry_builder_helpers_attach_details_and_title() {
    let entry = TimelineEntry::response("body")
        .with_details("full details")
        .with_title("Plan Title");

    assert_eq!(entry.details, "full details");
    assert_eq!(entry.title.as_deref(), Some("Plan Title"));
}
