use super::session::PlanSession;

pub(crate) struct PlanStore;

impl PlanStore {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn create_session(self) -> PlanSession {
        PlanSession::new()
    }
}
