use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum PlanPhase {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PlanSession {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    phase: PlanPhase,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_step: Option<u32>,
    #[serde(default)]
    steps: Vec<PlanStep>,
    #[serde(default)]
    events: Vec<PlanEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PlanStep {
    title: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PlanEvent {
    #[serde(rename = "type")]
    kind: String,
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SegmentDecision {
    pub(super) summary: String,
    pub(super) phase: PlanPhase,
}

impl PlanSession {
    pub(super) fn new() -> Self {
        Self {
            id: None,
            phase: PlanPhase::Discover,
            status: "pending".to_owned(),
            goal: None,
            current_step: None,
            steps: Vec::new(),
            events: Vec::new(),
        }
    }

    pub(super) fn set_plan_id(&mut self, id: impl Into<String>) {
        self.id = Some(id.into());
    }

    pub(super) fn plan_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub(super) fn set_goal(&mut self, goal: String) {
        self.goal = Some(goal);
    }

    pub(super) fn run_one_segment(&mut self) -> SegmentDecision {
        let summary = match self.phase {
            PlanPhase::Discover => {
                if self
                    .goal
                    .as_deref()
                    .is_some_and(|goal| !goal.trim().is_empty())
                {
                    self.phase = PlanPhase::DraftStep;
                    self.status = "executing".to_owned();
                    "Discovery complete. Ready to draft the first plan step.".to_owned()
                } else {
                    self.status = "pending".to_owned();
                    "Discovery needs a goal before drafting can continue.".to_owned()
                }
            }
            phase => {
                let label = phase.as_str();
                self.status = "executing".to_owned();
                format!(
                    "Single-step orchestration reached {label}; deeper phase loops are deferred."
                )
            }
        };

        self.events.push(PlanEvent {
            kind: "decision".to_owned(),
            summary: summary.clone(),
        });

        SegmentDecision {
            summary,
            phase: self.phase,
        }
    }

    #[cfg(test)]
    fn new_at(phase: PlanPhase) -> Self {
        Self {
            phase,
            ..Self::new()
        }
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
}

impl PlanPhase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            PlanPhase::Discover => "Discover",
            PlanPhase::Clarify => "Clarify",
            PlanPhase::DraftStep => "DraftStep",
            PlanPhase::FinalizePlan => "FinalizePlan",
            PlanPhase::ExecuteStep => "ExecuteStep",
            PlanPhase::Review => "Review",
            PlanPhase::Done => "Done",
        }
    }

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

    #[test]
    fn run_one_segment_moves_discover_to_draft_when_goal_is_present() {
        let mut session = PlanSession::new();
        session.set_goal("stabilize orchestration".to_owned());

        let decision = session.run_one_segment();

        assert_eq!(decision.phase, PlanPhase::DraftStep);
        assert!(decision.summary.contains("Discovery complete"));
    }

    #[test]
    fn run_one_segment_keeps_discover_when_goal_is_missing() {
        let mut session = PlanSession::new();

        let decision = session.run_one_segment();

        assert_eq!(decision.phase, PlanPhase::Discover);
        assert!(decision.summary.contains("needs a goal"));
    }
}
