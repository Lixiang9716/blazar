use blazar::chat::demo::{demo_playback_script, demo_timeline};
use blazar::chat::model::{Actor, EntryKind};

#[test]
fn demo_timeline_uses_first_three_script_entries() {
    let timeline = demo_timeline();
    let script = demo_playback_script();

    assert_eq!(timeline.len(), 3);
    assert_eq!(timeline, script.into_iter().take(3).collect::<Vec<_>>());
}

#[test]
fn demo_playback_script_covers_major_entry_kinds() {
    let script = demo_playback_script();

    assert!(script.len() > 10);
    assert!(script.iter().any(|entry| entry.actor == Actor::User));
    assert!(
        script
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::ToolUse { .. }))
    );
    assert!(
        script
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Bash { .. }))
    );
    assert!(script.iter().any(|entry| entry.kind == EntryKind::Thinking));
    assert!(
        script
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::CodeBlock { .. }))
    );
}
