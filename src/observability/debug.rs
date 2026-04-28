use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DebugEventSnapshot {
    pub turn_id: Option<String>,
    pub turn_kind: Option<String>,
    pub tool_name: Option<String>,
    pub call_id: Option<String>,
    pub error_kind: Option<String>,
    pub session_id: String,
    pub workspace_path: String,
    pub queue_depth: u64,
    pub event_seq: Option<i64>,
}

#[derive(Debug)]
pub struct DebugRecorder {
    db_path: PathBuf,
    session_id: String,
    workspace_path: String,
    current_turn_id: Option<String>,
    current_turn_kind: Option<String>,
    current_event_seq: i64,
    active_call_id: Option<String>,
    latest_turn_id: Option<String>,
    latest_error_kind: Option<String>,
    tokio_console_enabled: bool,
}

impl DebugRecorder {
    pub fn new(workspace_root: &Path) -> Self {
        let log_dir = workspace_root.join("logs");
        let _ = std::fs::create_dir_all(&log_dir);

        let db_path = log_dir.join("blazar-debug.sqlite");
        let session_id = format!(
            "session-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default()
        );
        let workspace_path = workspace_root.display().to_string();
        let tokio_console_enabled = std::env::var_os("BLAZAR_ENABLE_TOKIO_CONSOLE").is_some();

        let recorder = Self {
            db_path,
            session_id,
            workspace_path,
            current_turn_id: None,
            current_turn_kind: None,
            current_event_seq: 0,
            active_call_id: None,
            latest_turn_id: None,
            latest_error_kind: None,
            tokio_console_enabled,
        };
        recorder.init_schema();
        // Propagate session context to the global logger so even plain log!()
        // calls include session_id and workspace_path.
        crate::observability::logging::set_global_log_context(
            &recorder.session_id,
            &recorder.workspace_path,
        );
        recorder
    }

