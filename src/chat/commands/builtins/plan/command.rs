use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

use super::session::PlanPhase;
use super::store::PlanStore;

struct PlanCommand {
    spec: CommandSpec,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanArgs {
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    continue_id: Option<String>,
    #[serde(default)]
    review: Option<String>,
}

impl PaletteCommand for PlanCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            let parsed = parse_plan_args(args)?;
            let store = PlanStore::for_workspace(ctx.app.workspace_root());
            let resumed = parsed.continue_id.is_some();

            let mut session = if let Some(continue_id) = parsed.continue_id.as_deref() {
                let mut session = load_continued_session(&store, continue_id)?;
                if session.plan_id().is_none() {
                    session.set_plan_id(continue_id.to_owned());
                }
                session
            } else {
                let mut session = store.create_session();
                let plan_id = generate_plan_id(&store)?;
                session.set_plan_id(plan_id);
                session
            };

            if let Some(goal) = parsed.goal {
                session.set_goal(goal);
            }
            if let Some(review_decision) = parsed.review {
                session.record_review_decision(review_decision);
            }

            let decision = session.run_one_segment();
            let plan_id = session
                .plan_id()
                .ok_or_else(|| CommandError::ExecutionFailed("plan id missing".to_owned()))?
                .to_owned();

            store
                .save_session_json(&plan_id, &session)
                .map_err(map_store_save_error)?;

            let continue_command = format!("/plan --continue {plan_id}");
            ctx.app.push_system_hint(format!(
                "Plan {plan_id}: {} (phase: {}).",
                decision.summary,
                decision.phase.as_str()
            ));
            if decision.phase == PlanPhase::Clarify {
                ctx.app.push_system_hint(
                    "Clarification required. Re-run `/plan --continue <plan-id> --goal \"...\"` with the missing goal details."
                        .to_owned(),
                );
            }
            if decision.phase == PlanPhase::Review {
                ctx.app.push_system_hint(
                    "Review required. Re-run with `/plan --continue <plan-id> --review continue|revise|fail`."
                        .to_owned(),
                );
            }
            ctx.app.push_system_hint(format!(
                "Continue this plan with `{continue_command}`. This workflow is intentionally staged; deeper steps are deferred until you run the continue command."
            ));
            ctx.app.set_composer_text(&continue_command);

            let lifecycle = if resumed { "continued" } else { "started" };
            Ok(CommandResult {
                summary: format!(
                    "Plan {plan_id} {lifecycle}: {} (phase: {}). Continuation is intentionally staged; deeper steps remain deferred until `/plan --continue`.",
                    decision.summary,
                    decision.phase.as_str()
                ),
            })
        })
    }
}

fn parse_plan_args(args: serde_json::Value) -> Result<PlanArgs, CommandError> {
    let parsed: PlanArgs = serde_json::from_value(args)
        .map_err(|err| CommandError::InvalidArgs(format!("failed to parse /plan args: {err}")))?;

    if let Some(goal) = parsed.goal.as_deref()
        && goal.trim().is_empty()
    {
        return Err(CommandError::InvalidArgs(
            "goal must not be blank when provided".to_owned(),
        ));
    }

    if let Some(continue_id) = parsed.continue_id.as_deref()
        && continue_id.trim().is_empty()
    {
        return Err(CommandError::InvalidArgs(
            "continue_id must not be blank when provided".to_owned(),
        ));
    }

    let review = parsed.review.map(|value| value.trim().to_ascii_lowercase());
    if let Some(review) = review.as_deref() {
        if review.is_empty() {
            return Err(CommandError::InvalidArgs(
                "review must not be blank when provided".to_owned(),
            ));
        }
        if !matches!(review, "continue" | "revise" | "fail") {
            return Err(CommandError::InvalidArgs(
                "review must be one of: continue, revise, fail".to_owned(),
            ));
        }
    }

    Ok(PlanArgs {
        goal: parsed.goal.map(|value| value.trim().to_owned()),
        continue_id: parsed.continue_id.map(|value| value.trim().to_owned()),
        review,
    })
}

fn load_continued_session(
    store: &PlanStore,
    continue_id: &str,
) -> Result<super::session::PlanSession, CommandError> {
    store
        .load_session_json(continue_id)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                CommandError::Unavailable(format!("plan session not found: {continue_id}"))
            }
            std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
                CommandError::InvalidArgs(format!("invalid continue_id {continue_id:?}: {error}"))
            }
            _ => CommandError::ExecutionFailed(format!(
                "failed to load plan session {continue_id}: {error}"
            )),
        })
}

fn map_store_save_error(error: std::io::Error) -> CommandError {
    match error.kind() {
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
            CommandError::InvalidArgs(format!("failed to persist plan session: {error}"))
        }
        _ => CommandError::ExecutionFailed(format!("failed to persist plan session: {error}")),
    }
}

fn generate_plan_id(store: &PlanStore) -> Result<String, CommandError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    for suffix in 0..1000_u16 {
        let candidate = format!("plan-{now}-{suffix}");
        let path = store.plan_json_path(&candidate).map_err(|error| {
            CommandError::ExecutionFailed(format!("failed to create plan id: {error}"))
        })?;
        if !path.exists() {
            return Ok(candidate);
        }
    }

    Err(CommandError::ExecutionFailed(
        "failed to allocate unique plan id".to_owned(),
    ))
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/plan",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(PlanCommand {
                spec: CommandSpec {
                    name: "/plan".to_owned(),
                    description: "Generate a plan with an auto-titled summary".to_owned(),
                    args_schema: json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "goal": { "type": "string" },
                            "continue_id": { "type": "string" },
                            "review": { "type": "string", "enum": ["continue", "revise", "fail"] }
                        }
                    }),
                },
            })
        },
    }
}
