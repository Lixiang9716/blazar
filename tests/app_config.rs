use blazar::app;
use blazar::config::{
    APP_SCHEMA_PATH, load_app_schema, load_app_schema_from_path, load_mascot_config,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn default_schema_path_uses_config_directory() {
    assert_eq!(APP_SCHEMA_PATH, "config/app.json");
}

#[test]
fn bundled_app_schema_contains_runtime_defaults() {
    let schema = load_app_schema().expect("bundled config should load");

    assert_eq!(schema["title"], "Blazar Mission Console");
    assert_eq!(
        schema["properties"]["task"]["properties"]["request"]["default"],
        "Work on this repository with clear, safe steps"
    );
    assert_eq!(
        schema["properties"]["workspace"]["properties"]["repoPath"]["default"],
        "/home/lx/blazar"
    );
}

#[test]
fn config_loader_reads_schema_from_json_file() {
    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("temp dir should be created");
    let path = dir.join("app.json");

    fs::write(
        &path,
        r#"{
            "title": "Custom Console",
            "properties": {
                "task": {
                    "properties": {
                        "request": {
                            "default": "Custom request"
                        }
                    }
                }
            }
        }"#,
    )
    .expect("temp config should be written");

    let schema = load_app_schema_from_path(&path).expect("custom schema should load");

    assert_eq!(schema["title"], "Custom Console");
    assert_eq!(
        schema["properties"]["task"]["properties"]["request"]["default"],
        "Custom request"
    );

    fs::remove_dir_all(&dir).expect("temp dir should be removed");
}

#[test]
fn mascot_config_centralizes_slime_idle_settings() {
    let mascot = load_mascot_config().expect("bundled mascot config should load");

    assert_eq!(mascot.asset_path, "assets/spirit/slime/slime_idle.png");
    assert_eq!(mascot.frame_count, 4);
    assert_eq!(mascot.fps, 8);
    assert_eq!(mascot.frame_interval_ms(), 125);
}

#[test]
fn app_run_uses_the_chat_runtime_entrypoint() {
    let description = app::runtime_name_for_test();

    assert_eq!(description, "spirit-chat-tui");
}

fn unique_temp_dir() -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    path.push(format!("blazar-config-test-{}-{nanos}", std::process::id()));
    path
}
