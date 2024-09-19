#[derive(Debug, Clone)]
pub enum ProcessStatus {
    RuntimeError,
    SignalExit,
    TimedOut,
    SandboxError,
}

#[derive(Debug, Clone)]
pub struct ProcessMeta {
    pub cg_mem_kb: u32,
    pub status: Option<ProcessStatus>,
    pub time_ms: u32,
}

impl TryFrom<&String> for ProcessStatus {
    type Error = ();

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        match value as &str {
            "RE" => Ok(ProcessStatus::RuntimeError),
            "SG" => Ok(ProcessStatus::SignalExit),
            "TO" => Ok(ProcessStatus::TimedOut),
            "XX" => Ok(ProcessStatus::SandboxError),
            _ => Err(()),
        }
    }
}
