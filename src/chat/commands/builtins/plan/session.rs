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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewDecision {
    Continue,
    Revise,
    Fail,
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

    pub(super) fn record_review_decision(&mut self, decision: String) {
        self.events.push(PlanEvent {
            kind: "review_decision".to_owned(),
            summary: decision,
        });
    }

    pub(super) fn run_one_segment(&mut self) -> SegmentDecision {
        let summary = match self.phase {
            PlanPhase::Discover => {
                if self.goal_is_present() {
                    self.phase = PlanPhase::DraftStep;
                    self.status = "executing".to_owned();
                    "Discovery complete. Ready to draft the first plan step.".to_owned()
                } else {
                    self.phase = PlanPhase::Clarify;
                    self.status = "pending".to_owned();
                    "Discovery needs clarification before drafting can continue.".to_owned()
                }
            }
            PlanPhase::Clarify => {
                if self.goal_is_present() {
                    self.phase = PlanPhase::DraftStep;
                    self.status = "executing".to_owned();
                    "Clarification complete. Ready to draft the next plan step.".to_owned()
                } else {
                    self.status = "pending".to_owned();
                    "Clarify loop is waiting for a concrete goal before planning continues."
                        .to_owned()
                }
            }
            PlanPhase::DraftStep => {
                if self.goal_needs_clarification() {
                    self.phase = PlanPhase::Clarify;
                    self.status = "pending".to_owned();
                    "Drafting paused until the goal is clarified.".to_owned()
                } else {
                    if self.steps.is_empty() {
                        let goal = self.goal.as_deref().map(str::trim).unwrap_or("goal");
                        self.steps.push(PlanStep {
                            title: format!("Deliver: {goal}"),
                            status: "pending".to_owned(),
                        });
                    }
                    if self.current_step.is_none() {
                        self.current_step = Some(0);
                    }
                    self.phase = PlanPhase::FinalizePlan;
                    self.status = "executing".to_owned();
                    "Drafted a micro-step. Ready to finalize the plan.".to_owned()
                }
            }
            PlanPhase::FinalizePlan => {
                if self.session_needs_clarification() {
                    self.phase = PlanPhase::Clarify;
                    self.status = "pending".to_owned();
                    "Finalize plan needs clarification before execution can continue.".to_owned()
                } else {
                    if let Some(next_step) = self.next_pending_step_index() {
                        self.current_step = Some(next_step as u32);
                    }
                    self.phase = PlanPhase::ExecuteStep;
                    self.status = "executing".to_owned();
                    "Plan finalized. Ready to execute the next micro-step.".to_owned()
                }
            }
            PlanPhase::ExecuteStep => {
                if self.session_needs_clarification() {
                    self.phase = PlanPhase::Clarify;
                    self.status = "pending".to_owned();
                    "Execution needs clarification before the next micro-step.".to_owned()
                } else {
                    let step_index = self
                        .current_step
                        .map(|step| step as usize)
                        .or_else(|| self.next_pending_step_index())
                        .unwrap_or(0);

                    if step_index >= self.steps.len() {
                        self.steps.push(PlanStep {
                            title: format!("Execution step {}", step_index + 1),
                            status: "pending".to_owned(),
                        });
                    }

                    if let Some(step) = self.steps.get_mut(step_index) {
                        step.status = "done".to_owned();
                    }

                    self.current_step = Some(step_index as u32);
                    self.phase = PlanPhase::Review;
                    self.status = "executing".to_owned();
                    "Execution micro-step completed. Review decision required: continue, revise, or fail."
                        .to_owned()
                }
            }
            PlanPhase::Review => {
                if self.session_needs_clarification() {
                    self.phase = PlanPhase::Clarify;
                    self.status = "pending".to_owned();
                    "Review needs clarification before choosing the next transition.".to_owned()
                } else {
                    match self.review_decision() {
                        ReviewDecision::Continue => {
                            if let Some(next_step) = self.next_pending_step_index() {
                                self.current_step = Some(next_step as u32);
                                self.phase = PlanPhase::ExecuteStep;
                                self.status = "executing".to_owned();
                                format!(
                                    "Review selected continue. Next execution micro-step: {}.",
                                    next_step + 1
                                )
                            } else {
                                self.phase = PlanPhase::Done;
                                self.status = "completed".to_owned();
                                "Review confirmed completion. Plan is done.".to_owned()
                            }
                        }
                        ReviewDecision::Revise => {
                            self.phase = PlanPhase::DraftStep;
                            self.current_step = None;
                            self.status = "pending".to_owned();
                            "Review selected revise. Returning to draft a new micro-step."
                                .to_owned()
                        }
                        ReviewDecision::Fail => {
                            self.phase = PlanPhase::Done;
                            self.status = "failed".to_owned();
                            "Review selected fail. Plan marked as failed.".to_owned()
                        }
                    }
                }
            }
            PlanPhase::Done => {
                if self.status.is_empty() {
                    self.status = "completed".to_owned();
                }
                "Plan is already complete.".to_owned()
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

    fn goal_is_present(&self) -> bool {
        self.goal
            .as_deref()
            .is_some_and(|goal| !goal.trim().is_empty())
    }

    fn goal_needs_clarification(&self) -> bool {
        !self.goal_is_present()
    }

    fn session_needs_clarification(&self) -> bool {
        self.goal_needs_clarification() || self.steps.is_empty()
    }

    fn next_pending_step_index(&self) -> Option<usize> {
        self.steps.iter().position(|step| step.status != "done")
    }

    fn review_decision(&self) -> ReviewDecision {
        self.events
            .iter()
            .rev()
            .find_map(|event| {
                if event.kind != "review_decision" {
                    return None;
                }
                let decision = event.summary.trim().to_ascii_lowercase();
                if decision.contains("fail") {
                    return Some(ReviewDecision::Fail);
                }
                if decision.contains("revise") {
                    return Some(ReviewDecision::Revise);
                }
                if decision.contains("continue") {
                    return Some(ReviewDecision::Continue);
                }
                None
            })
            .unwrap_or(ReviewDecision::Continue)
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
    fn run_one_segment_routes_discover_to_clarify_when_goal_is_missing() {
        let mut session = PlanSession::new();

        let decision = session.run_one_segment();

        assert_eq!(decision.phase, PlanPhase::Clarify);
        assert!(decision.summary.contains("clarification"));
    }

    #[test]
    fn run_one_segment_execute_step_moves_to_review() {
        let mut session = PlanSession {
            id: Some("plan-review".to_owned()),
            phase: PlanPhase::ExecuteStep,
            status: "executing".to_owned(),
            goal: Some("ship segmented plan".to_owned()),
            current_step: Some(0),
            steps: vec![super::PlanStep {
                title: "execute first step".to_owned(),
                status: "pending".to_owned(),
            }],
            events: Vec::new(),
        };

        let decision = session.run_one_segment();

        assert_eq!(decision.phase, PlanPhase::Review);
    }

    #[test]
    fn run_one_segment_review_defaults_to_continue_path() {
        let mut session = PlanSession {
            id: Some("plan-review-continue".to_owned()),
            phase: PlanPhase::Review,
            status: "executing".to_owned(),
            goal: Some("ship segmented plan".to_owned()),
            current_step: Some(0),
            steps: vec![
                super::PlanStep {
                    title: "execute first step".to_owned(),
                    status: "done".to_owned(),
                },
                super::PlanStep {
                    title: "execute second step".to_owned(),
                    status: "pending".to_owned(),
                },
            ],
            events: Vec::new(),
        };

        let decision = session.run_one_segment();

        assert_eq!(decision.phase, PlanPhase::ExecuteStep);
    }

    #[test]
    fn run_one_segment_review_can_revise_or_fail() {
        let mut revise = PlanSession {
            id: Some("plan-review-revise".to_owned()),
            phase: PlanPhase::Review,
            status: "executing".to_owned(),
            goal: Some("ship segmented plan".to_owned()),
            current_step: Some(0),
            steps: vec![super::PlanStep {
                title: "execute first step".to_owned(),
                status: "done".to_owned(),
            }],
            events: vec![super::PlanEvent {
                kind: "review_decision".to_owned(),
                summary: "revise".to_owned(),
            }],
        };
        let mut fail = PlanSession {
            id: Some("plan-review-fail".to_owned()),
            phase: PlanPhase::Review,
            status: "executing".to_owned(),
            goal: Some("ship segmented plan".to_owned()),
            current_step: Some(0),
            steps: vec![super::PlanStep {
                title: "execute first step".to_owned(),
                status: "done".to_owned(),
            }],
            events: vec![super::PlanEvent {
                kind: "review_decision".to_owned(),
                summary: "fail".to_owned(),
            }],
        };

        let revise_decision = revise.run_one_segment();
        let fail_decision = fail.run_one_segment();

        assert_eq!(revise_decision.phase, PlanPhase::DraftStep);
        assert_eq!(fail_decision.phase, PlanPhase::Done);
    }
}
