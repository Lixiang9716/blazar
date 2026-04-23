use super::AgentAdapterContractProbe;
use super::run_adapter_conformance_suite;
use crate::agent::adapters::acp_client::AcpAdapterContractProbe;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn acp_adapter_satisfies_generic_agent_adapter_contract() {
    let report = run_adapter_conformance_suite(AcpAdapterContractProbe::default());
    assert!(
        report.all_passed(),
        "ACP adapter must satisfy generic contract"
    );
}

#[derive(Clone)]
struct FetchFailureProbe {
    create_run_calls: Rc<Cell<usize>>,
    poll_terminal_calls: Rc<Cell<usize>>,
}

impl AgentAdapterContractProbe for FetchFailureProbe {
    fn fetch_agent(&self) -> Result<(), String> {
        Err("agent metadata endpoint unavailable".into())
    }

    fn create_run(&self) -> Result<(), String> {
        self.create_run_calls.set(self.create_run_calls.get() + 1);
        Ok(())
    }

    fn poll_terminal(&self) -> Result<(), String> {
        self.poll_terminal_calls
            .set(self.poll_terminal_calls.get() + 1);
        Ok(())
    }
}

#[test]
fn conformance_suite_skips_dependent_checks_when_fetch_fails() {
    let create_run_calls = Rc::new(Cell::new(0));
    let poll_terminal_calls = Rc::new(Cell::new(0));
    let probe = FetchFailureProbe {
        create_run_calls: Rc::clone(&create_run_calls),
        poll_terminal_calls: Rc::clone(&poll_terminal_calls),
    };

    let report = run_adapter_conformance_suite(probe);

    assert_eq!(
        report.failures,
        vec!["fetch_agent: agent metadata endpoint unavailable".to_string()]
    );
    assert_eq!(
        create_run_calls.get(),
        0,
        "create_run should be skipped after fetch_agent failure"
    );
    assert_eq!(
        poll_terminal_calls.get(),
        0,
        "poll_terminal should be skipped after fetch_agent failure"
    );
}

#[derive(Clone)]
struct CreateRunFailureProbe {
    poll_terminal_calls: Rc<Cell<usize>>,
}

impl AgentAdapterContractProbe for CreateRunFailureProbe {
    fn fetch_agent(&self) -> Result<(), String> {
        Ok(())
    }

    fn create_run(&self) -> Result<(), String> {
        Err("run creation failed".into())
    }

    fn poll_terminal(&self) -> Result<(), String> {
        self.poll_terminal_calls
            .set(self.poll_terminal_calls.get() + 1);
        Ok(())
    }
}

#[test]
fn conformance_suite_skips_poll_when_create_run_fails() {
    let poll_terminal_calls = Rc::new(Cell::new(0));
    let probe = CreateRunFailureProbe {
        poll_terminal_calls: Rc::clone(&poll_terminal_calls),
    };

    let report = run_adapter_conformance_suite(probe);

    assert_eq!(
        report.failures,
        vec!["create_run: run creation failed".to_string()]
    );
    assert_eq!(
        poll_terminal_calls.get(),
        0,
        "poll_terminal should be skipped after create_run failure"
    );
}

#[derive(Clone, Default)]
struct PollFailureProbe;

impl AgentAdapterContractProbe for PollFailureProbe {
    fn fetch_agent(&self) -> Result<(), String> {
        Ok(())
    }

    fn create_run(&self) -> Result<(), String> {
        Ok(())
    }

    fn poll_terminal(&self) -> Result<(), String> {
        Err("terminal polling failed".into())
    }
}

#[test]
fn conformance_suite_reports_poll_failures() {
    let report = run_adapter_conformance_suite(PollFailureProbe);
    assert_eq!(
        report.failures,
        vec!["poll_terminal: terminal polling failed".to_string()]
    );
}
