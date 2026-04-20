use blazar::agent::tools::Tool;
use blazar::agent::tools::bash::{
    BashRequest, BashTool, MAX_OUTPUT_BYTES, ProcessRunner, ShellConfig, SystemBashRunner,
};
use serde_json::json;
#[cfg(target_os = "linux")]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(target_os = "linux")]
fn thread_count() -> usize {
    fs::read_dir("/proc/self/task").unwrap().count()
}

fn unique_workspace_path(label: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    manifest_dir()
        .join("target")
        .join("test-workspaces")
        .join(format!("blazar-bash-{label}-{suffix}.txt"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedRequest {
    shell_path: PathBuf,
    workspace_root: PathBuf,
    command: String,
    timeout: Duration,
}

#[derive(Clone)]
struct RecordingRunner {
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    result: blazar::agent::tools::ToolResult,
}

impl RecordingRunner {
    fn new(result: blazar::agent::tools::ToolResult) -> Self {
        Self {
            recorded: Arc::new(Mutex::new(Vec::new())),
            result,
        }
    }

    fn recorded(&self) -> Arc<Mutex<Vec<RecordedRequest>>> {
        Arc::clone(&self.recorded)
    }
}

impl ProcessRunner for RecordingRunner {
    fn run(&self, request: BashRequest<'_>) -> blazar::agent::tools::ToolResult {
        self.recorded.lock().unwrap().push(RecordedRequest {
            shell_path: request.shell_path.to_path_buf(),
            workspace_root: request.workspace_root.to_path_buf(),
            command: request.command.to_string(),
            timeout: request.timeout,
        });
        self.result.clone()
    }
}

#[test]
fn bash_tool_captures_stdout_and_exit_code() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "printf 'hello from bash'",
        "timeout_secs": 5
    }));

    assert!(!result.is_error);
    assert_eq!(result.output, "hello from bash");
    assert_eq!(result.exit_code, Some(0));
}

#[test]
fn bash_tool_captures_stderr_with_stdout() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "echo stdout; echo stderr >&2",
        "timeout_secs": 5
    }));

    assert!(!result.is_error);
    assert!(result.output.contains("stdout"));
    assert!(result.output.contains("stderr"));
    assert_eq!(result.exit_code, Some(0));
}

#[test]
fn bash_tool_captures_shell_syntax_errors() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "(",
        "timeout_secs": 5
    }));

    assert!(result.is_error);
    assert!(result.output.contains("syntax error") || result.output.contains("unexpected end"));
    assert_eq!(result.exit_code, Some(2));
}

#[test]
fn bash_tool_non_zero_exit_marks_error() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "exit 42",
        "timeout_secs": 5
    }));

    assert!(result.is_error);
    assert_eq!(result.exit_code, Some(42));
}

#[test]
fn bash_tool_truncates_large_output() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "head -c 9000 </dev/zero | tr '\\0' x",
        "timeout_secs": 5
    }));

    assert!(!result.is_error);
    assert!(result.output_truncated);
    assert!(result.output.contains("[output truncated]"));
    assert!(result.output.len() <= MAX_OUTPUT_BYTES + 32);
}

#[test]
fn bash_tool_caps_combined_stdout_and_stderr_output() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "head -c 5000 </dev/zero | tr '\\0' o; head -c 5000 </dev/zero | tr '\\0' e >&2",
        "timeout_secs": 5
    }));

    assert!(!result.is_error);
    assert!(result.output_truncated);
    assert!(result.output.contains("[output truncated]"));
    assert!(result.output.len() <= MAX_OUTPUT_BYTES + 32);
}

#[test]
fn bash_tool_times_out_and_returns_error() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "sleep 2",
        "timeout_secs": 1
    }));

    assert!(result.is_error);
    assert_eq!(result.exit_code, None);
    assert!(result.output.contains("timed out"));
}

#[test]
fn bash_tool_timeout_stays_bounded_when_descendant_escapes_process_group() {
    let tool = BashTool::new(manifest_dir());
    let started = Instant::now();
    let result = tool.execute(json!({
        "command": "setsid sh -c 'sleep 5' & sleep 5",
        "timeout_secs": 1
    }));

    assert!(result.is_error);
    assert!(result.output.contains("timed out"));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout took {:?}",
        started.elapsed()
    );
}

