use crate::chat::model::{Author, ChatMessage};
use crate::config::MascotConfig;
use ratatui_textarea::TextArea;
use serde_json::Value;

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    composer: TextArea<'static>,
}

impl ChatApp {
    pub fn new_for_test(_repo_path: &str) -> Self {
        Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            composer: TextArea::default(),
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn send_message(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        self.messages.push(ChatMessage {
            author: Author::User,
            body: trimmed.to_owned(),
        });
        self.messages.push(ChatMessage {
            author: Author::Spirit,
            body: format!("Spirit: I hear you — {trimmed}"),
        });
    }

    pub fn set_composer_text(&mut self, value: &str) {
        self.composer = TextArea::from([value.to_owned()]);
    }

    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    pub fn submit_composer(&mut self) {
        let text = self.composer_text();
        self.send_message(&text);
        self.composer = TextArea::default();
    }
}

pub fn run_terminal_chat(
    _schema: Value,
    _mascot: MascotConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
