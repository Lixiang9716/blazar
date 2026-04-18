#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    User,
    Spirit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub author: Author,
    pub body: String,
}