#[test]
fn bash_tool_timeout_stays_bounded_with_unbounded_output() {
    let tool = BashTool::new(manifest_dir());
    let started = Instant::now();
    let result = tool.execute(json!({
        "command": "python - <<'PY'\nimport os, time\nend = time.time() + 2\nchunk = b'x' * 65536\nwhile time.time() < end:\n    os.write(1, chunk)\nPY",
        "timeout_secs": 1
    }));

    assert!(result.is_error);
    assert!(result.output.contains("timed out"));
    assert!(result.output_truncated);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout took {:?}",
        started.elapsed()
    );
}

#[test]
fn bash_tool_timeout_kills_same_group_child_that_ignores_sigterm() {
    let output_path = unique_workspace_path("sigterm-ignored");
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let _ = fs::remove_file(&output_path);

    let command = format!(
        "sh -c 'trap \"\" TERM; sleep 2; printf lingering > \"{}\"' & sleep 5",
        output_path.display()
    );
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": command,
        "timeout_secs": 1
    }));

    assert!(result.is_error);
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !output_path.exists(),
        "same-group child survived timeout cleanup"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn bash_tool_timeout_does_not_leave_blocked_reader_thread() {
    let before = thread_count();
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "setsid sh -c 'sleep 2' & sleep 2",
        "timeout_secs": 1
    }));

    assert!(result.is_error);
    assert!(
        thread_count() <= before,
        "thread count before={before} after={}",
        thread_count()
    );
}

#[test]
fn bash_tool_uses_noninteractive_stdin() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "if read value; then printf '%s' \"${value:-empty}\"; else printf eof; fi",
        "timeout_secs": 1
    }));

    assert!(!result.is_error);
    assert_eq!(result.output, "eof");
}

#[test]
fn bash_tool_missing_command_returns_error() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "timeout_secs": 5
    }));

    assert!(result.is_error);
    assert!(result.output.contains("requires a string command"));
}

#[test]
fn bash_tool_rejects_invalid_timeout_argument() {
    let tool = BashTool::new(manifest_dir());
    let result = tool.execute(json!({
        "command": "echo ok",
        "timeout_secs": 0
    }));

    assert!(result.is_error);
    assert!(result.output.contains("timeout_secs"));
}

#[test]
fn bash_tool_uses_workspace_as_current_dir() {
    let workspace = manifest_dir();
    let tool = BashTool::new(workspace.clone());
    let result = tool.execute(json!({
        "command": "pwd",
        "timeout_secs": 5
    }));

    assert!(!result.is_error);
    assert_eq!(PathBuf::from(result.output.trim()), workspace);
}

#[test]
fn bash_tool_uses_default_timeout_when_not_specified() {
    let runner = RecordingRunner::new(blazar::agent::tools::ToolResult::success("ok"));
    let recorded = runner.recorded();
    let tool = BashTool::with_runner(
        manifest_dir(),
        ShellConfig::new("/bin/sh", Duration::from_secs(30)),
        runner,
    );

    let result = tool.execute(json!({
        "command": "echo ok"
    }));

    assert!(!result.is_error);
    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].timeout, Duration::from_secs(30));
}

#[test]
fn shell_config_prefers_env_shell_when_present() {
    let config = ShellConfig::detect_from(Some(PathBuf::from("/custom-shell")), |path| {
        path == Path::new("/custom-shell")
    });

    assert_eq!(config.shell_path(), Path::new("/custom-shell"));
}

#[test]
fn shell_config_ignores_missing_env_shell_and_falls_back() {
    let config = ShellConfig::detect_from(Some(PathBuf::from("/missing-shell")), |path| {
        path == Path::new("/bin/bash")
    });

    assert_eq!(config.shell_path(), Path::new("/bin/bash"));
}

#[test]
fn shell_config_falls_back_to_bash_then_sh() {
    let bash = ShellConfig::detect_from(None, |path| path == Path::new("/bin/bash"));
    let sh = ShellConfig::detect_from(None, |path| path == Path::new("/bin/sh"));

    assert_eq!(bash.shell_path(), Path::new("/bin/bash"));
    assert_eq!(sh.shell_path(), Path::new("/bin/sh"));
}

#[test]
fn bash_tool_spec_requires_command() {
    let tool = BashTool::<SystemBashRunner>::new(manifest_dir());
    let spec = tool.spec();

    assert_eq!(spec.name, "bash");
    assert!(!spec.description.is_empty());

    let params = spec.parameters.as_object().unwrap();
    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::Value::String("command".to_string())));
}
