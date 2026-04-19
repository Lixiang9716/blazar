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
        }
    }

    pub fn user_message(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::User,
            kind: EntryKind::Message,
            body: body.into(),
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
        }
    }

    pub fn bash(command: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Tool,
            kind: EntryKind::Bash {
                command: command.into(),
            },
            body: body.into(),
        }
    }

    pub fn thinking(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::Thinking,
            body: body.into(),
        }
    }

    pub fn code_block(language: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            actor: Actor::Assistant,
            kind: EntryKind::CodeBlock {
                language: language.into(),
            },
            body: body.into(),
        }
    }

    pub fn warning(body: impl Into<String>) -> Self {
        Self {
            actor: Actor::System,
            kind: EntryKind::Warning,
            body: body.into(),
        }
    }
}
