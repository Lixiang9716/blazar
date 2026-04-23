use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_log_file(prefix: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("observability-script-tests");
    std::fs::create_dir_all(&base).expect("should create test log directory");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    base.join(format!("{prefix}-{nanos}.log"))
}

fn run_script(script_name: &str, args: &[&str]) -> Output {
    run_script_with_env(script_name, args, &[])
}

fn run_script_with_env(script_name: &str, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = repo_root
        .join("scripts")
        .join("observability")
        .join(script_name);
    let bash_path = if Path::new("/usr/bin/bash").exists() {
        "/usr/bin/bash"
    } else {
        "/bin/bash"
    };

    Command::new(bash_path)
        .arg(script_path)
        .args(args)
        .envs(envs.iter().copied())
        .current_dir(repo_root)
        .output()
        .expect("script process should execute")
}

fn run_just(args: &[&str]) -> Output {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    Command::new("just")
        .args(args)
        .current_dir(repo_root)
        .output()
        .expect("just command should execute")
}

fn just_is_available() -> bool {
    static JUST_AVAILABLE: OnceLock<bool> = OnceLock::new();
    *JUST_AVAILABLE.get_or_init(|| Command::new("just").arg("--version").output().is_ok())
}

#[test]
fn logs_errors_filters_warn_and_error_levels() {
    let log_file = unique_log_file("logs-errors");
    std::fs::write(
        &log_file,
        concat!(
            "{\"level\":\"INFO\",\"message\":\"startup\"}\n",
            "{\"level\":\"warn\",\"message\":\"slow request lower\"}\n",
            "{\"level\":\"WARN\",\"message\":\"slow request\"}\n",
            "{\"level\":\"error\",\"message\":\"tool failed lower\"}\n",
            "{\"level\":\"ERROR\",\"message\":\"tool failed\"}\n",
        ),
    )
    .expect("should write test log file");

    let output = run_script(
        "logs-errors.sh",
        &[log_file.to_str().expect("utf-8 log file path")],
    );

    assert!(
        output.status.success(),
        "script should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "expected warn/error records regardless of case"
    );

    let mut messages = Vec::new();
    for line in lines {
        let record: Value = serde_json::from_str(line).expect("line should be valid json");
        let level = record
            .get("level")
            .and_then(Value::as_str)
            .expect("record should include string level");
        assert!(
            matches!(level.to_ascii_uppercase().as_str(), "WARN" | "ERROR"),
            "unexpected level in output: {level}"
        );
        messages.push(
            record
                .get("message")
                .and_then(Value::as_str)
                .expect("record should include string message")
                .to_string(),
        );
    }
    assert!(
        messages.iter().any(|msg| msg == "slow request lower"),
        "lowercase warn event should be preserved"
    );
    assert!(
        messages.iter().any(|msg| msg == "tool failed lower"),
        "lowercase error event should be preserved"
    );
}

