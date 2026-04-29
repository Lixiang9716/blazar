use super::*;
use crate::chat::picker::{PickerContext, PickerItem};

impl ChatApp {
    pub(crate) fn reset_command_picker(&mut self) {
        self.picker = ModalPicker::command_palette_from_registry(&self.command_registry);
    }

    pub(crate) fn open_theme_picker(&mut self) {
        let theme_items: Vec<PickerItem> = crate::chat::theme::available_themes()
            .into_iter()
            .map(|info| PickerItem::new(info.name.clone(), info.display_name.clone()))
            .collect();
        self.picker =
            ModalPicker::with_context("Select Theme", theme_items, PickerContext::ThemeSelect);
        self.picker.open();
    }

    pub(crate) fn open_model_picker(&mut self) {
        let current = &self.model_name;
        let repo_str = self.workspace_root.to_string_lossy();
        let model_items: Vec<PickerItem> = self
            .model_metadata
            .fetch_available_models(&repo_str)
            .into_iter()
            .map(|m| {
                let label = if m.id == *current {
                    format!("{} ✓", m.id)
                } else {
                    m.id
                };
                PickerItem::new(label, m.description)
            })
            .collect();
        self.picker =
            ModalPicker::with_context("Select Model", model_items, PickerContext::ModelSelect);
        self.picker.open();
    }

    pub fn handle_action(&mut self, action: InputAction) {
        // When picker is open, route input to it
        if self.picker.is_open() {
            match action {
                InputAction::Quit => {
                    self.picker.close();
                }
                InputAction::Submit => {
                    if let Some(cmd) = self.picker.select_current() {
                        let ctx = self.picker.context;
                        self.picker.close();

                        // Sub-picker selection (no / prefix) — dispatch by context
                        if !cmd.starts_with('/') {
                            match ctx {
                                PickerContext::ThemeSelect => {
                                    self.set_theme(&cmd);
                                }
                                PickerContext::ModelSelect => {
                                    let clean = cmd.trim_end_matches(" ✓");
                                    self.set_model(clean);
                                }
                                PickerContext::Commands => {
                                    self.send_message_without_command_dispatch(&cmd);
                                }
                            }
                            self.reset_command_picker();
                            return;
                        }

                        if self.command_registry.find(&cmd).is_some() {
                            if let Err(err) =
                                self.execute_palette_command_sync(&cmd, serde_json::json!({}))
                            {
                                self.timeline
                                    .push(TimelineEntry::warning(format!("Command failed: {err}")));
                                self.scroll_offset = u16::MAX;
                            }
                            return;
                        }

                        self.send_message_without_command_dispatch(&cmd);
                    }
                }
                InputAction::ScrollUp => self.picker.move_up(),
                InputAction::ScrollDown => self.picker.move_down(),
                InputAction::PickerUp => self.picker.move_up(),
                InputAction::PickerDown => self.picker.move_down(),
                InputAction::Backspace => {
                    if self.picker.filter.is_empty() {
                        self.picker.close();
                    } else {
                        self.picker.pop_filter();
                    }
                }
                InputAction::Key(key) => {
                    if let crossterm::event::KeyCode::Char(ch) = key.code {
                        self.picker.push_filter(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        match action {
            InputAction::Quit => {
                if self.is_streaming() {
                    self.cancel_turn();
                } else {
                    self.should_quit = true;
                }
            }
            InputAction::Submit => self.submit_composer(),
            InputAction::InsertNewline => {
                self.composer.insert_newline();
                self.sync_users_status_from_composer();
            }
            InputAction::ToggleMode => {
                self.user_mode = match self.user_mode {
                    UserMode::Auto => UserMode::Plan,
                    UserMode::Plan => UserMode::Auto,
                };
            }
            InputAction::ToggleDetails => self.show_details = !self.show_details,
            InputAction::ScrollUp => {
                if self.users_status_mode == StatusMode::CommandList {
                    self.scroll_users_command_window(-1);
                } else {
                    self.resolve_scroll_sentinel();
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                }
            }
            InputAction::ScrollDown => {
                if self.users_status_mode == StatusMode::CommandList {
                    self.scroll_users_command_window(1);
                } else {
                    self.resolve_scroll_sentinel();
                    self.scroll_offset = self.scroll_offset.saturating_add(3);
                }
            }
            InputAction::PickerUp => {
                if self.users_status_mode == StatusMode::CommandList {
                    self.scroll_users_command_window(-1);
                } else {
                    self.composer.input(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Up,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
            }
            InputAction::PickerDown => {
                if self.users_status_mode == StatusMode::CommandList {
                    self.scroll_users_command_window(1);
                } else {
                    self.composer.input(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Down,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
            }
            InputAction::Key(key) => {
                self.composer.input(key);
                self.sync_users_status_from_composer();
            }
            InputAction::Backspace => {
                self.composer.input(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Backspace,
                    crossterm::event::KeyModifiers::NONE,
                ));
                self.sync_users_status_from_composer();
            }
            InputAction::Paste(text) => {
                debug!("handle_action: paste len={}", text.len());
                self.composer.insert_str(&text);
                self.sync_users_status_from_composer();
            }
        }
    }

    /// Request application quit. Safe to call at any time.
    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    /// Toggle debug overlay (show_details flag).
    pub fn toggle_debug_overlay(&mut self) {
        self.show_details = !self.show_details;
    }

    /// Clear all conversation messages and timeline entries, preserving only the welcome banner.
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.timeline
            .retain(|entry| matches!(entry.kind, crate::chat::model::EntryKind::Banner));
        self.has_user_sent = false;
        self.scroll_offset = u16::MAX;
    }

    /// Push a system hint message to the timeline and auto-scroll.
    pub fn push_system_hint(&mut self, message: impl Into<String>) {
        self.timeline
            .push(crate::chat::model::TimelineEntry::hint(message));
        self.scroll_offset = u16::MAX;
    }

    /// Push a system hint with details to the timeline and auto-scroll.
    pub fn push_system_hint_with_details(
        &mut self,
        message: impl Into<String>,
        details: impl Into<String>,
    ) {
        self.timeline
            .push(crate::chat::model::TimelineEntry::hint(message).with_details(details));
        self.scroll_offset = u16::MAX;
    }

    /// Returns the workspace root path for file operations.
    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }

    /// Find the last assistant message body from the timeline.
    pub fn last_assistant_message(&self) -> Option<String> {
        self.timeline
            .iter()
            .rev()
            .find(|entry| {
                entry.actor == crate::chat::model::Actor::Assistant
                    && matches!(entry.kind, crate::chat::model::EntryKind::Message)
            })
            .map(|entry| entry.body.clone())
    }

    /// Export conversation messages as JSON for /export command.
    pub fn export_conversation_json(&self) -> serde_json::Value {
        serde_json::json!({
            "workspace": self.display_path,
            "branch": self.branch,
            "model": self.model_name,
            "messages": self.messages.iter().map(|msg| {
                serde_json::json!({
                    "author": match msg.author {
                        crate::chat::model::Author::User => "user",
                        crate::chat::model::Author::Spirit => "assistant",
                    },
                    "body": msg.body,
                })
            }).collect::<Vec<_>>(),
        })
    }
}
