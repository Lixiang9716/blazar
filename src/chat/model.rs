#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    User,
    Spirit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Message,
    Warning,
    ToolUse {
        tool: String,
        target: String,
        additions: u16,
        deletions: u16,
    },
    Bash {
        command: String,
    },
    Thinking,
    CodeBlock {
        language: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub actor: Actor,
    pub kind: EntryKind,
    pub body: String,
    /// Expanded detail content shown when Ctrl+O is toggled.
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub author: Author,
    pub body: String,
}

impl TimelineEntry {
    pub fn response(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::Message,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn user_message(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::User,
            kind: EntryKind::Message,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn tool_use(
        tool: impl Into<String>,
        target: impl Into<String>,
        additions: u16,
        deletions: u16,
        body: impl Into<String>,
    ) -> Self {
        Self {
            actor: Actor::Tool,
            kind: EntryKind::ToolUse {
                tool: tool.into(),
                target: target.into(),
                additions,
                deletions,
            },
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn bash(command: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Tool,
            kind: EntryKind::Bash {
                command: command.into(),
            },
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn thinking(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::Thinking,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn code_block(language: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::CodeBlock {
                language: language.into(),
            },
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn warning(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::System,
            kind: EntryKind::Warning,
            body: body.into(),
            details: String::new(),
        }
    }

    /// Set the expanded detail content.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }
}
