use blazar::chat::input::InputAction;
use blazar::chat::launcher::{LauncherApp, LauncherFocus, LauncherOutcome};
use blazar::chat::workspace::WorkspaceView;
use blazar::chat::workspace_catalog::WorkspaceRecord;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[test]
fn launcher_moves_selection_and_opens_sessions_for_selected_workspace() {
    let mut app = LauncherApp::new(vec![
        WorkspaceRecord::named("blazar", "/tmp/blazar"),
        WorkspaceRecord::named("graphify-lab", "/tmp/graphify"),
    ]);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.selected_index(), 1);

    let outcome = app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.focus(), LauncherFocus::List);
    assert_eq!(
        outcome,
        LauncherOutcome::OpenWorkspace {
            repo_path: PathBuf::from("/tmp/graphify"),
            initial_view: Some(WorkspaceView::Sessions),
        }
    );
}

#[test]
fn launcher_cycles_focus_and_wraps_back_to_list() {
    let mut app = LauncherApp::new(vec![WorkspaceRecord::named("blazar", "/tmp/blazar")]);

    assert_eq!(app.focus(), LauncherFocus::List);

    let outcome = app.handle_action(InputAction::CycleFocus);
    assert_eq!(outcome, LauncherOutcome::None);
    assert_eq!(app.focus(), LauncherFocus::Preview);

    let outcome = app.handle_action(InputAction::CycleFocus);
    assert_eq!(outcome, LauncherOutcome::None);
    assert_eq!(app.focus(), LauncherFocus::Actions);

    let outcome = app.handle_action(InputAction::CycleFocus);
    assert_eq!(outcome, LauncherOutcome::None);
    assert_eq!(app.focus(), LauncherFocus::List);
}

#[test]
fn launcher_opens_selected_workspace_with_enter_and_git_shortcut() {
    let mut app = LauncherApp::new(vec![WorkspaceRecord::named("blazar", "/tmp/blazar")]);

    let enter_outcome = app.handle_action(InputAction::Submit);
    assert_eq!(
        enter_outcome,
        LauncherOutcome::OpenWorkspace {
            repo_path: PathBuf::from("/tmp/blazar"),
            initial_view: None,
        }
    );

    let git_outcome = app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )));
    assert_eq!(
        git_outcome,
        LauncherOutcome::OpenWorkspace {
            repo_path: PathBuf::from("/tmp/blazar"),
            initial_view: Some(WorkspaceView::Git),
        }
    );
}

#[test]
fn launcher_keeps_selection_within_bounds() {
    let mut app = LauncherApp::new(vec![
        WorkspaceRecord::named("blazar", "/tmp/blazar"),
        WorkspaceRecord::named("graphify-lab", "/tmp/graphify"),
    ]);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Up,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.selected_index(), 0);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.selected_index(), 1);
}

#[test]
fn launcher_ignores_open_actions_when_no_workspaces_exist() {
    let mut app = LauncherApp::new(vec![]);

    assert_eq!(
        app.handle_action(InputAction::Submit),
        LauncherOutcome::None
    );
    assert_eq!(
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::NONE,
        ))),
        LauncherOutcome::None
    );
    assert_eq!(
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::NONE,
        ))),
        LauncherOutcome::None
    );
}