#[test]
fn logs_turn_filters_by_turn_id() {
    let log_file = unique_log_file("logs-turn");
    std::fs::write(
        &log_file,
        concat!(
            "{\"level\":\"INFO\",\"turn_id\":\"turn-1\",\"message\":\"a\"}\n",
            "{\"level\":\"ERROR\",\"turn_id\":\"turn-2\",\"message\":\"b\"}\n",
            "{\"level\":\"WARN\",\"turn_id\":\"turn-1\",\"message\":\"c\"}\n",
        ),
    )
    .expect("should write test log file");

    let output = run_script(
        "logs-turn.sh",
        &[
            "  turn-1  ",
            log_file.to_str().expect("utf-8 log file path"),
        ],
    );

    assert!(
        output.status.success(),
        "script should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "expected only turn-1 records");

    let mut messages = Vec::new();
    for line in lines {
        let record: Value = serde_json::from_str(line).expect("line should be valid json");
        let turn_id = record
            .get("turn_id")
            .and_then(Value::as_str)
            .expect("record should include string turn_id");
        assert_eq!(turn_id, "turn-1");
        messages.push(
            record
                .get("message")
                .and_then(Value::as_str)
                .expect("record should include string message")
                .to_string(),
        );
    }
    assert_eq!(messages, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn logs_errors_missing_log_file_exits_with_code_2() {
    let missing = unique_log_file("missing-errors");
    let output = run_script(
        "logs-errors.sh",
        &[missing.to_str().expect("utf-8 missing path")],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("log file not found"),
        "stderr should explain missing log file"
    );
}

#[test]
fn logs_turn_missing_log_file_exits_with_code_2() {
    let missing = unique_log_file("missing-turn");
    let output = run_script(
        "logs-turn.sh",
        &["turn-1", missing.to_str().expect("utf-8 missing path")],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("log file not found"),
        "stderr should explain missing log file"
    );
}

#[test]
fn logs_tail_missing_log_file_exits_with_code_2() {
    let missing = unique_log_file("missing-tail");
    let output = run_script(
        "logs-tail.sh",
        &[missing.to_str().expect("utf-8 missing path")],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("log file not found"),
        "stderr should explain missing log file"
    );
}

#[test]
fn logs_errors_fails_on_malformed_json_with_readable_message() {
    let log_file = unique_log_file("malformed-errors");
    std::fs::write(&log_file, "{this is not json}\n").expect("should write malformed log file");

    let output = run_script(
        "logs-errors.sh",
        &[log_file.to_str().expect("utf-8 log file path")],
    );

    assert!(!output.status.success(), "malformed json should fail");
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(
        stderr.contains("json") || stderr.contains("parse"),
        "stderr should contain parser diagnostics, got: {stderr}"
    );
}

#[test]
fn logs_turn_fails_on_malformed_json_with_readable_message() {
    let log_file = unique_log_file("malformed-turn");
    std::fs::write(&log_file, "{still not json}\n").expect("should write malformed log file");

    let output = run_script(
        "logs-turn.sh",
        &["turn-1", log_file.to_str().expect("utf-8 log file path")],
    );

    assert!(!output.status.success(), "malformed json should fail");
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(
        stderr.contains("json") || stderr.contains("parse"),
        "stderr should contain parser diagnostics, got: {stderr}"
    );
}

#[test]
fn logs_errors_empty_log_returns_no_matches() {
    let log_file = unique_log_file("empty-errors");
    std::fs::write(&log_file, "").expect("should write empty log file");

    let output = run_script(
        "logs-errors.sh",
        &[log_file.to_str().expect("utf-8 log file path")],
    );

    assert!(
        output.status.success(),
        "empty log should not fail, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "empty log should return no matches, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn logs_turn_empty_log_returns_no_matches() {
    let log_file = unique_log_file("empty-turn");
    std::fs::write(&log_file, "").expect("should write empty log file");

    let output = run_script(
        "logs-turn.sh",
        &["turn-1", log_file.to_str().expect("utf-8 log file path")],
    );

    assert!(
        output.status.success(),
        "empty log should not fail, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "empty log should return no matches, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn just_logs_errors_accepts_log_file_paths_with_spaces() {
    if !just_is_available() {
        eprintln!("skipping: just is not installed");
        return;
    }

    let log_file = unique_log_file("just-errors-with-space");
    let parent = log_file.parent().expect("log file should have parent");
    let spaced_dir = parent.join("space dir");
    std::fs::create_dir_all(&spaced_dir).expect("should create spaced directory");
    let spaced_log = spaced_dir.join("errors log.jsonl");
    std::fs::write(
        &spaced_log,
        "{\"level\":\"ERROR\",\"message\":\"spaced path should work\"}\n",
    )
    .expect("should write spaced log file");

    let output = run_just(&[
        "logs-errors",
        spaced_log.to_str().expect("utf-8 spaced log path"),
    ]);

    assert!(
        output.status.success(),
        "just logs-errors should support spaced paths, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn just_logs_turn_accepts_spaced_turn_id_and_log_path() {
    if !just_is_available() {
        eprintln!("skipping: just is not installed");
        return;
    }

    let log_file = unique_log_file("just-turn-with-space");
    let parent = log_file.parent().expect("log file should have parent");
    let spaced_dir = parent.join("turn space dir");
    std::fs::create_dir_all(&spaced_dir).expect("should create spaced directory");
    let spaced_log = spaced_dir.join("turn log.jsonl");
    std::fs::write(
        &spaced_log,
        "{\"level\":\"WARN\",\"turn_id\":\"turn alpha\",\"message\":\"x\"}\n",
    )
    .expect("should write spaced log file");

    let output = run_just(&[
        "logs-turn",
        "turn alpha",
        spaced_log.to_str().expect("utf-8 spaced log path"),
    ]);

    assert!(
        output.status.success(),
        "just logs-turn should support spaced args, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn install_tools_script_reports_missing_tools_in_check_mode() {
    let fake_path = unique_log_file("install-tools-check-path");
    std::fs::create_dir_all(&fake_path).expect("should create fake PATH directory");
    let fake_path_text = fake_path.to_str().expect("utf-8 fake PATH");

    let output = run_script_with_env(
        "install-tools.sh",
        &[],
        &[("CHECK_ONLY", "1"), ("PATH", fake_path_text)],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "check-only mode should return success while reporting missing tools"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Missing tools: jq lnav fzf"),
        "stdout should list missing tools, got: {stdout}"
    );
    assert!(
        stdout.contains("Package manager: none detected"),
        "stdout should report package manager detection, got: {stdout}"
    );
    assert!(
        stdout.contains("Check-only mode: no installation attempted."),
        "stdout should mention check-only behavior, got: {stdout}"
    );
    assert!(
        stdout.contains("Action: rerun with '--install' to attempt auto-install when supported."),
        "stdout should provide explicit next-step for install mode, got: {stdout}"
    );
}

#[test]
fn install_tools_script_defaults_to_check_mode() {
    let fake_path = unique_log_file("install-tools-default-check-path");
    std::fs::create_dir_all(&fake_path).expect("should create fake PATH directory");
    let fake_path_text = fake_path.to_str().expect("utf-8 fake PATH");

    let output = run_script_with_env("install-tools.sh", &[], &[("PATH", fake_path_text)]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "default mode should be safe check-only"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Check-only mode: no installation attempted."),
        "default mode should not attempt installation, got: {stdout}"
    );
}

#[test]
fn install_tools_script_rejects_invalid_arguments() {
    let output = run_script("install-tools.sh", &["--invalid"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid arguments should return usage error"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Usage: install-tools.sh"),
        "stderr should show usage for invalid arguments"
    );
}

#[test]
fn install_tools_script_blocks_install_attempts_in_ci_without_opt_in() {
    let fake_path = unique_log_file("install-tools-ci-path");
    std::fs::create_dir_all(&fake_path).expect("should create fake PATH directory");
    let fake_path_text = fake_path.to_str().expect("utf-8 fake PATH");

    let output = run_script_with_env(
        "install-tools.sh",
        &["--install"],
        &[("CI", "true"), ("PATH", fake_path_text)],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "CI protection should block install mode unless explicitly allowed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("CI protection: refusing automatic installation."),
        "stdout should explain CI safety guard, got: {stdout}"
    );
    assert!(
        stdout.contains("Action: set ALLOW_INSTALL_IN_CI=1 to permit --install in CI."),
        "stdout should explain explicit CI opt-in, got: {stdout}"
    );
}
