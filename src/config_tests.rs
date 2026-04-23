use super::*;
use std::error::Error as _;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(prefix: &str, extension: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}.{extension}",
        std::process::id()
    ))
}

fn write_temp_file(prefix: &str, extension: &str, content: &str) -> PathBuf {
    let path = temp_path(prefix, extension);
    fs::write(&path, content).expect("temp file should be writable");
    path
}

#[test]
fn mascot_frame_interval_ms_uses_fps() {
    let cfg = MascotConfig {
        asset_path: "assets/mascot.riv".to_owned(),
        frame_count: 8,
        fps: 20,
    };
    assert_eq!(cfg.frame_interval_ms(), 50);
}

#[test]
fn load_app_schema_from_path_reads_valid_json() {
    let path = write_temp_file(
        "app-schema",
        "json",
        r#"{"title":"Blazar App","mascot":{"assetPath":"a","frameCount":1,"fps":1}}"#,
    );

    let schema = load_app_schema_from_path(&path).expect("schema should load");
    assert_eq!(schema["title"], "Blazar App");

    let _ = fs::remove_file(path);
}

#[test]
fn load_app_schema_from_path_reports_parse_error() {
    let path = write_temp_file("app-schema-invalid", "json", "{invalid");

    let err = load_app_schema_from_path(&path).expect_err("invalid json should fail");
    match err {
        ConfigError::Parse { path: err_path, .. } => assert_eq!(err_path, path),
        other => panic!("unexpected error variant: {other:?}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn load_app_schema_from_path_reports_read_error() {
    let missing = temp_path("missing-app-schema", "json");
    let err = load_app_schema_from_path(&missing).expect_err("missing file should fail");
    match err {
        ConfigError::Read { path, .. } => assert_eq!(path, missing),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn schema_title_requires_string() {
    let schema = serde_json::json!({ "title": 123 });
    let err = schema_title(&schema).expect_err("non-string title should fail");
    match err {
        ConfigError::InvalidSchema { message, .. } => {
            assert_eq!(message, "schema title must be a string");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn load_mascot_config_from_path_reads_valid_schema() {
    let path = write_temp_file(
        "mascot-schema",
        "json",
        r#"{"title":"Blazar","mascot":{"assetPath":"assets/m.riv","frameCount":12,"fps":24}}"#,
    );

    let cfg = load_mascot_config_from_path(&path).expect("mascot config should load");
    assert_eq!(cfg.asset_path, "assets/m.riv");
    assert_eq!(cfg.frame_count, 12);
    assert_eq!(cfg.fps, 24);

    let _ = fs::remove_file(path);
}

#[test]
fn load_mascot_config_from_path_rejects_zero_frame_count() {
    let path = write_temp_file(
        "mascot-zero-frame",
        "json",
        r#"{"title":"Blazar","mascot":{"assetPath":"assets/m.riv","frameCount":0,"fps":24}}"#,
    );

    let err = load_mascot_config_from_path(&path).expect_err("zero frame count should fail");
    match err {
        ConfigError::InvalidSchema { message, .. } => {
            assert_eq!(message, "mascot.frameCount must be greater than 0");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn load_mascot_config_from_path_rejects_zero_fps() {
    let path = write_temp_file(
        "mascot-zero-fps",
        "json",
        r#"{"title":"Blazar","mascot":{"assetPath":"assets/m.riv","frameCount":1,"fps":0}}"#,
    );

    let err = load_mascot_config_from_path(&path).expect_err("zero fps should fail");
    match err {
        ConfigError::InvalidSchema { message, .. } => {
            assert_eq!(message, "mascot.fps must be greater than 0");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn parse_agents_config_reads_and_trims_values() {
    let path = Path::new("config/agents.toml");
    let contents = r#"
        [[agents]]
        name = "  alpha  "
        endpoint = " http://localhost:9001 "
        agent_id = " agent-alpha "

        [discovery]
        endpoints = [" http://localhost:9100 ", "http://localhost:9200"]
    "#;

    let parsed = parse_agents_config(path, contents).expect("agents config should parse");
    assert_eq!(parsed.agents.len(), 1);
    assert_eq!(parsed.agents[0].name, "alpha");
    assert_eq!(parsed.agents[0].endpoint, "http://localhost:9001");
    assert_eq!(parsed.agents[0].agent_id, "agent-alpha");
    assert!(parsed.agents[0].enabled);
    assert_eq!(parsed.discovery.endpoints.len(), 2);
    assert_eq!(parsed.discovery.endpoints[0], "http://localhost:9100");
}

#[test]
fn parse_agents_config_rejects_empty_agent_fields() {
    let path = Path::new("config/agents.toml");
    let cases = [
        (
            r#"
            [[agents]]
            name = " "
            endpoint = "http://localhost:9001"
            agent_id = "agent"
            "#,
            "agents[0].name must not be empty",
        ),
        (
            r#"
            [[agents]]
            name = "alpha"
            endpoint = " "
            agent_id = "agent"
            "#,
            "agents[0].endpoint must not be empty",
        ),
        (
            r#"
            [[agents]]
            name = "alpha"
            endpoint = "http://localhost:9001"
            agent_id = " "
            "#,
            "agents[0].agent_id must not be empty",
        ),
    ];

    for (contents, expected_message) in cases {
        let err = parse_agents_config(path, contents).expect_err("invalid agent should fail");
        match err {
            AgentConfigError::InvalidConfig { message, .. } => {
                assert_eq!(message, expected_message);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}

#[test]
fn parse_agents_config_rejects_empty_discovery_endpoint() {
    let path = Path::new("config/agents.toml");
    let contents = r#"
        [discovery]
        endpoints = [" "]
    "#;

    let err = parse_agents_config(path, contents).expect_err("invalid discovery should fail");
    match err {
        AgentConfigError::InvalidConfig { message, .. } => {
            assert_eq!(message, "discovery.endpoints[0] must not be empty");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn load_agents_config_from_path_reports_parse_error() {
    let path = write_temp_file("agents-invalid", "toml", "[[agents]]\nname =");
    let err = load_agents_config_from_path(&path).expect_err("invalid toml should fail");

    match err {
        AgentConfigError::Parse { path: err_path, .. } => assert_eq!(err_path, path),
        other => panic!("unexpected error variant: {other:?}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn config_errors_expose_sources() {
    let missing = temp_path("missing-config", "json");
    let read_err = load_app_schema_from_path(&missing).expect_err("missing file should fail");
    assert!(read_err.source().is_some());

    let parse_path = write_temp_file("schema-parse-source", "json", "{bad json");
    let parse_err = load_app_schema_from_path(&parse_path).expect_err("invalid json should fail");
    assert!(parse_err.source().is_some());
    let _ = fs::remove_file(parse_path);
}

#[test]
fn agent_config_errors_expose_sources() {
    let missing = temp_path("missing-agents", "toml");
    let read_err = load_agents_config_from_path(&missing).expect_err("missing file should fail");
    assert!(read_err.source().is_some());
}
