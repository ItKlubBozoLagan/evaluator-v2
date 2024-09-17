use crate::isolate::{CommandMeta, IsolateError, IsolatedProcess};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessRunError {
    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),
}

#[derive(Debug)]
pub struct CompiledProcessData {
    pub executable_name: String,
}

#[derive(Debug)]
pub struct PythonProcessData {
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
            RunnableProcess::Compiled(CompiledProcessData { executable_name }) => {
                IsolatedProcess::new(
                    0,
                    CommandMeta {
                        executable: executable_name.to_string(),
                        args: Vec::new(),
                        in_path: false,
                    },
                )?
            }
            RunnableProcess::Python(PythonProcessData { code }) => IsolatedProcess::new(
                0,
                CommandMeta {
                    executable: "/usr/bin/python3".to_string(),
                    args: vec!["-c".to_string(), code.clone()],
                    in_path: true,
                },
            )?,
        };

        process.spawn(stdin)?;

        let output = process.wait_for_output()?;

        process.cleanup_and_reset()?;

        Ok(output)
    }
}
