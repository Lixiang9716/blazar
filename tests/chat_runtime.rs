use blazar::chat::app::ChatApp;
use blazar::chat::input::InputAction;
use blazar::chat::root::{RootApp, RootMode};
use blazar::chat::workspace::{WorkspaceApp, WorkspaceFocus, WorkspaceView};
use blazar::chat::workspace_catalog::{LaunchDecision, WorkspaceCatalog, WorkspaceRecord};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn enter_key_submits_composer_content_and_clears() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.set_composer_text("Hello Spirit");

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Enter));
    app.handle_action(action);

    assert!(
        app.messages()
            .iter()
            .any(|msg| msg.body.contains("Hello Spirit"))
    );
    assert_eq!(app.composer_text(), "");
}

#[test]
fn esc_key_requests_quit() {
    let _app = ChatApp::new_for_test(REPO_ROOT);
    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Esc));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn ctrl_c_requests_quit() {
    let _app = ChatApp::new_for_test(REPO_ROOT);
    let action =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn digit_shortcuts_switch_workspace_views() {
    let chat = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    let git = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    let sessions =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));

    assert!(matches!(chat, InputAction::SelectChatView));
    assert!(matches!(git, InputAction::SelectGitView));
    assert!(matches!(sessions, InputAction::SelectSessionsView));
}

#[test]
fn character_input_is_forwarded_to_composer() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Char('a')));
    app.handle_action(action);

    // Composer should have received the character
    assert!(app.composer_text().contains('a'));
}

#[test]
fn tab_shortcut_cycles_workspace_focus() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);
}

#[test]
fn workspace_routes_shortcuts_and_forwards_other_keys() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Git);
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Sessions);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    assert!(app.chat().composer_text().contains('a'));
}

#[test]
fn digit_shortcuts_type_into_composer_when_footer_is_focused() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.active_view(), WorkspaceView::Chat);
    assert_eq!(app.chat().composer_text(), "123");
}

#[test]
fn app_tracks_quit_flag() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}

// Task 6: workspace runtime wiring — quit and shortcut behavior
#[test]
fn workspace_app_quits_on_quit_action() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(
        app.should_quit(),
        "WorkspaceApp must expose should_quit() after Quit action"
    );
}

#[test]
fn workspace_default_view_is_chat() {
    let app = WorkspaceApp::new_for_test(REPO_ROOT);
    assert_eq!(
        app.active_view(),
        WorkspaceView::Chat,
        "Chat must be the default home view"
    );
}

#[test]
fn digit_shortcuts_from_footer_switch_views_in_non_chat_view() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    // Switch to Git view first (focus becomes Content)
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Git);

    // Cycle focus to Footer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    // Press '1' while in Git view with Footer focus → should switch to Chat view, not type into composer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::NONE,
    )));
    assert_eq!(
        app.active_view(),
        WorkspaceView::Chat,
        "pressing '1' in non-chat footer should switch to Chat view"
    );
    assert_eq!(
        app.chat().composer_text(),
        "",
        "pressing '1' in non-chat footer must not type into the composer"
    );
}

#[test]
fn digit_shortcuts_from_footer_in_chat_view_still_type_into_composer() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    // default is Chat view
    assert_eq!(app.active_view(), WorkspaceView::Chat);

    // Cycle focus twice to reach Footer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    // Press '2' while in Chat view with Footer focus → should type into composer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(
        app.active_view(),
        WorkspaceView::Chat,
        "view must not change when typing in chat footer"
    );
    assert_eq!(
        app.chat().composer_text(),
        "2",
        "pressing '2' in chat footer must type into the composer"
    );
}

#[test]
fn generic_key_input_in_non_chat_footer_does_not_write_to_composer() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    // Switch to Git view (focus becomes Content)
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Git);

    // Cycle focus from Content → Footer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    // Type an ordinary character — must not reach the hidden composer
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )));

    assert_eq!(
        app.chat().composer_text(),
        "",
        "generic key input in a non-chat footer must not write into the hidden composer"
    );
}

