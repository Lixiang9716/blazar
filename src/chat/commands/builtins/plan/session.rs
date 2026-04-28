#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanPhase {
    Discover,
    Clarify,
    DraftStep,
    FinalizePlan,
    ExecuteStep,
    Review,
    Done,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanSignal {
    NeedClarification,
    DiscoveryComplete,
    ClarificationComplete,
    StepDrafted,
    PlanFinalized,
    ActionCompleted,
    RetryExecution,
    RevisePlan,
    PlanCompleted,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlanTransitionError {
    pub(crate) phase: PlanPhase,
    pub(crate) signal: PlanSignal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlanSession {
    phase: PlanPhase,
}

impl PlanSession {
    pub(crate) fn new() -> Self {
        Self {
            phase: PlanPhase::Discover,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_at(phase: PlanPhase) -> Self {
        Self { phase }
    }

    #[allow(dead_code)]
    pub(crate) fn phase(&self) -> PlanPhase {
        self.phase
    }

    #[allow(dead_code)]
    pub(crate) fn apply(&mut self, signal: PlanSignal) -> Result<PlanPhase, PlanTransitionError> {
        let next = self.phase.next(signal)?;
        self.phase = next;
        Ok(next)
    }

    pub(crate) fn prefill_text(&self) -> &'static str {
        "/plan "
    }
}

impl PlanPhase {
    #[allow(dead_code)]
    fn next(self, signal: PlanSignal) -> Result<Self, PlanTransitionError> {
        use PlanPhase::{Clarify, Discover, Done, DraftStep, ExecuteStep, FinalizePlan, Review};
        use PlanSignal::{
            ActionCompleted, ClarificationComplete, DiscoveryComplete, NeedClarification,
            PlanCompleted, PlanFinalized, RetryExecution, RevisePlan, StepDrafted,
        };

        match (self, signal) {
            (Discover, NeedClarification) => Ok(Clarify),
            (Discover, DiscoveryComplete) => Ok(DraftStep),
            (Clarify, ClarificationComplete) => Ok(DraftStep),
            (DraftStep, NeedClarification) => Ok(Clarify),
            (DraftStep, StepDrafted) => Ok(FinalizePlan),
            (FinalizePlan, PlanFinalized) => Ok(ExecuteStep),
            (ExecuteStep, ActionCompleted) => Ok(Review),
            (Review, RetryExecution) => Ok(ExecuteStep),
            (Review, RevisePlan) => Ok(DraftStep),
            (Review, NeedClarification) => Ok(Clarify),
            (Review, PlanCompleted) => Ok(Done),
            _ => Err(PlanTransitionError {
                phase: self,
                signal,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PlanPhase, PlanSession, PlanSignal};

    #[test]
    fn discover_need_clarification_transitions_to_clarify() {
        let mut session = PlanSession::new();
        assert_eq!(session.phase(), PlanPhase::Discover);

        session
            .apply(PlanSignal::NeedClarification)
            .expect("discover + need clarification should be valid");

        assert_eq!(session.phase(), PlanPhase::Clarify);
    }

    #[test]
    fn execute_step_moves_to_review_after_action() {
        let mut session = PlanSession::new_at(PlanPhase::ExecuteStep);

        session
            .apply(PlanSignal::ActionCompleted)
            .expect("execute step + action completed should be valid");

        assert_eq!(session.phase(), PlanPhase::Review);
    }

    #[test]
    fn discover_cannot_jump_to_review() {
        let mut session = PlanSession::new();

        let err = session
            .apply(PlanSignal::ActionCompleted)
            .expect_err("discover + action completed should be rejected");

        assert_eq!(err.phase, PlanPhase::Discover);
        assert_eq!(err.signal, PlanSignal::ActionCompleted);
        assert_eq!(session.phase(), PlanPhase::Discover);
    }
}
