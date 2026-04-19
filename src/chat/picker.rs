/// Modal picker overlay — a bottom-anchored selection list.
/// Used for `/` command palette, confirmation dialogs, and setup wizards.

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
    pub selected: usize,
    pub scroll_offset: usize,
    pub visible: bool,
    pub filter: String,
}

impl ModalPicker {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Self {
            title: title.into(),
            items,
            selected: 0,
            scroll_offset: 0,
            visible: false,
            filter: String::new(),
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
        self.visible = true;
        self.selected = 0;
        self.scroll_offset = 0;
        self.filter.clear();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.filter.clear();
    }

    pub fn move_up(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            return;
        }
        self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
        // Wrap to bottom: scroll to end
        if self.selected == count - 1 && self.scroll_offset == 0 {
            self.scroll_offset = count.saturating_sub(PICKER_PAGE_SIZE);
        } else if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    pub fn move_down(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            return;
        }
        self.selected = (self.selected + 1) % count;
        // Wrap to top: scroll to start
        if self.selected == 0 {
            self.scroll_offset = 0;
        } else if self.selected >= self.scroll_offset + PICKER_PAGE_SIZE {
            self.scroll_offset = self.selected + 1 - PICKER_PAGE_SIZE;
        }
    }

    pub fn select_current(&self) -> Option<String> {
        let filtered = self.filtered_items();
        filtered.get(self.selected).map(|item| item.label.clone())
    }

    pub fn push_filter(&mut self, ch: char) {
        self.filter.push(ch);
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Returns the slice of filtered items visible in the current scroll window.
    pub fn visible_window(&self) -> (Vec<&PickerItem>, usize) {
        let filtered = self.filtered_items();
        let end = (self.scroll_offset + PICKER_PAGE_SIZE).min(filtered.len());
        let window: Vec<&PickerItem> = filtered[self.scroll_offset..end].to_vec();
        (window, self.scroll_offset)
    }

    pub fn has_scroll_up(&self) -> bool {
        self.scroll_offset > 0
    }

    pub fn has_scroll_down(&self) -> bool {
        let count = self.filtered_items().len();
        self.scroll_offset + PICKER_PAGE_SIZE < count
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
}
