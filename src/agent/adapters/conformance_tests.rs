use super::run_adapter_conformance_suite;
use crate::agent::adapters::acp_client::AcpAdapterContractProbe;

#[test]
fn acp_adapter_satisfies_generic_agent_adapter_contract() {
    let report = run_adapter_conformance_suite(AcpAdapterContractProbe::default());
    assert!(
        report.all_passed(),
        "ACP adapter must satisfy generic contract"
    );
}
