use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

    pub(crate) fn save_session_json(&self, plan_id: &str, session: &PlanSession) -> io::Result<()> {
        let plans_dir = self.plans_dir();
        fs::create_dir_all(&plans_dir)?;
        let json = serde_json::to_string_pretty(session)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let plan_path = self.plan_json_path(plan_id)?;
        fs::write(plan_path, json)?;
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
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
