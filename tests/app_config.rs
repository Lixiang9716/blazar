use blazar::app;
use blazar::config::{
    APP_SCHEMA_PATH, load_app_schema, load_app_schema_from_path, load_mascot_config,
    load_mascot_config_from_path, schema_title,
};
use serde_json::json;
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

#[test]
fn mascot_config_validation_reports_schema_errors() {
    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("temp dir should be created");
    let path = dir.join("app.json");

    fs::write(
        &path,
        r#"{
            "title": "Custom Console",
            "mascot": {
                "assetPath": "assets/spirit.png",
                "frameCount": 0,
                "fps": 8
            }
        }"#,
    )
    .expect("temp config should be written");
    let err = load_mascot_config_from_path(&path).expect_err("frameCount=0 should fail");
    assert!(
        err.to_string()
            .contains("frameCount must be greater than 0")
    );

    fs::write(
        &path,
        r#"{
            "title": "Custom Console",
            "mascot": {
                "assetPath": "assets/spirit.png",
                "frameCount": 4,
                "fps": 0
            }
        }"#,
    )
    .expect("temp config should be written");
    let err = load_mascot_config_from_path(&path).expect_err("fps=0 should fail");
    assert!(err.to_string().contains("fps must be greater than 0"));

    fs::remove_dir_all(&dir).expect("temp dir should be removed");
}

#[test]
fn schema_title_and_loader_error_paths_are_descriptive() {
    let missing = PathBuf::from("config/does-not-exist.json");
    let missing_err = load_app_schema_from_path(&missing).expect_err("missing file should fail");
    assert!(
        missing_err
            .to_string()
            .contains("failed to read config file")
    );

    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("temp dir should be created");
    let path = dir.join("broken.json");
    fs::write(&path, "{not-json").expect("broken config should be written");
    let parse_err = load_app_schema_from_path(&path).expect_err("invalid json should fail");
    assert!(
        parse_err
            .to_string()
            .contains("failed to parse config file")
    );

    let no_title = json!({"mascot":{"assetPath":"a","frameCount":1,"fps":1}});
    let title_err = schema_title(&no_title).expect_err("missing title should fail");
    assert!(
        title_err
            .to_string()
            .contains("schema title must be a string")
    );

    fs::remove_dir_all(&dir).expect("temp dir should be removed");
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