    pub fn start_turn(&mut self, turn_id: &str, turn_kind: Option<&str>, queue_depth: usize) {
        self.current_turn_id = Some(turn_id.to_owned());
        self.current_turn_kind = turn_kind.map(str::to_owned);
        self.current_event_seq = 0;
        self.active_call_id = None;
        self.latest_turn_id = Some(turn_id.to_owned());
        self.latest_error_kind = None;

        if let Ok(connection) = self.connection() {
            let _ = connection.execute(
                "INSERT OR REPLACE INTO debug_turns (
                    session_id, turn_id, turn_kind, status, queue_depth, started_at, finished_at, error_kind, error_message
                ) VALUES (?1, ?2, ?3, 'streaming', ?4, ?5, NULL, NULL, NULL)",
                params![
                    &self.session_id,
                    turn_id,
                    turn_kind,
                    queue_depth as i64,
                    timestamp_seconds()
                ],
            );
        }
    }

    pub fn record_event(
        &mut self,
        event_name: &str,
        tool_name: Option<&str>,
        call_id: Option<&str>,
        error_kind: Option<&str>,
        queue_depth: usize,
        message: &str,
    ) -> DebugEventSnapshot {
        if event_name == "tool_call_started" {
            self.active_call_id = call_id.map(str::to_owned);
        } else if event_name == "tool_call_completed" && self.active_call_id.as_deref() == call_id {
            self.active_call_id = None;
        }

        if event_name == "turn_failed" {
            self.latest_error_kind = error_kind.map(str::to_owned);
        }

        let event_seq = self.current_turn_id.as_ref().map(|_| {
            self.current_event_seq = self.current_event_seq.saturating_add(1);
            self.current_event_seq
        });

        if let Ok(connection) = self.connection() {
            let _ = connection.execute(
                "INSERT INTO debug_events (
                    session_id, turn_id, turn_kind, event_seq, event_name, tool_name, call_id, error_kind, queue_depth, message, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &self.session_id,
                    self.current_turn_id.as_deref(),
                    self.current_turn_kind.as_deref(),
                    event_seq,
                    event_name,
                    tool_name,
                    call_id,
                    error_kind,
                    queue_depth as i64,
                    message,
                    timestamp_seconds()
                ],
            );
        }

        DebugEventSnapshot {
            turn_id: self.current_turn_id.clone(),
            turn_kind: self.current_turn_kind.clone(),
            tool_name: tool_name.map(str::to_owned),
            call_id: call_id.map(str::to_owned),
            error_kind: error_kind.map(str::to_owned),
            session_id: self.session_id.clone(),
            workspace_path: self.workspace_path.clone(),
            queue_depth: queue_depth as u64,
            event_seq,
        }
    }

    pub fn finish_turn(
        &mut self,
        status: &str,
        error_kind: Option<&str>,
        error_message: Option<&str>,
    ) {
        let Some(turn_id) = self.current_turn_id.clone() else {
            return;
        };

        if let Ok(connection) = self.connection() {
            let _ = connection.execute(
                "UPDATE debug_turns
                 SET status = ?3, finished_at = ?4, error_kind = ?5, error_message = ?6
                 WHERE session_id = ?1 AND turn_id = ?2",
                params![
                    &self.session_id,
                    &turn_id,
                    status,
                    timestamp_seconds(),
                    error_kind,
                    error_message
                ],
            );
        }

        self.current_turn_id = None;
        self.current_turn_kind = None;
        self.current_event_seq = 0;
        self.active_call_id = None;
    }

    pub fn latest_turn_bundle(&self) -> Option<String> {
        self.latest_turn_id
            .as_deref()
            .map(|turn_id| self.bundle_for_turn(turn_id))
    }

    pub fn status_summary(&self, pending_count: usize) -> String {
        if pending_count == 0
            && self.current_turn_id.is_none()
            && self.active_call_id.is_none()
            && self.latest_error_kind.is_none()
            && !self.tokio_console_enabled
        {
            return String::new();
        }

        let mut parts = vec![format!("dbg q{pending_count}")];

        if let Some(turn_id) = self.current_turn_id.as_deref() {
            parts.push(format!("turn:{turn_id}"));
        }
        if let Some(call_id) = self.active_call_id.as_deref() {
            parts.push(format!("call:{call_id}"));
        }
        if let Some(error_kind) = self.latest_error_kind.as_deref() {
            parts.push(format!("err:{error_kind}"));
        }
        if self.tokio_console_enabled {
            parts.push("console:on".to_owned());
        }

        parts.join(" ")
    }

    fn bundle_for_turn(&self, turn_id: &str) -> String {
        let mut lines = vec![
            "debug evidence".to_owned(),
            format!("session_id={}", self.session_id),
            format!("workspace_path={}", self.workspace_path),
            format!("db_path={}", self.db_path.display()),
            format!("turn_id={turn_id}"),
        ];

        if let Ok(connection) = self.connection()
            && let Ok(mut statement) = connection.prepare(
                "SELECT COALESCE(event_seq, 0), event_name, tool_name, call_id, error_kind, queue_depth
                 FROM debug_events
                 WHERE session_id = ?1 AND turn_id = ?2
                 ORDER BY COALESCE(event_seq, 0), rowid",
            )
            && let Ok(events) = statement.query_map(params![&self.session_id, turn_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
        {
            for event in events.flatten() {
                let (event_seq, event_name, tool_name, call_id, error_kind, queue_depth) = event;
                let mut line = format!("{event_seq:02} {event_name} q{queue_depth}");
                if let Some(tool_name) = tool_name {
                    line.push_str(&format!(" tool_name={tool_name}"));
                }
                if let Some(call_id) = call_id {
                    line.push_str(&format!(" call_id={call_id}"));
                }
                if let Some(error_kind) = error_kind {
                    line.push_str(&format!(" error_kind={error_kind}"));
                }
                lines.push(line);
            }
        }

        lines.join("\n")
    }

    fn init_schema(&self) {
        if let Ok(connection) = self.connection() {
            let _ = connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS debug_turns (
                    session_id TEXT NOT NULL,
                    turn_id TEXT NOT NULL,
                    turn_kind TEXT,
                    status TEXT NOT NULL,
                    queue_depth INTEGER NOT NULL,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    error_kind TEXT,
                    error_message TEXT,
                    PRIMARY KEY (session_id, turn_id)
                );
                CREATE TABLE IF NOT EXISTS debug_events (
                    session_id TEXT NOT NULL,
                    turn_id TEXT,
                    turn_kind TEXT,
                    event_seq INTEGER,
                    event_name TEXT NOT NULL,
                    tool_name TEXT,
                    call_id TEXT,
                    error_kind TEXT,
                    queue_depth INTEGER NOT NULL,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );",
            );
        }
    }

    fn connection(&self) -> rusqlite::Result<Connection> {
        Connection::open(&self.db_path)
    }
}

fn timestamp_seconds() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .to_string()
}
