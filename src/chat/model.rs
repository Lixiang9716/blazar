use crate::agent::tools::ToolKind;

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
pub enum ToolCallStatus {
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Message,
    Warning,
    Hint,
    ToolUse {
        tool: String,
        target: String,
        additions: u16,
        deletions: u16,
    },
    ToolCall {
        call_id: String,
        tool_name: String,
        kind: ToolKind,
        arguments: String,
        status: ToolCallStatus,
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
    pub title: Option<String>,
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
            title: None,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn user_message(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::User,
            kind: EntryKind::Message,
            title: None,
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
            title: None,
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
            title: None,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn tool_call(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        kind: ToolKind,
        arguments: impl Into<String>,
        body: impl Into<String>,
        details: impl Into<String>,
        status: ToolCallStatus,
    ) -> Self {
        Self {
            actor: Actor::Tool,
            kind: EntryKind::ToolCall {
                call_id: call_id.into(),
                tool_name: tool_name.into(),
                kind,
                arguments: arguments.into(),
                status,
            },
            title: None,
            body: body.into(),
            details: details.into(),
        }
    }

    pub fn thinking(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::Thinking,
            title: None,
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
            title: None,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn warning(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::System,
            kind: EntryKind::Warning,
            title: None,
            body: body.into(),
            details: String::new(),
        }
    }

    pub fn hint(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::System,
            kind: EntryKind::Hint,
            title: None,
            body: body.into(),
            details: String::new(),
        }
    }

    /// Set the expanded detail content.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}
