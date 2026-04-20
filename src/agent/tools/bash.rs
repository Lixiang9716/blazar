use super::{Tool, ToolResult, ToolSpec};
use nix::errno::Errno;
use nix::libc;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, setsid};
use serde_json::{Value, json};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const MAX_OUTPUT_BYTES: usize = 8 * 1024;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const OUTPUT_TRUNCATED_MARKER: &str = "\n[output truncated]";
const TERM_DRAIN_TIMEOUT: Duration = Duration::from_millis(250);
const TERM_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellConfig {
    shell_path: PathBuf,
    default_timeout: Duration,
}

impl ShellConfig {
    pub fn new(shell_path: impl Into<PathBuf>, default_timeout: Duration) -> Self {
        Self {
            shell_path: shell_path.into(),
            default_timeout,
        }
    }

    pub fn detect() -> Self {
        Self::detect_from(std::env::var_os("SHELL").map(PathBuf::from), |path| {
            path.exists()
        })
    }

    pub fn detect_from<F>(env_shell: Option<PathBuf>, path_exists: F) -> Self
    where
        F: Fn(&Path) -> bool,
    {
        let shell_path = select_shell_path(env_shell, path_exists);
        Self::new(shell_path, Duration::from_secs(DEFAULT_TIMEOUT_SECS))
    }

    pub fn shell_path(&self) -> &Path {
        &self.shell_path
    }

    pub fn default_timeout(&self) -> Duration {
        self.default_timeout
    }
}

fn select_shell_path<F>(env_shell: Option<PathBuf>, path_exists: F) -> PathBuf
where
    F: Fn(&Path) -> bool,
{
    env_shell
        .filter(|candidate| path_exists(candidate))
        .or_else(|| {
            ["/bin/bash", "/bin/sh"]
                .into_iter()
                .map(PathBuf::from)
                .find(|candidate| path_exists(candidate))
        })
        .unwrap_or_else(|| PathBuf::from("/bin/sh"))
}

#[derive(Debug, Clone)]
pub struct BashRequest<'a> {
    pub shell_path: &'a Path,
    pub workspace_root: &'a Path,
    pub command: &'a str,
    pub timeout: Duration,
}

pub trait ProcessRunner: Send + Sync {
    fn run(&self, request: BashRequest<'_>) -> ToolResult;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBashRunner;

pub struct BashTool<R = SystemBashRunner> {
    workspace_root: PathBuf,
    shell: ShellConfig,
    runner: R,
}

impl BashTool<SystemBashRunner> {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self::with_runner(workspace_root, ShellConfig::detect(), SystemBashRunner)
    }
}

impl<R> BashTool<R> {
    pub fn with_runner(workspace_root: PathBuf, shell: ShellConfig, runner: R) -> Self {
        Self {
            workspace_root,
            shell,
            runner,
        }
    }
}

impl<R> Tool for BashTool<R>
where
    R: ProcessRunner,
{
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bash".into(),
            description: "Run a shell command in the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_secs": { "type": "integer", "minimum": 1 }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(command) = args.get("command").and_then(Value::as_str) else {
            return ToolResult::failure("bash requires a string command");
        };

        let timeout = match parse_timeout(&args, self.shell.default_timeout()) {
            Ok(timeout) => timeout,
            Err(error) => return ToolResult::failure(error),
        };

        self.runner.run(BashRequest {
            shell_path: self.shell.shell_path(),
            workspace_root: &self.workspace_root,
            command,
            timeout,
        })
    }
}

fn parse_timeout(args: &Value, default_timeout: Duration) -> Result<Duration, String> {
    let Some(timeout_value) = args.get("timeout_secs") else {
        return Ok(default_timeout);
    };

    let Some(timeout_secs) = timeout_value.as_u64() else {
        return Err("bash timeout_secs must be an integer >= 1".into());
    };

    if timeout_secs == 0 {
        return Err("bash timeout_secs must be an integer >= 1".into());
    }

    Ok(Duration::from_secs(timeout_secs))
}

impl ProcessRunner for SystemBashRunner {
    fn run(&self, request: BashRequest<'_>) -> ToolResult {
        let mut command_builder = Command::new(request.shell_path);
        command_builder
            .arg("-c")
            .arg(request.command)
            .current_dir(request.workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        unsafe {
            command_builder.pre_exec(|| {
                setsid().map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
                if libc::dup2(libc::STDOUT_FILENO, libc::STDERR_FILENO) == -1 {
                    return Err(std::io::Error::from_raw_os_error(Errno::last_raw()));
                }
                Ok(())
            });
        }

        let mut child = match command_builder.spawn() {
            Ok(child) => child,
            Err(error) => return ToolResult::failure(format!("cannot spawn shell: {error}")),
        };
        let process_group = Pid::from_raw(child.id() as i32);

        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return ToolResult::failure("cannot capture shell output");
        };

        let output_reader = thread::spawn(move || read_capped(stdout));
        wait_for_completion(&mut child, process_group, request.timeout, output_reader)
    }
}

fn wait_for_completion(
    child: &mut Child,
    process_group: Pid,
    timeout: Duration,
    output_reader: JoinHandle<std::io::Result<(String, bool)>>,
) -> ToolResult {
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (output, output_truncated) = join_output(output_reader);
                return ToolResult {
                    output,
                    exit_code: status.code(),
                    is_error: !status.success(),
                    output_truncated,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(TERM_POLL_INTERVAL),
            Ok(None) => {
                terminate_process_group(child, process_group);
                let _ = child.wait();
                let (mut output, output_truncated) = join_output(output_reader);
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!("command timed out after {}s", timeout.as_secs()));
                return ToolResult {
                    output,
                    exit_code: None,
                    is_error: true,
                    output_truncated,
                };
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let (mut output, output_truncated) = join_output(output_reader);
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!("wait error: {error}"));
                return ToolResult {
                    output,
                    exit_code: None,
                    is_error: true,
                    output_truncated,
                };
            }
        }
    }
}

fn terminate_process_group(child: &mut Child, process_group: Pid) {
    let _ = killpg(process_group, Signal::SIGTERM);

    let drain_deadline = Instant::now() + TERM_DRAIN_TIMEOUT;
    while Instant::now() < drain_deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(TERM_POLL_INTERVAL),
        }
    }

    let _ = killpg(process_group, Signal::SIGKILL);
}

fn join_output(handle: JoinHandle<std::io::Result<(String, bool)>>) -> (String, bool) {
    match handle.join() {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => (format!("cannot read shell output: {error}"), false),
        Err(_) => (
            "cannot read shell output: reader thread panicked".into(),
            false,
        ),
    }
}

fn read_capped<R: Read>(mut reader: R) -> std::io::Result<(String, bool)> {
    let mut output = Vec::new();
    let mut buf = [0u8; 1024];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }

        let remaining = MAX_OUTPUT_BYTES.saturating_sub(output.len());
        if remaining > 0 {
            let keep = remaining.min(read);
            output.extend_from_slice(&buf[..keep]);
        }
        if read > remaining {
            truncated = true;
        }
    }

    let mut text = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        text.push_str(OUTPUT_TRUNCATED_MARKER);
    }

    Ok((text, truncated))
}
