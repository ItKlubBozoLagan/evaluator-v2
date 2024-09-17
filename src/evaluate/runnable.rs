use crate::isolate::{IsolateError, IsolatedProcess};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessRunError {
    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),
}

#[derive(Debug)]
pub struct CompiledProcessData {
    pub work_dir: PathBuf,
    pub executable_name: String,
}

#[derive(Debug)]
pub struct PythonProcessData {
    pub work_dir: PathBuf,
    pub code: String,
}

#[derive(Debug)]
pub enum RunnableProcess {
    Compiled(CompiledProcessData),
    Python(PythonProcessData),
}

impl RunnableProcess {
    pub fn run(&self, stdin: &[u8]) -> Result<std::process::Output, ProcessRunError> {
        let mut process = match self {
            RunnableProcess::Compiled(CompiledProcessData {
                work_dir,
                executable_name,
            }) => IsolatedProcess::new(work_dir, (executable_name, &[]), false, None)?,
            RunnableProcess::Python(PythonProcessData { work_dir, code }) => IsolatedProcess::new(
                work_dir,
                ("/usr/bin/python3", &["-c".to_string(), code.clone()]),
                true,
                None,
            )?,
        };

        process.spawn(stdin)?;

        Ok(process.wait_for_output()?)
    }
}
