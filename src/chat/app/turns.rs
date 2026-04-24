use super::*;

impl ChatApp {
    pub fn send_message(&mut self, input: &str) {
        self.send_message_internal(input, true);
    }

    pub(crate) fn send_message_without_command_dispatch(&mut self, input: &str) {
        self.send_message_internal(input, false);
    }

    pub(crate) fn execute_discover_agents_command(&mut self) {
        self.push_user_message("/discover-agents");
        self.refresh_acp_agents();
    }

    fn send_message_internal(&mut self, input: &str, allow_command_dispatch: bool) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        if allow_command_dispatch
            && !self.agent_state.is_busy()
            && trimmed != "/plan"
            && self.command_registry.find(trimmed).is_some()
        {
            if let Err(err) = self.execute_palette_command_sync(trimmed, serde_json::json!({})) {
                self.timeline
                    .push(TimelineEntry::warning(format!("Command failed: {err}")));
                self.scroll_offset = u16::MAX;
            }
            return;
        }

        let turn = build_pending_turn_for_mode(trimmed, self.user_mode);

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

        self.has_user_sent = true;

        // Admission control: queue if agent is busy instead of dropping
        if self.agent_state.is_busy() {
            info!(
                "send_message: queued (agent busy) queue_depth={}",
                self.pending_messages.len() + 1
            );
            self.pending_messages.push_back(turn);
            return;
        }

        if !self.dispatch_turn(turn) {
            self.dispatch_next_queued();
        }
    }

    fn push_user_message(&mut self, body: &str) {
        self.messages.push(ChatMessage {
            author: Author::User,
            body: body.to_owned(),
        });

        self.has_user_sent = true;
        self.timeline
            .push(TimelineEntry::user_message(body.to_owned()));
    }

    pub(super) fn refresh_acp_agents(&mut self) -> bool {
        self.timeline
            .push(TimelineEntry::hint("Discovering ACP agents..."));
        self.scroll_offset = u16::MAX;

        if let Err(error) = self.agent_runtime.refresh_acp_agents() {
            warn!("refresh_acp_agents: failed to enqueue refresh: {error}");
            self.timeline.push(TimelineEntry::warning(format!(
                "Failed to refresh ACP agents: {error}"
            )));
            self.scroll_offset = u16::MAX;
            return false;
        }

        true
    }

    pub(super) fn dispatch_turn(&mut self, mut turn: PendingTurn) -> bool {
        if !turn.timeline_inserted {
            self.messages.push(ChatMessage {
                author: Author::User,
                body: turn.user_text.clone(),
            });
            self.timeline
                .push(TimelineEntry::user_message(turn.user_text.clone()));
            turn.timeline_inserted = true;
        }

        let dispatched = match turn.dispatch {
            PendingDispatch::Runtime {
                runtime_prompt,
                kind,
            } => {
                self.active_turn_kind = Some(kind);
                self.active_turn_title = self.streaming_title_for_turn(kind);
                if let Err(e) = self.agent_runtime.submit_turn(&runtime_prompt) {
                    warn!("dispatch_turn: submit_turn failed: {e}");
                    self.active_turn_kind = None;
                    self.active_turn_title = None;
                    self.timeline
                        .push(TimelineEntry::warning(format!("Runtime error: {e}")));
                    self.scroll_offset = u16::MAX;
                    return false;
                }
                true
            }
            PendingDispatch::DiscoverAgents => self.refresh_acp_agents(),
        };

        self.scroll_offset = u16::MAX;
        dispatched
    }

    /// Dispatches the next queued message to the agent runtime (FIFO).
    /// Called after any terminal turn event (TurnComplete, TurnFailed).
    pub(super) fn dispatch_next_queued(&mut self) {
        while let Some(turn) = self.pending_messages.pop_front() {
            info!(
                "dispatch_next_queued: dispatching len={} remaining={}",
                turn.user_text.len(),
                self.pending_messages.len()
            );
            if self.dispatch_turn(turn) {
                break;
            }
        }
    }
}

#[cfg(test)]
pub(super) fn build_pending_turn(input: &str) -> PendingTurn {
    build_pending_turn_for_mode(input, UserMode::Auto)
}

pub(super) fn build_pending_turn_for_mode(input: &str, user_mode: UserMode) -> PendingTurn {
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

    if user_mode == UserMode::Plan {
        return PendingTurn {
            user_text: trimmed.to_owned(),
            dispatch: PendingDispatch::Runtime {
                runtime_prompt: build_plan_prompt(trimmed),
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

pub(super) fn short_action_name_from_text(text: &str) -> Option<String> {
    let action = first_action_word(text)?;
    Some(truncate_action_name(action, 10))
}

pub(super) fn extract_plan_action_names(body: &str) -> Vec<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(name) = parse_next_step_name_line(trimmed) {
        return vec![name];
    }

    let numbered_actions: Vec<String> = trimmed
        .lines()
        .filter_map(parse_numbered_plan_action_line)
        .collect();

    if !numbered_actions.is_empty() {
        return numbered_actions;
    }

    short_action_name_from_text(trimmed).into_iter().collect()
}

pub(super) fn parse_next_step_name_line(text: &str) -> Option<String> {
    let first_line = text.lines().next()?.trim();
    let name = first_line.strip_prefix("next_step_name:")?.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn parse_numbered_plan_action_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let candidate = trimmed
        .trim_start_matches(|c: char| {
            c.is_ascii_digit() || matches!(c, '.' | ')' | ':' | '-' | '•')
        })
        .trim_start();

    if candidate.is_empty() || candidate == trimmed {
        return None;
    }

    Some(candidate.to_owned())
}

fn first_action_word(text: &str) -> Option<&str> {
    text.split_whitespace().find_map(|token| {
        let token = token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | '.'
                        | '!'
                        | '?'
                        | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '"'
                        | '\''
                        | '`'
                        | '*'
                        | '-'
                )
            })
            .trim();
        if token.is_empty() || token.chars().all(|c| c.is_ascii_digit()) {
            None
        } else {
            Some(token)
        }
    })
}

fn truncate_action_name(name: &str, max_chars: usize) -> String {
    let mut chars = name.chars();
    let mut shortened = String::new();
    for _ in 0..max_chars {
        if let Some(ch) = chars.next() {
            shortened.push(ch);
        } else {
            return shortened;
        }
    }

    if chars.next().is_some() {
        shortened.push('…');
    }

    shortened
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

pub(crate) fn tool_call_details_payload(details: &str) -> &str {
    let mut cutoff = details.len();
    for marker in ["\ndebug.", "\nbatch_id="] {
        if let Some(index) = details.find(marker) {
            cutoff = cutoff.min(index);
        }
    }

    details[..cutoff].trim_end_matches('\n')
}

pub(super) fn extract_tool_call_metadata_line(details: &str) -> Option<String> {
    tool_call_metadata_line(details).map(ToOwned::to_owned)
}

pub(crate) fn tool_call_metadata_line(details: &str) -> Option<&str> {
    details.lines().rev().find(|line| {
        line.starts_with("batch_id=")
            && line.contains(" replay_index=")
            && line.contains(" normalized_claims=")
    })
}
