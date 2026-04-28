use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;
#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, Transaction, params};
use serde_json::Value;

use super::session::PlanSession;

#[allow(dead_code)]
pub(crate) struct PlanStore {
    workspace_root: PathBuf,
}

#[allow(dead_code)]
impl PlanStore {
    pub(crate) fn new() -> Self {
        let workspace_root =
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(Path::new(".")));
        Self { workspace_root }
    }

    pub(crate) fn for_workspace(path: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: path.as_ref().to_path_buf(),
        }
    }

    pub(crate) fn create_session(&self) -> PlanSession {
        PlanSession::new()
    }

    pub(crate) fn plans_dir(&self) -> PathBuf {
        self.workspace_root.join(".blazar").join("plans")
    }

    fn plan_state_dir(&self) -> PathBuf {
        self.workspace_root.join(".blazar").join("state")
    }

    fn plan_index_path(&self) -> PathBuf {
        self.plan_state_dir().join("plan_index.db")
    }

    fn is_valid_plan_id(plan_id: &str) -> bool {
        !plan_id.is_empty()
            && plan_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }

    fn validate_plan_id(plan_id: &str) -> io::Result<&str> {
        if Self::is_valid_plan_id(plan_id) {
            Ok(plan_id)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid plan id: {plan_id:?}; only [A-Za-z0-9_-] are allowed and ID cannot be empty"
                ),
            ))
        }
    }

    pub(crate) fn plan_json_path(&self, plan_id: &str) -> io::Result<PathBuf> {
        let plan_id = Self::validate_plan_id(plan_id)?;
        Ok(self.plans_dir().join(format!("{plan_id}.json")))
    }

    /// Persists plan session JSON as the source of truth.
    ///
    /// Index rows are derived cache data. Index sync failures are surfaced via
    /// warning logs but do not fail the JSON write contract.
    pub(crate) fn save_session_json(&self, plan_id: &str, session: &PlanSession) -> io::Result<()> {
        let plans_dir = self.plans_dir();
        fs::create_dir_all(&plans_dir)?;
        let json = serde_json::to_string_pretty(session)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let plan_path = self.plan_json_path(plan_id)?;
        fs::write(plan_path, json)?;
        if let Err(error) = self.sync_index_for_plan(plan_id) {
            warn!("plan store: JSON saved for {plan_id}, index sync failed: {error}");
        }
        Ok(())
    }

    pub(crate) fn load_session_json(&self, plan_id: &str) -> io::Result<PlanSession> {
        let plan_path = self.plan_json_path(plan_id)?;
        let json = fs::read_to_string(plan_path)?;
        serde_json::from_str(&json).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    pub(crate) fn list_plan_ids(&self) -> io::Result<Vec<String>> {
        let plans_dir = self.plans_dir();
        if !plans_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for entry_result in fs::read_dir(plans_dir)? {
            let entry = entry_result?;
            let path = entry.path();
            if !path.is_file() || path.extension() != Some(OsStr::new("json")) {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
                && Self::is_valid_plan_id(stem)
            {
                ids.push(stem.to_owned());
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub(crate) fn sync_index_for_plan(&self, plan_id: &str) -> io::Result<()> {
        let plan_id = Self::validate_plan_id(plan_id)?;
        let plan_path = self.plan_json_path(plan_id)?;
        let json = fs::read_to_string(&plan_path)?;
        let payload = parse_plan_payload(&json)?;

        let mut conn = self.open_plan_index()?;
        let tx = conn.transaction().map_err(io::Error::other)?;
        self.replace_index_rows_for_plan(&tx, plan_id, &plan_path, &json, &payload)?;
        tx.commit().map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn rebuild_index_from_json(&self) -> io::Result<()> {
        let plan_ids = self.list_plan_ids()?;
        let mut conn = self.open_plan_index()?;
        let tx = conn.transaction().map_err(io::Error::other)?;
        tx.execute_batch(
            "DELETE FROM plan_events;
             DELETE FROM plan_steps;
             DELETE FROM plans;",
        )
        .map_err(io::Error::other)?;

        for plan_id in plan_ids {
            let plan_path = self.plan_json_path(&plan_id)?;
            let json = fs::read_to_string(&plan_path)?;
            let payload = parse_plan_payload(&json)?;
            self.replace_index_rows_for_plan(&tx, &plan_id, &plan_path, &json, &payload)?;
        }

        tx.commit().map_err(io::Error::other)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn query_indexed_plans(&self) -> io::Result<Vec<String>> {
        let index_path = self.plan_index_path();
        if !index_path.exists() {
            return Ok(Vec::new());
        }
        let conn = Connection::open_with_flags(
            index_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(io::Error::other)?;
        let mut stmt = conn
            .prepare("SELECT plan_id FROM plans ORDER BY plan_id")
            .map_err(io::Error::other)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(io::Error::other)?;
        let mut plans = Vec::new();
        for row in rows {
            plans.push(row.map_err(io::Error::other)?);
        }
        Ok(plans)
    }

    fn open_plan_index(&self) -> io::Result<Connection> {
        fs::create_dir_all(self.plan_state_dir())?;
        let conn = Connection::open(self.plan_index_path()).map_err(io::Error::other)?;
        bootstrap_index_schema(&conn)?;
        Ok(conn)
    }

    fn replace_index_rows_for_plan(
        &self,
        tx: &Transaction<'_>,
        plan_id: &str,
        plan_path: &Path,
        raw_json: &str,
        payload: &Value,
    ) -> io::Result<()> {
        tx.execute(
            "DELETE FROM plan_events WHERE plan_id = ?1",
            params![plan_id],
        )
        .map_err(io::Error::other)?;
        tx.execute(
            "DELETE FROM plan_steps WHERE plan_id = ?1",
            params![plan_id],
        )
        .map_err(io::Error::other)?;
        tx.execute("DELETE FROM plans WHERE plan_id = ?1", params![plan_id])
            .map_err(io::Error::other)?;

        let phase = payload.get("phase").and_then(Value::as_str);
        let status = payload.get("status").and_then(Value::as_str);
        let goal = payload.get("goal").and_then(Value::as_str);
        let current_step = payload.get("current_step").and_then(Value::as_i64);

        tx.execute(
            "INSERT INTO plans (
                plan_id, phase, status, goal, current_step, source_path, raw_json, indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                plan_id,
                phase,
                status,
                goal,
                current_step,
                plan_path.to_string_lossy().to_string(),
                raw_json,
                timestamp_seconds()
            ],
        )
        .map_err(io::Error::other)?;

        if let Some(steps) = payload.get("steps").and_then(Value::as_array) {
            for (step_index, step) in steps.iter().enumerate() {
                let step_status = step.get("status").and_then(Value::as_str);
                let step_title = step
                    .get("title")
                    .or_else(|| step.get("name"))
                    .or_else(|| step.get("summary"))
                    .and_then(Value::as_str);
                tx.execute(
                    "INSERT INTO plan_steps (plan_id, step_index, status, title, raw_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        plan_id,
                        step_index as i64,
                        step_status,
                        step_title,
                        step.to_string()
                    ],
                )
                .map_err(io::Error::other)?;
            }
        }

        if let Some(events) = payload.get("events").and_then(Value::as_array) {
            for (event_index, event) in events.iter().enumerate() {
                let event_kind = event
                    .get("type")
                    .or_else(|| event.get("kind"))
                    .or_else(|| event.get("event"))
                    .and_then(Value::as_str);
                let summary = event
                    .get("summary")
                    .or_else(|| event.get("message"))
                    .or_else(|| event.get("title"))
                    .and_then(Value::as_str);
                tx.execute(
                    "INSERT INTO plan_events (plan_id, event_index, event_kind, summary, raw_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        plan_id,
                        event_index as i64,
                        event_kind,
                        summary,
                        event.to_string()
                    ],
                )
                .map_err(io::Error::other)?;
            }
        }

        Ok(())
    }
}

fn parse_plan_payload(raw_json: &str) -> io::Result<Value> {
    serde_json::from_str(raw_json).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn timestamp_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn bootstrap_index_schema(conn: &Connection) -> io::Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS plans (
             plan_id TEXT PRIMARY KEY,
             phase TEXT,
             status TEXT,
             goal TEXT,
             current_step INTEGER,
             source_path TEXT NOT NULL,
             raw_json TEXT NOT NULL,
             indexed_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS plan_steps (
             plan_id TEXT NOT NULL,
             step_index INTEGER NOT NULL,
             status TEXT,
             title TEXT,
             raw_json TEXT NOT NULL,
             PRIMARY KEY (plan_id, step_index),
             FOREIGN KEY (plan_id) REFERENCES plans(plan_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS plan_events (
             plan_id TEXT NOT NULL,
             event_index INTEGER NOT NULL,
             event_kind TEXT,
             summary TEXT,
             raw_json TEXT NOT NULL,
             PRIMARY KEY (plan_id, event_index),
             FOREIGN KEY (plan_id) REFERENCES plans(plan_id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_plans_phase ON plans(phase);
         CREATE INDEX IF NOT EXISTS idx_plan_steps_status ON plan_steps(status);
         CREATE INDEX IF NOT EXISTS idx_plan_events_kind ON plan_events(event_kind);",
    )
    .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{Connection, params};
    use serde_json::json;

    use super::PlanStore;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new(test_name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after UNIX_EPOCH")
                .as_nanos();
            let path = std::env::current_dir()
                .expect("current directory should resolve")
                .join("target/test-workspaces")
                .join(format!("{test_name}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("test workspace should be creatable");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn round_trips_session_json_in_temp_workspace() {
        let workspace = TempWorkspace::new("plan-store-round-trip");
        let store = PlanStore::for_workspace(workspace.path());
        let plan_id = "plan-round-trip";
        let session = store.create_session();

        store
            .save_session_json(plan_id, &session)
            .expect("session should save");

        let loaded = store
            .load_session_json(plan_id)
            .expect("session should load");

        assert_eq!(loaded, session, "loaded session should match saved session");
        assert!(
            store
                .plan_json_path(plan_id)
                .expect("plan path should resolve")
                .exists(),
            "plan JSON file should exist after save"
        );
    }

    #[test]
    fn lists_only_json_plan_ids() {
        let workspace = TempWorkspace::new("plan-store-listing");
        let store = PlanStore::for_workspace(workspace.path());
        store
            .save_session_json("alpha-plan", &store.create_session())
            .expect("alpha plan should save");
        store
            .save_session_json("beta-plan", &store.create_session())
            .expect("beta plan should save");

        fs::write(store.plans_dir().join("notes.txt"), "ignore me")
            .expect("non-json marker file should write");
        fs::create_dir_all(store.plans_dir().join("subdir")).expect("subdir marker should write");

        let ids = store.list_plan_ids().expect("plan IDs should list");
        assert_eq!(ids, vec!["alpha-plan", "beta-plan"]);
    }

    #[test]
    fn rejects_invalid_plan_ids_with_invalid_input_errors() {
        let workspace = TempWorkspace::new("plan-store-invalid-id");
        let store = PlanStore::for_workspace(workspace.path());
        let session = store.create_session();

        let invalid_ids = [
            "",
            ".",
            "..",
            "../escape",
            r"..\escape",
            "nested/path",
            "space id",
            "dot.name",
        ];

        for plan_id in invalid_ids {
            let path_err = store
                .plan_json_path(plan_id)
                .expect_err("invalid ID should fail path resolution");
            assert_eq!(
                path_err.kind(),
                io::ErrorKind::InvalidInput,
                "path error kind should be InvalidInput for {plan_id:?}"
            );

            let save_err = store
                .save_session_json(plan_id, &session)
                .expect_err("invalid ID should fail save");
            assert_eq!(
                save_err.kind(),
                io::ErrorKind::InvalidInput,
                "save error kind should be InvalidInput for {plan_id:?}"
            );

            let load_err = store
                .load_session_json(plan_id)
                .expect_err("invalid ID should fail load");
            assert_eq!(
                load_err.kind(),
                io::ErrorKind::InvalidInput,
                "load error kind should be InvalidInput for {plan_id:?}"
            );
        }
    }

    #[test]
    fn load_session_json_returns_not_found_when_plan_file_is_missing() {
        let workspace = TempWorkspace::new("plan-store-missing-json");
        let store = PlanStore::for_workspace(workspace.path());

        let err = store
            .load_session_json("missing-plan")
            .expect_err("missing plan file should fail");
        assert_eq!(
            err.kind(),
            io::ErrorKind::NotFound,
            "missing plan should map to NotFound"
        );
    }

    #[test]
    fn load_session_json_maps_malformed_json_to_invalid_data() {
        let workspace = TempWorkspace::new("plan-store-malformed-json");
        let store = PlanStore::for_workspace(workspace.path());
        let plan_id = "malformed-json";

        fs::create_dir_all(store.plans_dir()).expect("plans dir should be creatable");
        fs::write(
            store
                .plan_json_path(plan_id)
                .expect("valid ID should resolve path"),
            "{ this is not valid json",
        )
        .expect("malformed plan file should write");

        let err = store
            .load_session_json(plan_id)
            .expect_err("malformed plan JSON should fail");
        assert_eq!(
            err.kind(),
            io::ErrorKind::InvalidData,
            "malformed json should map to InvalidData"
        );
    }

    #[test]
    fn list_plan_ids_skips_invalid_plan_file_stems() {
        let workspace = TempWorkspace::new("plan-store-list-skip-invalid");
        let store = PlanStore::for_workspace(workspace.path());
        store
            .save_session_json("valid_plan", &store.create_session())
            .expect("valid plan should save");

        fs::write(store.plans_dir().join("has space.json"), "{}")
            .expect("invalid plan ID with spaces should write");
        fs::write(store.plans_dir().join("dot.name.json"), "{}")
            .expect("invalid plan ID with dot should write");

        let ids = store.list_plan_ids().expect("plan IDs should list");
        assert_eq!(
            ids,
            vec!["valid_plan"],
            "invalid stems should be ignored by list API"
        );
    }

    #[test]
    fn save_session_json_succeeds_when_index_sync_fails_and_rebuild_recovers() {
        let workspace = TempWorkspace::new("plan-store-save-json-source-of-truth");
        let store = PlanStore::for_workspace(workspace.path());
        let plan_id = "recoverable-plan";
        let session = store.create_session();

        fs::create_dir_all(workspace.path().join(".blazar"))
            .expect("blazar dir should be creatable");
        fs::write(
            workspace.path().join(".blazar/state"),
            "block index dir creation",
        )
        .expect("state file should block index dir creation");

        store
            .save_session_json(plan_id, &session)
            .expect("json save should succeed even if index sync fails");

        let saved = store
            .load_session_json(plan_id)
            .expect("saved json should still load");
        assert_eq!(saved, session, "json should remain source of truth");
        assert_eq!(
            store
                .query_indexed_plans()
                .expect("querying missing index should still succeed"),
            Vec::<String>::new(),
            "index should remain unsynced when state dir is blocked"
        );

        fs::remove_file(workspace.path().join(".blazar/state"))
            .expect("blocking state file should be removable");
        fs::create_dir_all(workspace.path().join(".blazar/state"))
            .expect("state dir should be recreatable");

        store
            .rebuild_index_from_json()
            .expect("rebuild should recover derived index from json");
        assert_eq!(
            store
                .query_indexed_plans()
                .expect("index should query after recovery"),
            vec![plan_id.to_string()]
        );
    }

    #[test]
    fn sync_index_for_plan_tracks_latest_plan_row_and_children() {
        let workspace = TempWorkspace::new("plan-store-index-sync");
        let store = PlanStore::for_workspace(workspace.path());
        let plan_id = "sync-plan";

        write_plan_json(
            &store,
            plan_id,
            json!({
                "id": plan_id,
                "phase": "DraftStep",
                "status": "pending",
                "steps": [
                    {"title": "one", "status": "pending"},
                    {"title": "two", "status": "done"}
                ],
                "events": [
                    {"type": "decision", "summary": "first"}
                ]
            }),
        );
        store
            .sync_index_for_plan(plan_id)
            .expect("initial index sync should succeed");

        write_plan_json(
            &store,
            plan_id,
            json!({
                "id": plan_id,
                "phase": "Review",
                "status": "executing",
                "steps": [
                    {"title": "updated-one", "status": "done"}
                ],
                "events": [
                    {"type": "decision", "summary": "first"},
                    {"type": "status", "summary": "second"}
                ]
            }),
        );
        store
            .sync_index_for_plan(plan_id)
            .expect("second index sync should replace prior children");

        let indexed = store
            .query_indexed_plans()
            .expect("indexed plans query should succeed");
        assert_eq!(indexed, vec![plan_id.to_string()]);

        assert_eq!(
            read_plan_index_scalar(
                &store,
                "SELECT phase FROM plans WHERE plan_id = ?1",
                plan_id
            ),
            Some("Review".to_string())
        );
        assert_eq!(
            read_plan_index_scalar(
                &store,
                "SELECT status FROM plans WHERE plan_id = ?1",
                plan_id
            ),
            Some("executing".to_string())
        );
        assert_eq!(
            read_plan_index_count(
                &store,
                "SELECT COUNT(*) FROM plan_steps WHERE plan_id = ?1",
                plan_id
            ),
            1
        );
        assert_eq!(
            read_plan_index_count(
                &store,
                "SELECT COUNT(*) FROM plan_events WHERE plan_id = ?1",
                plan_id
            ),
            2
        );
    }

    #[test]
    fn rebuild_index_from_json_replaces_stale_index_rows() {
        let workspace = TempWorkspace::new("plan-store-index-rebuild");
        let store = PlanStore::for_workspace(workspace.path());

        write_plan_json(
            &store,
            "alpha",
            json!({
                "id": "alpha",
                "phase": "Discover",
                "status": "pending",
                "steps": [{"title": "alpha-step", "status": "pending"}],
                "events": []
            }),
        );
        store
            .sync_index_for_plan("alpha")
            .expect("alpha should sync into index");
        assert_eq!(
            store
                .query_indexed_plans()
                .expect("indexed plans should query after alpha sync"),
            vec!["alpha".to_string()]
        );

        fs::remove_file(
            store
                .plan_json_path("alpha")
                .expect("alpha path should resolve for removal"),
        )
        .expect("alpha source json should be removable");

        write_plan_json(
            &store,
            "beta",
            json!({
                "id": "beta",
                "phase": "ExecuteStep",
                "status": "executing",
                "steps": [
                    {"title": "beta-step-1", "status": "done"},
                    {"title": "beta-step-2", "status": "executing"}
                ],
                "events": [{"type": "status", "summary": "running"}]
            }),
        );

        store
            .rebuild_index_from_json()
            .expect("index rebuild should succeed from JSON source");

        assert_eq!(
            store
                .query_indexed_plans()
                .expect("indexed plans should query after rebuild"),
            vec!["beta".to_string()]
        );
        assert_eq!(
            read_plan_index_count(
                &store,
                "SELECT COUNT(*) FROM plan_steps WHERE plan_id = ?1",
                "beta"
            ),
            2
        );
        assert_eq!(
            read_plan_index_count(
                &store,
                "SELECT COUNT(*) FROM plan_events WHERE plan_id = ?1",
                "beta"
            ),
            1
        );
        assert_eq!(
            read_plan_index_count(
                &store,
                "SELECT COUNT(*) FROM plans WHERE plan_id = ?1",
                "alpha"
            ),
            0
        );
    }

    fn write_plan_json(store: &PlanStore, plan_id: &str, payload: serde_json::Value) {
        fs::create_dir_all(store.plans_dir()).expect("plans dir should be creatable");
        let path = store
            .plan_json_path(plan_id)
            .expect("plan id in test fixture should be valid");
        fs::write(
            path,
            serde_json::to_string_pretty(&payload).expect("fixture json should serialize"),
        )
        .expect("fixture plan json should write");
    }

    fn read_plan_index_count(store: &PlanStore, query: &str, plan_id: &str) -> usize {
        let conn = Connection::open(store.plan_index_path()).expect("index db should open");
        conn.query_row(query, params![plan_id], |row| row.get::<_, i64>(0))
            .expect("count query should succeed") as usize
    }

    fn read_plan_index_scalar(store: &PlanStore, query: &str, plan_id: &str) -> Option<String> {
        let conn = Connection::open(store.plan_index_path()).expect("index db should open");
        conn.query_row(query, params![plan_id], |row| {
            row.get::<_, Option<String>>(0)
        })
        .expect("scalar query should succeed")
    }
}
