use crate::isolate::{wrap_isolate, IsolateError};
use std::process::Child;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessRunError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError)
}

#[derive(Debug)]
pub enum RunnableProcess {
    Compiled(String),
    Python(String)
}

impl RunnableProcess {
    pub fn run(&self, stdin: &[u8]) -> Result<Child, ProcessRunError> {
        Ok(match self {
            RunnableProcess::Compiled(file) => wrap_isolate((file, &[]), None, stdin)?
                .spawn()?,
            RunnableProcess::Python(code) =>
                wrap_isolate(("python", &["-c".to_string(), code.clone()]), None, stdin)?.spawn()?

        })
    }
}