use super::*;

impl ChatApp {
    pub fn send_message(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        let turn = build_pending_turn(trimmed);

        info!(
            "send_message: len={} preview={:.60}",
            trimmed.len(),
            trimmed
        );

        // Trigger demo playback when user types "1"
        if trimmed == "1" {
            self.timeline.clear();
            self.demo_queue = crate::chat::demo::demo_playback_script();
            self.demo_last_add = None;
            self.scroll_offset = u16::MAX;
            return;
        }

        self.messages.push(ChatMessage {
            author: Author::User,
            body: turn.user_text.clone(),
        });

        self.has_user_sent = true;

        // Add user message to timeline
        self.timeline
            .push(TimelineEntry::user_message(turn.user_text.clone()));

        match &turn.dispatch {
            PendingDispatch::DiscoverAgents => {
                self.refresh_acp_agents();
                return;
            }
            PendingDispatch::Runtime {
                runtime_prompt,
                kind,
            } => {
                // Admission control: queue if agent is busy instead of dropping
                if self.agent_state.is_busy() {
                    info!(
                        "send_message: queued (agent busy) queue_depth={}",
                        self.pending_messages.len() + 1
                    );
                    self.pending_messages.push_back(turn);
                    return;
                }

                // Dispatch to agent runtime — response arrives via events in tick()
                self.active_turn_kind = Some(*kind);
                self.active_turn_title = self.streaming_title_for_turn(*kind);
                if let Err(e) = self.agent_runtime.submit_turn(runtime_prompt) {
                    warn!("send_message: submit_turn failed: {e}");
                    self.active_turn_kind = None;
                    self.active_turn_title = None;
                    self.timeline
                        .push(TimelineEntry::warning(format!("Runtime error: {e}")));
                }

                // Auto-scroll to bottom
                self.scroll_offset = u16::MAX;
            }
        }
    }

    pub(super) fn refresh_acp_agents(&mut self) {
        self.timeline
            .push(TimelineEntry::hint("Discovering ACP agents..."));
        self.scroll_offset = u16::MAX;

        if let Err(error) = self.agent_runtime.refresh_acp_agents() {
            warn!("refresh_acp_agents: failed to enqueue refresh: {error}");
            self.timeline.push(TimelineEntry::warning(format!(
                "Failed to refresh ACP agents: {error}"
            )));
            self.scroll_offset = u16::MAX;
        }
    }

    /// Dispatches the next queued message to the agent runtime (FIFO).
    /// Called after any terminal turn event (TurnComplete, TurnFailed).
    pub(super) fn dispatch_next_queued(&mut self) {
        if let Some(turn) = self.pending_messages.pop_front() {
            info!(
                "dispatch_next_queued: dispatching len={} remaining={}",
                turn.user_text.len(),
                self.pending_messages.len()
            );
            match turn.dispatch {
                PendingDispatch::DiscoverAgents => {
                    self.refresh_acp_agents();
                }
                PendingDispatch::Runtime {
                    runtime_prompt,
                    kind,
                } => {
                    self.active_turn_kind = Some(kind);
                    self.active_turn_title = self.streaming_title_for_turn(kind);
                    if let Err(e) = self.agent_runtime.submit_turn(&runtime_prompt) {
                        warn!("dispatch_next_queued: submit_turn failed: {e}");
                        self.active_turn_kind = None;
                        self.active_turn_title = None;
                        self.timeline
                            .push(TimelineEntry::warning(format!("Runtime error: {e}")));
                        // Don't re-queue on channel error — runtime is dead
                    }
                    self.scroll_offset = u16::MAX;
                }
            }
        }
    }
}

pub(super) fn build_pending_turn(input: &str) -> PendingTurn {
    let trimmed = input.trim();
    if trimmed == "/discover-agents" {
        return PendingTurn {
            user_text: trimmed.to_owned(),
            dispatch: PendingDispatch::DiscoverAgents,
            timeline_inserted: false,
        };
    }

    if let Some(request) = trimmed.strip_prefix("/plan") {
        return PendingTurn {
            user_text: trimmed.to_owned(),
            dispatch: PendingDispatch::Runtime {
                runtime_prompt: build_plan_prompt(request.trim()),
                kind: TurnKind::Plan,
            },
            timeline_inserted: false,
        };
    }

    PendingTurn {
        user_text: trimmed.to_owned(),
        dispatch: PendingDispatch::Runtime {
            runtime_prompt: trimmed.to_owned(),
            kind: TurnKind::Chat,
        },
        timeline_inserted: false,
    }
}

fn build_plan_prompt(request: &str) -> String {
    format!(
        "You are in planning mode.\n\
         Generate a concise implementation plan only.\n\
         First line must be a short plain-text title with no markdown, no numbering, and no label.\n\
         After a blank line, write the plan body as concise ordered steps.\n\
         Keep the answer focused on planning.\n\n\
         User request:\n{request}"
    )
}

pub(super) fn extract_plan_title_and_body(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut lines = trimmed.lines();
    let title = lines.next()?.trim().trim_matches('#').trim().to_owned();
    if title.is_empty() {
        return None;
    }

    let body = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    Some((title, body))
}

pub(super) fn format_tool_call_details(
    arguments: &str,
    batch_id: u32,
    replay_index: usize,
    normalized_claims: &[String],
) -> String {
    let claims = if normalized_claims.is_empty() {
        "<none>".to_owned()
    } else {
        let mut sorted_claims = normalized_claims.to_vec();
        sorted_claims.sort();
        sorted_claims.join(",")
    };
    let metadata_line =
        format!("batch_id={batch_id} replay_index={replay_index} normalized_claims={claims}");

    if arguments.is_empty() {
        metadata_line
    } else {
        format!("{arguments}\n{metadata_line}")
    }
}

pub(super) fn extract_tool_call_metadata_line(details: &str) -> Option<String> {
    details
        .lines()
        .rev()
        .find(|line| {
            line.starts_with("batch_id=")
                && line.contains(" replay_index=")
                && line.contains(" normalized_claims=")
        })
        .map(ToOwned::to_owned)
}
