use blazar::chat::event_loop::resolve_repo_path;
use serde_json::json;

#[test]
fn resolve_repo_path_prefers_schema_default_value() {
    let schema = json!({
        "properties": {
            "workspace": {
                "properties": {
                    "repoPath": {
                        "default": "/workspace/blazar"
                    }
                }
            }
        }
    });

    assert_eq!(resolve_repo_path(&schema), "/workspace/blazar");
}

#[test]
fn resolve_repo_path_falls_back_to_current_directory() {
    let schema = json!({});
    let expected = std::env::current_dir()
        .expect("cwd should be available")
        .display()
        .to_string();

    assert_eq!(resolve_repo_path(&schema), expected);
}
