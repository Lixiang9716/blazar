#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub body: String,
}

pub struct ChatApp {
    messages: Vec<ChatMessage>,
}

impl ChatApp {
    pub fn new_for_test(_repo_path: &str) -> Self {
        Self {
            messages: vec![ChatMessage {
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}
