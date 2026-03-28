mod cache;
mod ctx;

use crate::environment::Environment;
use crate::evaluate::compilation::ctx::CompilationCtx;
use crate::evaluate::runnable::{PythonProcessData, RunnableProcess};
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess, ProcessInput};
use crate::messages::EvaluationLanguage;
use std::borrow::Cow;
use thiserror::Error;
use tracing::debug;

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

    #[error("syscall error: {0}")]
    NixError(#[from] nix::Error),

    #[error("compilation cache in an unsupported state: {0}")]
    InvalidCache(Cow<'static, str>),
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
    let ctx = CompilationCtx::new(code, language, box_id);

    let code_mutex = cache::mutex_by_code(&ctx);
    let _guard = code_mutex.lock().expect("code_mutex poisoned");

    if let Some(result) = cache::check_cached_compile(&ctx)? {
        return Ok(result);
    }

    debug!("compiling new binary");

    let paths = &ctx.tmp_paths;

    let (compiler, args, dir_mounts) = ctx
        .language
        .get_compiler_command(&ctx.binary_name)
        .ok_or_else(|| CompilationError::UnsupportedLanguage(ctx.language.clone()))?;

    let mut process = IsolatedProcess::new(
        ctx.box_id,
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

    process.spawn(ProcessInput::StdIn(ctx.code.as_bytes().to_vec()), None)?;

    let output = process.wait_for_output()?;

    let compiler_stderr = Some(String::from_utf8_lossy(&output.stderr).to_string());

    if !output.status.success() {
        process.cleanup_and_reset()?;

        return Err(CompilationError::CompilationProcessError(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    process.move_out_of_box(&ctx.binary_name, &paths.binary)?;
    process.cleanup_and_reset()?;

    if Environment::get().compile_cache.is_some() {
        cache::finalize_compile_cache(&ctx, &output.stderr)?;
    }

    Ok(CompilationResult {
        process: RunnableProcess::new_compiled(language, paths.binary.clone()),
        compiler_stderr,
    })
}
