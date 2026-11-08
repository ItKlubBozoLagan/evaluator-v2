use crate::environment::Environment;
use crate::evaluate::compile_cache;
use crate::evaluate::runnable::{
    CompiledProcessData, JavaProcessData, PythonProcessData, RunnableProcess,
};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess, ProcessInput};
use crate::messages::EvaluationLanguage;
use crate::util::temp_file::TempFile;
use std::path::Path;
use thiserror::Error;

// name of the compiled artifact inside the compilation box
pub const BINARY_NAME: &str = "compiled.bin";

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

#[derive(Debug)]
pub enum CompilerOutput {
    Success { binary: TempFile, stderr: String },
    // the compiler rejected the code, this is deterministic so it can be cached
    Rejected { stderr: String },
    // the compiler was interrupted (timeout, signal, OOM, sandbox error), not cached
    Aborted { stderr: String },
}

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
    box_id: u8,
) -> Result<CompilationResult, CompilationError> {
    if let EvaluationLanguage::Python = language {
        return Ok(CompilationResult {
            process: RunnableProcess::Python(PythonProcessData {
                code: code.to_string(),
            }),
            compiler_stderr: None,
        });
    }

    let output = match &Environment::get().compile_cache {
        Some(cache) => compile_cache::get_or_compile(cache, code, language, |out_dir| {
            compile(code, language, box_id, out_dir)
        })?,
        None => compile(code, language, box_id, Path::new("/tmp"))?,
    };

    let (binary, stderr) = match output {
        CompilerOutput::Success { binary, stderr } => (binary, stderr),
        CompilerOutput::Rejected { stderr } | CompilerOutput::Aborted { stderr } => {
            return Err(CompilationError::CompilationProcessError(stderr));
        }
    };

    Ok(CompilationResult {
        process: match language {
            EvaluationLanguage::Java => RunnableProcess::Java(JavaProcessData {
                built_class_name: binary,
            }),
            _ => RunnableProcess::Compiled(CompiledProcessData {
                executable_path: binary,
            }),
        },
        compiler_stderr: Some(stderr),
    })
}

// compiles the code in isolate, on success the binary is placed in `out_dir`
fn compile(
    code: &str,
    language: &EvaluationLanguage,
    box_id: u8,
    out_dir: &Path,
) -> Result<CompilerOutput, CompilationError> {
    let (compiler, args, dir_mounts) = language
        .get_compiler_command(BINARY_NAME)
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
    let meta = process.load_meta()?;

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let result = if output.status.success() {
        let binary = TempFile::new_in(out_dir);
        process.move_out_of_box(BINARY_NAME, binary.path())?;
        CompilerOutput::Success { binary, stderr }
    } else if matches!(meta.status, Some(ProcessStatus::RuntimeError)) && !meta.cg_oom_killed {
        CompilerOutput::Rejected { stderr }
    } else {
        CompilerOutput::Aborted { stderr }
    };

    process.cleanup_and_reset()?;

    Ok(result)
}
