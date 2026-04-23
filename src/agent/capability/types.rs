use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityInput {
    pub arguments: Value,
}

impl CapabilityInput {
    pub fn new(arguments: Value) -> Self {
        Self { arguments }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityKind {
    Local,
    Agent { is_acp: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityHandle {
    pub name: String,
    pub kind: CapabilityKind,
}

impl CapabilityHandle {
    pub fn new(name: impl Into<String>, kind: CapabilityKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityAccess {
    ReadOnly,
    ReadWrite,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityClaim {
    pub resource: String,
    pub access: CapabilityAccess,
}

impl CapabilityClaim {
    pub fn conflict_policy_with(&self, other: &Self) -> ConflictPolicy {
        ConflictPolicy::from_claims(std::slice::from_ref(self), std::slice::from_ref(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    Compatible,
    Conflicting,
}

impl ConflictPolicy {
    pub fn is_conflicting(self) -> bool {
        matches!(self, Self::Conflicting)
    }

    pub fn from_claims(left: &[CapabilityClaim], right: &[CapabilityClaim]) -> Self {
        if has_exclusive_claim(left) || has_exclusive_claim(right) {
            return Self::Conflicting;
        }

        if left.iter().any(|left_claim| {
            right.iter().any(|right_claim| {
                left_claim.resource == right_claim.resource
                    && !matches!(
                        (left_claim.access, right_claim.access),
                        (CapabilityAccess::ReadOnly, CapabilityAccess::ReadOnly)
                    )
            })
        }) {
            return Self::Conflicting;
        }

        Self::Compatible
    }
}

fn has_exclusive_claim(claims: &[CapabilityClaim]) -> bool {
    claims
        .iter()
        .any(|claim| matches!(claim.access, CapabilityAccess::Exclusive))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityError {
    pub message: String,
    pub code: Option<String>,
}

impl CapabilityError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }

    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

impl Display for CapabilityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(code) => write!(f, "{code}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl Error for CapabilityError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityContentPart {
    Text {
        text: String,
    },
    Resource {
        uri: String,
        mime_type: Option<String>,
    },
}

impl CapabilityContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    fn text_projection(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Resource { uri, mime_type } => match mime_type {
                Some(mime_type) => format!("[resource] {uri} ({mime_type})"),
                None => format!("[resource] {uri}"),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityResult {
    pub content: Vec<CapabilityContentPart>,
    pub exit_code: Option<i32>,
    pub is_error: bool,
    pub output_truncated: bool,
    pub error: Option<CapabilityError>,
}

impl CapabilityResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            content: vec![CapabilityContentPart::text(output)],
            exit_code: None,
            is_error: false,
            output_truncated: false,
            error: None,
        }
    }

    pub fn failure(output: impl Into<String>) -> Self {
        let message = output.into();
        Self {
            content: vec![CapabilityContentPart::text(message.clone())],
            exit_code: None,
            is_error: true,
            output_truncated: false,
            error: Some(CapabilityError::new(message)),
        }
    }

    pub fn from_error(error: CapabilityError) -> Self {
        let message = error.message.clone();
        Self {
            content: vec![CapabilityContentPart::text(message)],
            exit_code: None,
            is_error: true,
            output_truncated: false,
            error: Some(error),
        }
    }

    pub fn text_output(&self) -> String {
        // Keep this projection behavior in lockstep with ToolResult::text_output and
        // conversion bridges so mixed text/resource rendering stays behavior-stable.
        let mut output = String::new();
        let mut previous_was_resource = false;
        for part in &self.content {
            match part {
                CapabilityContentPart::Text { text } => {
                    if previous_was_resource && !output.ends_with('\n') && !text.starts_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(text);
                    previous_was_resource = false;
                }
                CapabilityContentPart::Resource { .. } => {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&part.text_projection());
                    previous_was_resource = true;
                }
            }
        }
        if output.is_empty()
            && let Some(error) = &self.error
        {
            output.push_str(&error.message);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_result_helpers_build_expected_flags() {
        let ok = CapabilityResult::success("done");
        assert_eq!(
            ok.content,
            vec![CapabilityContentPart::Text {
                text: "done".into()
            }]
        );
        assert_eq!(ok.text_output(), "done");
        assert!(!ok.is_error);
        assert_eq!(ok.exit_code, None);
        assert_eq!(ok.error, None);

        let err = CapabilityResult::failure("nope");
        assert_eq!(
            err.content,
            vec![CapabilityContentPart::Text {
                text: "nope".into()
            }]
        );
        assert_eq!(err.text_output(), "nope");
        assert!(err.is_error);
        assert!(!err.output_truncated);
        assert_eq!(err.error, Some(CapabilityError::new("nope")));
    }

    #[test]
    fn capability_result_text_output_summarizes_resource_content_parts() {
        let result = CapabilityResult {
            content: vec![CapabilityContentPart::Resource {
                uri: "file://workspace/out.txt".into(),
                mime_type: Some("text/plain".into()),
            }],
            exit_code: None,
            is_error: false,
            output_truncated: false,
            error: None,
        };

        assert_eq!(
            result.text_output(),
            "[resource] file://workspace/out.txt (text/plain)"
        );
    }

    #[test]
    fn conflict_policy_matches_scheduler_rules() {
        let read_claim = CapabilityClaim {
            resource: "fs:src/main.rs".into(),
            access: CapabilityAccess::ReadOnly,
        };
        let write_claim = CapabilityClaim {
            resource: "fs:src/main.rs".into(),
            access: CapabilityAccess::ReadWrite,
        };
        let exclusive_claim = CapabilityClaim {
            resource: "process:bash".into(),
            access: CapabilityAccess::Exclusive,
        };
        let other_read_claim = CapabilityClaim {
            resource: "fs:src/lib.rs".into(),
            access: CapabilityAccess::ReadOnly,
        };

        assert_eq!(
            ConflictPolicy::from_claims(
                std::slice::from_ref(&read_claim),
                std::slice::from_ref(&read_claim)
            ),
            ConflictPolicy::Compatible
        );
        assert_eq!(
            ConflictPolicy::from_claims(std::slice::from_ref(&read_claim), &[write_claim]),
            ConflictPolicy::Conflicting
        );
        assert_eq!(
            ConflictPolicy::from_claims(
                std::slice::from_ref(&other_read_claim),
                std::slice::from_ref(&exclusive_claim)
            ),
            ConflictPolicy::Conflicting
        );
        assert_eq!(
            read_claim.conflict_policy_with(&other_read_claim),
            ConflictPolicy::Compatible
        );
    }

    #[test]
    fn capability_error_display_includes_optional_code() {
        let plain = CapabilityError::new("boom");
        assert_eq!(plain.to_string(), "boom");

        let coded = CapabilityError::with_code("ACP_TIMEOUT", "timed out");
        assert_eq!(coded.to_string(), "ACP_TIMEOUT: timed out");
    }

    #[test]
    fn capability_handle_captures_identity_and_kind() {
        let handle = CapabilityHandle::new("read_file", CapabilityKind::Local);
        assert_eq!(
            handle,
            CapabilityHandle {
                name: "read_file".into(),
                kind: CapabilityKind::Local,
            }
        );
    }

    #[test]
    fn capability_text_output_falls_back_to_error_message_for_metadata_only_failures() {
        let result = CapabilityResult {
            content: Vec::new(),
            exit_code: None,
            is_error: false,
            output_truncated: false,
            error: Some(CapabilityError::with_code("ACP_TIMEOUT", "timed out")),
        };

        assert_eq!(result.text_output(), "timed out");
    }
}
