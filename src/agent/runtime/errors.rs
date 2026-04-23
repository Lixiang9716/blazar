#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    ProviderTransient,
    ProviderFatal,
    ProtocolInvalidPayload,
    ToolExecution,
    Cancelled,
}

impl RuntimeErrorKind {
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::ProviderTransient)
    }
}
