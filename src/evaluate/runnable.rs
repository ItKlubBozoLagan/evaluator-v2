use crate::isolate::{wrap_isolate, IsolateError};
use std::path::PathBuf;
use std::process::Child;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessRunError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

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
    pub fn run(&self, stdin: &[u8]) -> Result<Child, ProcessRunError> {
        Ok(match self {
            RunnableProcess::Compiled(CompiledProcessData {
                work_dir,
                executable_name,
            }) => wrap_isolate(work_dir, (executable_name, &[]), false, None, stdin)?.spawn()?,
            RunnableProcess::Python(PythonProcessData { work_dir, code }) => wrap_isolate(
                work_dir,
                ("/usr/bin/python3", &["-c".to_string(), code.clone()]),
                true,
                None,
                stdin,
            )?
            .spawn()?,
        })
    }
}