#[test]
fn root_app_transitions_from_launcher_into_requested_workspace_view() {
    let catalog = WorkspaceCatalog {
        last_opened: None,
        workspaces: vec![WorkspaceRecord::named("blazar", "/tmp/blazar")],
    };
    let mut app = RootApp::from_launch_decision(catalog, LaunchDecision::ShowLauncher);

    let outcome = app.handle_action(InputAction::Key(KeyEvent::from(KeyCode::Char('g'))));

    assert!(outcome.is_none());
    assert!(matches!(app.mode(), RootMode::Workspace(_)));
    assert_eq!(app.workspace().unwrap().active_view(), WorkspaceView::Git);
}

#[test]
fn root_app_resumes_directly_into_chat_workspace_view_by_default() {
    let catalog = WorkspaceCatalog::default();
    let app = RootApp::from_launch_decision(
        catalog,
        LaunchDecision::Resume {
            repo_path: REPO_ROOT.into(),
            initial_view: None,
        },
    );

    assert!(matches!(app.mode(), RootMode::Workspace(_)));
    assert_eq!(app.workspace().unwrap().active_view(), WorkspaceView::Chat);
}

#[test]
fn root_app_respects_explicit_initial_view_on_resume() {
    let catalog = WorkspaceCatalog::default();
    let app = RootApp::from_launch_decision(
        catalog,
        LaunchDecision::Resume {
            repo_path: REPO_ROOT.into(),
            initial_view: Some(WorkspaceView::Sessions),
        },
    );

    assert!(matches!(app.mode(), RootMode::Workspace(_)));
    assert_eq!(app.workspace().unwrap().active_view(), WorkspaceView::Sessions);
}

// Live-data gap: new_for_test must be deterministic and not load real repo/session state.
#[test]
fn new_for_test_git_summary_is_deterministic() {
    let app = WorkspaceApp::new_for_test(REPO_ROOT);
    // Must reflect GitSummary::for_test() values, not live git state.
    assert_eq!(
        app.git_summary().branch,
        "main",
        "new_for_test must use GitSummary::for_test(), not live git"
    );
    assert_eq!(app.git_summary().ahead, 2);
    assert_eq!(app.git_summary().staged, 1);
    assert_eq!(app.git_summary().unstaged, 3);
    assert!(
        !app.git_summary().recent_commits.is_empty(),
        "for_test should have at least one commit"
    );
}

#[test]
fn new_for_test_session_summary_is_deterministic() {
    let app = WorkspaceApp::new_for_test(REPO_ROOT);
    // Must reflect SessionSummary::for_test() values, not live session state.
    assert_eq!(
        app.session_summary().session_label,
        "spirit-workspace-tui",
        "new_for_test must use SessionSummary::for_test(), not live session"
    );
    assert_eq!(app.session_summary().ready_todos, 2);
    assert_eq!(app.session_summary().in_progress_todos, 1);
    assert_eq!(app.session_summary().done_todos, 4);
}

// Issue 2: repo-path resolution must be testable independently of terminal setup.
use blazar::chat::app::resolve_repo_path;

#[test]
fn resolve_repo_path_uses_schema_repopath_default() {
    let schema = serde_json::json!({
        "properties": {
            "workspace": {
                "properties": {
                    "repoPath": { "default": "/home/user/myproject" }
                }
            }
        }
    });
    assert_eq!(resolve_repo_path(&schema), "/home/user/myproject");
}

#[test]
fn resolve_repo_path_falls_back_to_non_empty_string_when_schema_empty() {
    let schema = serde_json::json!({});
    let path = resolve_repo_path(&schema);
    assert!(
        !path.is_empty(),
        "fallback must not be empty; got: {path:?}"
    );
}

// Issue 1: Quit must still work when focus is Footer and view is non-Chat.
#[test]
fn quit_from_git_view_footer_propagates() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    // Switch to Git view (focus becomes Content)
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Git);

    // Advance focus to Footer
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    // Esc → Quit must propagate even though we are in a non-Chat footer
    app.handle_action(InputAction::Quit);
    assert!(
        app.should_quit(),
        "Quit action from non-Chat footer must propagate; should_quit was false"
    );
}

#[test]
fn quit_from_sessions_view_footer_propagates() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Sessions);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    app.handle_action(InputAction::Quit);
    assert!(
        app.should_quit(),
        "Quit action from Sessions footer must propagate"
    );
}
