#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanPhase {
    Discover,
    Clarify,
    DraftStep,
    FinalizePlan,
    ExecuteStep,
    Review,
    Done,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanSignal {
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

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlanTransitionError {
    phase: PlanPhase,
    signal: PlanSignal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlanSession {
    phase: PlanPhase,
}

impl PlanSession {
    pub(super) fn new() -> Self {
        Self {
            phase: PlanPhase::Discover,
        }
    }

    #[cfg(test)]
    fn new_at(phase: PlanPhase) -> Self {
        Self { phase }
    }

    #[cfg(test)]
    fn phase(&self) -> PlanPhase {
        self.phase
    }

    #[cfg(test)]
    fn apply(&mut self, signal: PlanSignal) -> Result<PlanPhase, PlanTransitionError> {
        let next = self.phase.next(signal)?;
        self.phase = next;
        Ok(next)
    }

    pub(super) fn prefill_text(&self) -> &'static str {
        "/plan "
    }
}

impl PlanPhase {
    #[cfg(test)]
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
    fn allows_all_state_machine_transitions() {
        let cases = [
            (
                PlanPhase::Discover,
                PlanSignal::NeedClarification,
                PlanPhase::Clarify,
            ),
            (
                PlanPhase::Discover,
                PlanSignal::DiscoveryComplete,
                PlanPhase::DraftStep,
            ),
            (
                PlanPhase::Clarify,
                PlanSignal::ClarificationComplete,
                PlanPhase::DraftStep,
            ),
            (
                PlanPhase::DraftStep,
                PlanSignal::NeedClarification,
                PlanPhase::Clarify,
            ),
            (
                PlanPhase::DraftStep,
                PlanSignal::StepDrafted,
                PlanPhase::FinalizePlan,
            ),
            (
                PlanPhase::FinalizePlan,
                PlanSignal::PlanFinalized,
                PlanPhase::ExecuteStep,
            ),
            (
                PlanPhase::ExecuteStep,
                PlanSignal::ActionCompleted,
                PlanPhase::Review,
            ),
            (
                PlanPhase::Review,
                PlanSignal::RetryExecution,
                PlanPhase::ExecuteStep,
            ),
            (
                PlanPhase::Review,
                PlanSignal::RevisePlan,
                PlanPhase::DraftStep,
            ),
            (
                PlanPhase::Review,
                PlanSignal::NeedClarification,
                PlanPhase::Clarify,
            ),
            (
                PlanPhase::Review,
                PlanSignal::PlanCompleted,
                PlanPhase::Done,
            ),
        ];

        for (start, signal, expected) in cases {
            let mut session = PlanSession::new_at(start);
            let next = session
                .apply(signal)
                .unwrap_or_else(|_| panic!("expected valid transition: {start:?} + {signal:?}"));

            assert_eq!(
                next, expected,
                "unexpected next phase for {start:?} + {signal:?}"
            );
            assert_eq!(
                session.phase(),
                expected,
                "session phase should update for {start:?} + {signal:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_state_machine_transitions() {
        let cases = [
            (PlanPhase::Discover, PlanSignal::ActionCompleted),
            (PlanPhase::Clarify, PlanSignal::PlanFinalized),
            (PlanPhase::FinalizePlan, PlanSignal::RetryExecution),
            (PlanPhase::ExecuteStep, PlanSignal::NeedClarification),
            (PlanPhase::Review, PlanSignal::DiscoveryComplete),
            (PlanPhase::Done, PlanSignal::PlanCompleted),
        ];

        for (start, signal) in cases {
            let mut session = PlanSession::new_at(start);
            let err = match session.apply(signal) {
                Err(err) => err,
                Ok(next) => {
                    panic!("expected invalid transition: {start:?} + {signal:?}, got {next:?}")
                }
            };

            assert_eq!(err.phase, start, "error phase should remain start phase");
            assert_eq!(
                err.signal, signal,
                "error signal should match attempted signal"
            );
            assert_eq!(
                session.phase(),
                start,
                "session phase should remain unchanged for {start:?} + {signal:?}"
            );
        }
    }
}
