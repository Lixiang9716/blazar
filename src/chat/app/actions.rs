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
        let model_items: Vec<PickerItem> = crate::provider::available_models(&repo_str)
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
            InputAction::ToggleDetails => self.show_details = !self.show_details,
            InputAction::ScrollUp => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            InputAction::ScrollDown => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_add(3);
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
            _ => {}
        }
    }
}
