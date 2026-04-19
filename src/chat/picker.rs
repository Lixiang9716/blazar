/// Modal picker overlay — a bottom-anchored selection list.
/// Used for `/` command palette, confirmation dialogs, and setup wizards.

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
    pub visible: bool,
    pub filter: String,
}

impl ModalPicker {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Self {
            title: title.into(),
            items,
            selected: 0,
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
                PickerItem::new("/quit", "Exit Blazar"),
            ],
        )
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.selected = 0;
        self.filter.clear();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.filter.clear();
    }

    pub fn move_up(&mut self) {
        let filtered_count = self.filtered_items().len();
        if filtered_count > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(filtered_count - 1);
        }
    }

    pub fn move_down(&mut self) {
        let filtered_count = self.filtered_items().len();
        if filtered_count > 0 {
            self.selected = (self.selected + 1) % filtered_count;
        }
    }

    pub fn select_current(&self) -> Option<String> {
        let filtered = self.filtered_items();
        filtered.get(self.selected).map(|item| item.label.clone())
    }

    pub fn push_filter(&mut self, ch: char) {
        self.filter.push(ch);
        self.selected = 0;
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.selected = 0;
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
