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

    pub(crate) fn plan_json_path(&self, plan_id: &str) -> PathBuf {
        self.plans_dir().join(format!("{plan_id}.json"))
    }

    pub(crate) fn save_session_json(&self, plan_id: &str, session: &PlanSession) -> io::Result<()> {
        let plans_dir = self.plans_dir();
        fs::create_dir_all(&plans_dir)?;
        let json = serde_json::to_string_pretty(session)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(self.plan_json_path(plan_id), json)?;
        Ok(())
    }

    pub(crate) fn load_session_json(&self, plan_id: &str) -> io::Result<PlanSession> {
        let json = fs::read_to_string(self.plan_json_path(plan_id))?;
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
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
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
            store.plan_json_path(plan_id).exists(),
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
}
