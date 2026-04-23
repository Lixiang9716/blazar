pub trait AgentAdapterContractProbe {
    fn fetch_agent(&self) -> Result<(), String>;
    fn create_run(&self) -> Result<(), String>;
    fn poll_terminal(&self) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    pub failures: Vec<String>,
}

impl ConformanceReport {
    pub fn all_passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn run_adapter_conformance_suite<P: AgentAdapterContractProbe>(probe: P) -> ConformanceReport {
    let mut failures = Vec::new();
    if let Err(err) = probe.fetch_agent() {
        failures.push(format!("fetch_agent: {err}"));
        return ConformanceReport { failures };
    }
    if let Err(err) = probe.create_run() {
        failures.push(format!("create_run: {err}"));
        return ConformanceReport { failures };
    }
    if let Err(err) = probe.poll_terminal() {
        failures.push(format!("poll_terminal: {err}"));
    }
    ConformanceReport { failures }
}

#[cfg(test)]
#[path = "conformance_tests.rs"]
mod tests;
