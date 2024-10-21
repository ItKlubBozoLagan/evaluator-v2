use crate::isolate::meta::ProcessMeta;
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess, ProcessInput};
use crate::util;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::process::Output;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessRunError {
    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),
}

#[derive(Debug)]
pub struct CompiledProcessData {
    pub executable_path: PathBuf,
}

#[derive(Debug)]
pub struct PythonProcessData {
    pub code: String,
}

#[derive(Debug)]
pub struct JavaProcessData {
    pub built_class_name: PathBuf,
}

#[derive(Debug)]
pub enum RunnableProcess {
    Compiled(CompiledProcessData),
    Python(PythonProcessData),
    Java(JavaProcessData),
}

#[derive(Debug, Clone)]
pub struct ProcessRunResult {
    pub output: Output,
    pub meta: ProcessMeta,
}

impl RunnableProcess {
    pub fn run(
        &self,
        input: ProcessInput,
        limits: &IsolateLimits,
        output_pipe: Option<OwnedFd>,
    ) -> Result<ProcessRunResult, ProcessRunError> {
        let mut process = self.just_run(0, input, limits, output_pipe)?;

        let output = process.wait_for_output()?;

        let meta = process.load_meta()?;

        process.cleanup_and_reset()?;

        Ok(ProcessRunResult { output, meta })
    }

    pub fn just_run(
        &self,
        exec_id: u8,
        input: ProcessInput,
        limits: &IsolateLimits,
        output_pipe: Option<OwnedFd>,
    ) -> Result<IsolatedProcess, ProcessRunError> {
        let mut process = self.as_isolated(exec_id, limits)?;

        match self {
            RunnableProcess::Compiled(CompiledProcessData { executable_path }) => process
                .spawn_with_hooks(input, output_pipe, |isolated| {
                    isolated.copy_in_box(executable_path, "program")
                })?,
            RunnableProcess::Python(_) => process.spawn(input, output_pipe)?,
            RunnableProcess::Java(JavaProcessData { built_class_name }) => process
                .spawn_with_hooks(input, output_pipe, |isolated| {
                    isolated.copy_in_box(built_class_name, "Main.class")
                })?,
        };

        Ok(process)
    }

    // FIXME: execution_id
    pub fn as_isolated(
        &self,
        exec_id: u8,
        limits: &IsolateLimits,
    ) -> Result<IsolatedProcess, IsolateError> {
        let process = match self {
            RunnableProcess::Compiled(_) => IsolatedProcess::new(
                exec_id,
                &CommandMeta {
                    executable: "program".to_string(),
                    args: Vec::new(),
                    in_path: false,
                    system: false,
                },
                limits,
                vec![],
            )?,
            RunnableProcess::Python(PythonProcessData { code }) => IsolatedProcess::new(
                exec_id,
                &CommandMeta {
                    executable: "/usr/bin/python3".to_string(),
                    args: vec!["-c".to_string(), code.clone()],
                    in_path: true,
                    system: false,
                },
                limits,
                vec![],
            )?,
            RunnableProcess::Java(_) => IsolatedProcess::new(
                exec_id,
                &CommandMeta {
                    executable: "/usr/bin/java".to_string(),
                    args: vec!["Main".to_string()],
                    in_path: true,
                    system: false,
                },
                limits,
                util::ETC_JAVA_DIRECTORIES.clone(),
            )?,
        };

        Ok(process)
    }
}
