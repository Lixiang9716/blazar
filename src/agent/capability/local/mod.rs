use crate::agent::capability::{
    CapabilityClaim, CapabilityHandle, CapabilityInput, CapabilityKind, CapabilityResult,
};
use crate::agent::tools::{ResourceClaim, Tool};

pub struct LocalToolCapability<'a> {
    tool: &'a dyn Tool,
}

impl<'a> LocalToolCapability<'a> {
    pub fn from_tool(tool: &'a dyn Tool) -> Self {
        Self { tool }
    }

    pub fn kind(&self) -> CapabilityKind {
        self.tool.kind().into()
    }

    pub fn handle(&self) -> CapabilityHandle {
        CapabilityHandle::new(self.tool.spec().name, self.kind())
    }

    pub fn claims(&self, input: &CapabilityInput) -> Vec<CapabilityClaim> {
        self.tool
            .resource_claims(&input.arguments)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn resource_claims(&self, input: &CapabilityInput) -> Vec<ResourceClaim> {
        self.claims(input).into_iter().map(Into::into).collect()
    }

    pub fn execute(&self, input: CapabilityInput) -> CapabilityResult {
        self.tool.execute(input.arguments).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capability::{CapabilityAccess, CapabilityContentPart, ConflictPolicy};
    use crate::agent::tools::{ResourceAccess, ToolResult, ToolSpec};
    use serde_json::{Value, json};

    struct FakeLocalTool;

    impl Tool for FakeLocalTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "fake".into(),
                description: "fake local tool".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            }
        }

        fn resource_claims(&self, args: &Value) -> Vec<ResourceClaim> {
            args.get("path")
                .and_then(Value::as_str)
                .map(|path| {
                    vec![ResourceClaim {
                        resource: format!("fs:{path}"),
                        access: ResourceAccess::ReadWrite,
                    }]
                })
                .unwrap_or_default()
        }

        fn execute(&self, args: Value) -> ToolResult {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            ToolResult::success(text)
        }
    }

    #[test]
    fn local_wrapper_projects_claims_and_results_to_capability_contracts() {
        let wrapper = LocalToolCapability::from_tool(&FakeLocalTool);
        let input = CapabilityInput::new(json!({"path":"src/main.rs", "text":"done"}));

        assert_eq!(
            wrapper.handle(),
            CapabilityHandle {
                name: "fake".into(),
                kind: CapabilityKind::Local,
            }
        );

        assert_eq!(wrapper.kind(), CapabilityKind::Local);
        assert_eq!(
            wrapper.claims(&input),
            vec![CapabilityClaim {
                resource: "fs:src/main.rs".into(),
                access: CapabilityAccess::ReadWrite,
            }]
        );

        let result = wrapper.execute(input);
        assert!(!result.is_error);
        assert_eq!(
            result.content,
            vec![CapabilityContentPart::Text {
                text: "done".into()
            }]
        );
    }

    #[test]
    fn local_wrapper_claims_keep_conflict_policy_semantics() {
        let wrapper = LocalToolCapability::from_tool(&FakeLocalTool);
        let write_claims = wrapper.claims(&CapabilityInput::new(json!({"path":"src/main.rs"})));
        let other_write_claims =
            wrapper.claims(&CapabilityInput::new(json!({"path":"src/main.rs"})));

        assert_eq!(
            ConflictPolicy::from_claims(&write_claims, &other_write_claims),
            ConflictPolicy::Conflicting
        );
    }
}
