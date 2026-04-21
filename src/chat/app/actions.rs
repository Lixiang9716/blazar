use super::*;
use crate::chat::picker::{PickerContext, PickerItem};

impl ChatApp {
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
                                    self.send_message(&cmd);
                                }
                            }
                            self.picker = ModalPicker::command_palette();
                            return;
                        }

                        // /theme selected — open theme sub-picker
                        if cmd == "/theme" {
                            let theme_items: Vec<PickerItem> =
                                crate::chat::theme::available_themes()
                                    .into_iter()
                                    .map(|info| {
                                        PickerItem::new(
                                            info.name.clone(),
                                            info.display_name.clone(),
                                        )
                                    })
                                    .collect();
                            self.picker = ModalPicker::with_context(
                                "Select Theme",
                                theme_items,
                                PickerContext::ThemeSelect,
                            );
                            self.picker.open();
                            return;
                        }

                        // /model selected — open model sub-picker
                        if cmd == "/model" {
                            use crate::provider::siliconflow::POPULAR_MODELS;
                            let current = &self.model_name;
                            let model_items: Vec<PickerItem> = POPULAR_MODELS
                                .iter()
                                .map(|(name, desc)| {
                                    let label = if *name == current {
                                        format!("{name} ✓")
                                    } else {
                                        name.to_string()
                                    };
                                    PickerItem::new(label, *desc)
                                })
                                .collect();
                            self.picker = ModalPicker::with_context(
                                "Select Model",
                                model_items,
                                PickerContext::ModelSelect,
                            );
                            self.picker.open();
                            return;
                        }

                        if cmd == "/plan" {
                            self.set_composer_text("/plan ");
                            return;
                        }

                        self.send_message(&cmd);
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
                // Open command palette when typing '/' in empty composer
                if let crossterm::event::KeyCode::Char('/') = key.code
                    && self.composer_text().is_empty()
                {
                    self.picker.open();
                    return;
                }
                self.composer.input(key);
            }
            InputAction::Backspace => {
                self.composer.input(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Backspace,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            InputAction::Paste(text) => {
                debug!("handle_action: paste len={}", text.len());
                self.composer.insert_str(&text);
            }
            _ => {}
        }
    }
}
