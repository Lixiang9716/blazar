use crate::agent::capability::{
    CapabilityClaim, CapabilityHandle, CapabilityInput, CapabilityResult,
};
use crate::agent::tools::Tool;

pub struct AcpToolCapability<'a> {
    tool: &'a dyn Tool,
}

impl<'a> AcpToolCapability<'a> {
    pub fn from_tool(tool: &'a dyn Tool) -> Self {
        Self { tool }
    }

    pub fn handle(&self) -> CapabilityHandle {
        CapabilityHandle::new(self.tool.spec().name, self.tool.kind().into())
    }

    pub fn claims(&self, input: &CapabilityInput) -> Vec<CapabilityClaim> {
        self.tool
            .resource_claims(&input.arguments)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn execute(&self, input: CapabilityInput) -> CapabilityResult {
        self.tool.execute(input.arguments).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capability::{CapabilityAccess, CapabilityContentPart, CapabilityKind};
    use crate::agent::tools::{ResourceAccess, ResourceClaim, ToolKind, ToolResult, ToolSpec};
    use serde_json::{Value, json};

    struct FakeAcpTool;

    impl crate::agent::tools::Tool for FakeAcpTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "acp-probe".into(),
                description: "fake acp tool".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            }
        }

        fn kind(&self) -> ToolKind {
            ToolKind::Agent { is_acp: true }
        }

        fn resource_claims(&self, args: &Value) -> Vec<ResourceClaim> {
            if args.get("workspace_lock").and_then(Value::as_bool) == Some(true) {
                vec![ResourceClaim {
                    resource: "fs:workspace.lock".into(),
                    access: ResourceAccess::ReadWrite,
                }]
            } else {
                Vec::new()
            }
        }

        fn execute(&self, _args: Value) -> ToolResult {
            ToolResult::success("acp-ok")
        }
    }

    #[test]
    fn acp_wrapper_exposes_handle_claims_and_result_via_capability_contracts() {
        let wrapper = AcpToolCapability::from_tool(&FakeAcpTool);
        let input = CapabilityInput::new(json!({"workspace_lock": true}));

        assert_eq!(
            wrapper.handle(),
            CapabilityHandle {
                name: "acp-probe".into(),
                kind: CapabilityKind::Agent { is_acp: true },
            }
        );
        assert_eq!(
            wrapper.claims(&input),
            vec![CapabilityClaim {
                resource: "fs:workspace.lock".into(),
                access: CapabilityAccess::ReadWrite,
            }]
        );

        let result = wrapper.execute(input);
        assert_eq!(
            result.content,
            vec![CapabilityContentPart::Text {
                text: "acp-ok".into(),
            }]
        );
    }
}
