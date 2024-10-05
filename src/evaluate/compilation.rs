use crate::evaluate::runnable::{CompiledProcessData, PythonProcessData, RunnableProcess};
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess};
use crate::messages::EvaluationLanguage;
use crate::util;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug)]
pub struct CompilationResult {
    pub process: RunnableProcess,
}

#[derive(Error, Debug)]
pub enum CompilationError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to compile: {0}")]
    CompilationError(String),

    #[error("Tried to compile a non-compiled language: ${0}")]
    UnsupportedLanguage(EvaluationLanguage),

    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),
}

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    match language {
        EvaluationLanguage::Python => Ok(CompilationResult {
            process: RunnableProcess::Python(PythonProcessData {
                code: code.to_string(),
            }),
        }),
        _ => compile(code, language),
    }
}

fn compile(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::random_bytes(8);
    let file_path = PathBuf::from("/tmp").join(&output_file);

    let (compiler, args) = language
        .get_compiler_command(&output_file)
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let mut process = IsolatedProcess::new(
        0,
        &CommandMeta {
            executable: compiler.to_string(),
            args,
            in_path: true,
        },
        // TODO: extract into variables
        &IsolateLimits {
            time_limit: 30.0,
            memory_limit: 1 << 20, // 1 GiB
        },
    )?;

    process.spawn(code.as_bytes())?;

    let output = process.wait_for_output()?;

    process.move_out_of_box(&output_file, &file_path)?;
    process.cleanup_and_reset()?;

    if !output.status.success() {
        return Err(CompilationError::CompilationError(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(CompilationResult {
        process: RunnableProcess::Compiled(CompiledProcessData {
            executable_path: file_path,
        }),
    })
}
