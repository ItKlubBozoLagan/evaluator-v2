#[derive(Debug, Clone)]
pub enum ProcessStatus {
    RuntimeError,
    SignalExit,
    TimedOut,
    SandboxError,
}

#[derive(Debug, Clone)]
pub struct ProcessMeta {
    cg_mem_kb: u64,
    status: ProcessStatus,
    time: f64,
}
