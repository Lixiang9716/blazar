//! Modal picker overlay — a bottom-anchored selection list.
//! Used for `/` command palette, confirmation dialogs, and setup wizards.

use tui_overlay::OverlayState;
use tui_widget_list::ListState;

pub const PICKER_PAGE_SIZE: usize = 6;

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub description: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModalPicker {
    pub title: String,
    pub items: Vec<PickerItem>,
    pub filter: String,
    overlay_state: OverlayState,
    list_state: ListState,
}

impl ModalPicker {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Self {
            title: title.into(),
            items,
            filter: String::new(),
            overlay_state: OverlayState::new(),
            list_state: ListState::default(),
        }
    }

    pub fn command_palette() -> Self {
        Self::new(
            "Commands",
            vec![
                PickerItem::new("/help", "Show available commands and shortcuts"),
                PickerItem::new("/clear", "Clear the conversation history"),
                PickerItem::new("/copy", "Copy the last response to the clipboard"),
                PickerItem::new("/init", "Generate a blazar-instructions.md file"),
                PickerItem::new("/skills", "List loaded skills and their status"),
                PickerItem::new("/model", "Switch the active model"),
                PickerItem::new("/mcp", "Manage MCP server configuration"),
                PickerItem::new("/theme", "Switch the color theme"),
                PickerItem::new("/history", "Browse conversation history"),
                PickerItem::new("/export", "Export conversation to file"),
                PickerItem::new("/compact", "Compact conversation context"),
                PickerItem::new("/config", "Open configuration settings"),
                PickerItem::new("/tools", "List available tools"),
                PickerItem::new("/agents", "List running background agents"),
                PickerItem::new("/context", "Show current context window usage"),
                PickerItem::new("/diff", "Show pending file changes"),
                PickerItem::new("/git", "Show git repository status"),
                PickerItem::new("/undo", "Undo last file change"),
                PickerItem::new("/terminal", "Open a shell terminal"),
                PickerItem::new("/debug", "Toggle debug overlay"),
                PickerItem::new("/log", "Show application logs"),
                PickerItem::new("/quit", "Exit Blazar"),
            ],
        )
    }

    pub fn open(&mut self) {
        self.overlay_state.open();
        self.filter.clear();
        self.reset_selection();
    }

    pub fn close(&mut self) {
        self.overlay_state.close();
        self.filter.clear();
    }

    pub fn is_visible(&self) -> bool {
        self.overlay_state.is_open() || self.overlay_state.is_animating()
    }

    pub fn overlay_state(&self) -> &OverlayState {
        &self.overlay_state
    }

    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected
    }

    pub fn move_up(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.list_state.select(None);
            return;
        }
        let current = self
            .list_state
            .selected
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        let next = current.checked_sub(1).unwrap_or(count - 1);
        self.list_state.select(Some(next));
    }

    pub fn move_down(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.list_state.select(None);
            return;
        }
        let current = self
            .list_state
            .selected
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        let next = (current + 1) % count;
        self.list_state.select(Some(next));
    }

    pub fn select_current(&self) -> Option<String> {
        let filtered = self.filtered_items();
        let index = self
            .list_state
            .selected
            .map(|selected| selected.min(filtered.len().saturating_sub(1)))?;
        filtered.get(index).map(|item| item.label.clone())
    }

    pub fn push_filter(&mut self, ch: char) {
        self.filter.push(ch);
        self.reset_selection();
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.reset_selection();
    }

    /// Returns the slice of filtered items visible in the current scroll window.
    pub fn visible_window(&self) -> (Vec<&PickerItem>, usize) {
        let filtered = self.filtered_items();
        let offset = self.window_offset(filtered.len());
        let end = (offset + PICKER_PAGE_SIZE).min(filtered.len());
        let window: Vec<&PickerItem> = filtered[offset..end].to_vec();
        (window, offset)
    }

    pub fn has_scroll_up(&self) -> bool {
        let count = self.filtered_items().len();
        self.window_offset(count) > 0
    }

    pub fn has_scroll_down(&self) -> bool {
        let count = self.filtered_items().len();
        let offset = self.window_offset(count);
        offset + PICKER_PAGE_SIZE < count
    }

    pub fn filtered_items(&self) -> Vec<&PickerItem> {
        if self.filter.is_empty() {
            self.items.iter().collect()
        } else {
            self.items
                .iter()
                .filter(|item| {
                    item.label
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
                        || item
                            .description
                            .to_lowercase()
                            .contains(&self.filter.to_lowercase())
                })
                .collect()
        }
    }

    fn reset_selection(&mut self) {
        if self.filtered_items().is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn window_offset(&self, count: usize) -> usize {
        let selected = self
            .list_state
            .selected
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        if selected >= PICKER_PAGE_SIZE {
            selected + 1 - PICKER_PAGE_SIZE
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModalPicker, PickerItem};

    #[test]
    fn open_close_tracks_visibility_with_overlay_state() {
        let mut picker = ModalPicker::new("Commands", vec![PickerItem::new("/help", "help")]);
        assert!(!picker.is_visible());

        picker.open();
        assert!(picker.is_visible());

        picker.close();
        assert!(!picker.is_visible());
    }

    #[test]
    fn filter_updates_reset_selection() {
        let mut picker = ModalPicker::new(
            "Commands",
            vec![
                PickerItem::new("/help", "help"),
                PickerItem::new("/clear", "clear"),
            ],
        );
        picker.open();
        picker.move_down();
        assert_eq!(picker.selected_index(), Some(1));

        picker.push_filter('h');
        assert_eq!(picker.selected_index(), Some(0));
    }
}
