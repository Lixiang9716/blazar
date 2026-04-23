/// Governance metadata for Tool-facade compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCompatibilityTier {
    /// Canonical implementation target inside the Capability Kernel architecture.
    KernelNative,
    /// Transitional Tool-facade shim retained for migration compatibility.
    CompatibilityBridge,
}

/// Static compatibility governance for known built-in tools.
pub fn compatibility_tier_for_name(name: &str) -> Option<ToolCompatibilityTier> {
    match name {
        "bash" => Some(ToolCompatibilityTier::CompatibilityBridge),
        _ => None,
    }
}
