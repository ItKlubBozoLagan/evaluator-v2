use crate::evaluate::runnable::{
    CompiledProcessData, JavaProcessData, PythonProcessData, RunnableProcess,
};
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess, ProcessInput};
use crate::messages::EvaluationLanguage;
use crate::util;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug)]
pub struct CompilationResult {
    pub process: RunnableProcess,
    // compilation result is generic and is used for all languages (including interpreted ones)
    //  if the compilation step is done, it's stderr will be here
    pub compiler_stderr: Option<String>,
}

#[derive(Error, Debug)]
pub enum CompilationError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to compile: {0}")]
    CompilationProcessError(String),

    #[error("Tried to compile a non-compiled language: ${0}")]
    UnsupportedLanguage(EvaluationLanguage),

    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),
}

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
    box_id: u8,
) -> Result<CompilationResult, CompilationError> {
    match language {
        EvaluationLanguage::Python => Ok(CompilationResult {
            process: RunnableProcess::Python(PythonProcessData {
                code: code.to_string(),
            }),
            compiler_stderr: None,
        }),
        _ => compile(code, language, box_id),
    }
}

fn compile(
    code: &str,
    language: &EvaluationLanguage,
    box_id: u8,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::general::random_bytes(8);
    let file_path = PathBuf::from("/tmp").join(&output_file);

    let (compiler, args, dir_mounts) = language
        .get_compiler_command(&output_file)
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let mut process = IsolatedProcess::new(
        box_id,
        &CommandMeta {
            executable: compiler.to_string(),
            args,
            in_path: true,
            system: true,
        },
        // TODO: extract into constants
        &IsolateLimits {
            time_limit: 30.0,
            memory_limit: 1 << 20, // 1 GiB
        },
        dir_mounts,
    )?;

    process.spawn(ProcessInput::StdIn(code.as_bytes().to_vec()), None)?;

    let output = process.wait_for_output()?;

    let compiler_stderr = Some(String::from_utf8_lossy(&output.stderr).to_string());

    if !output.status.success() {
        process.cleanup_and_reset()?;

        return Err(CompilationError::CompilationProcessError(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    process.move_out_of_box(&output_file, &file_path)?;
    process.cleanup_and_reset()?;

    match language {
        EvaluationLanguage::Java => Ok(CompilationResult {
            process: RunnableProcess::Java(JavaProcessData {
                built_class_name: file_path,
            }),
            compiler_stderr,
        }),
        _ => Ok(CompilationResult {
            process: RunnableProcess::Compiled(CompiledProcessData {
                executable_path: file_path,
            }),
            compiler_stderr,
        }),
    }
}
