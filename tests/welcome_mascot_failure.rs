use blazar::welcome::mascot::{render_mascot, render_mascot_lines, render_mascot_plain};
use blazar::welcome::state::WelcomeState;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn switch_to(path: &Path) -> Self {
        let original = std::env::current_dir().expect("current dir should be readable");
        std::env::set_current_dir(path).expect("test should switch current dir");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original).expect("test should restore current dir");
    }
}

fn fresh_empty_workspace() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-workspaces")
        .join(format!("blazar-mascot-fallback-{suffix}"));
    fs::create_dir_all(&path).expect("test workspace should be created");
    path
}

#[test]
fn welcome_mascot_falls_back_when_production_config_load_fails() {
    let workspace = fresh_empty_workspace();
    let guard = CurrentDirGuard::switch_to(&workspace);

    let first = std::panic::catch_unwind(|| render_mascot(WelcomeState::new(), 0));
    let second = std::panic::catch_unwind(|| render_mascot(WelcomeState::new(), 200));
    let plain = std::panic::catch_unwind(|| render_mascot_plain(WelcomeState::new(), 200));
    let lines = std::panic::catch_unwind(|| render_mascot_lines(WelcomeState::new(), 200));

    assert!(
        first.is_ok(),
        "render_mascot should not panic on config failure"
    );
    assert!(
        second.is_ok(),
        "render_mascot should remain non-panicking across repeated calls"
    );
    assert!(
        plain.is_ok(),
        "render_mascot_plain should not panic on config failure"
    );
    assert!(
        lines.is_ok(),
        "render_mascot_lines should not panic on config failure"
    );

    let first = first.expect("fallback result should be available");
    let second = second.expect("fallback result should be available");
    let plain = plain.expect("fallback result should be available");
    let lines = lines.expect("fallback result should be available");

    assert_eq!(first, "", "fallback mascot should be deterministic");
    assert_eq!(second, "", "fallback mascot should remain deterministic");
    assert_eq!(plain, "", "plain fallback mascot should be deterministic");
    assert!(
        lines.is_empty(),
        "line fallback mascot should be deterministic"
    );

    drop(guard);
    fs::remove_dir_all(&workspace).expect("test workspace should be cleaned up");
}
