use crate::environment::{CompileCache, Environment};
use crate::evaluate::runnable::{
    CompiledProcessData, JavaProcessData, PythonProcessData, RunnableProcess,
};
use crate::isolate::{CommandMeta, IsolateError, IsolateLimits, IsolatedProcess, ProcessInput};
use crate::messages::EvaluationLanguage;
use crate::util;
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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

struct CompilationCtx<'a> {
    code: &'a str,
    language: &'a EvaluationLanguage,
    box_id: u8,

    hash: [u8; 32],
    #[allow(unused)]
    hash_hex: String,
    binary_file_name: String,
    binary_path: PathBuf,
    stderr_path: PathBuf,
    done_path: PathBuf,
}

fn check_cached_compile(
    ctx: &CompilationCtx,
) -> Result<Option<CompilationResult>, CompilationError> {
    let env = Environment::get();
    #[allow(unused)]
    let Some(CompileCache { ref cache_dir }) = env.compile_cache else {
        return Ok(None);
    };

    let ram_exists = std::fs::exists(&ctx.done_path)?;
    if ram_exists {
        let stderr = std::fs::read_to_string(&ctx.stderr_path)
            .map_err(|err| CompilationError::InvalidCache(err.to_string().into()))?;

        if !std::fs::exists(&ctx.binary_path)? {
            return Err(CompilationError::InvalidCache(
                "binary doesn't exist".into(),
            ));
        }

        debug!("cached compiled binary found");

        return Ok(Some(CompilationResult {
            process: match ctx.language {
                EvaluationLanguage::Java => RunnableProcess::Java(JavaProcessData {
                    built_class_name: ctx.binary_path.clone(),
                }),
                _ => RunnableProcess::Compiled(CompiledProcessData {
                    executable_path: ctx.binary_path.clone(),
                }),
            },
            compiler_stderr: Some(stderr),
        }));
    }

    // TODO:
    Ok(None)
}

fn create_compile_ctx<'a>(
    code: &'a str,
    language: &'a EvaluationLanguage,
    box_id: u8,
) -> CompilationCtx<'a> {
    let code_hash = util::hash::sha256(code.as_bytes());
    let code_hash_hex = hex::encode(code_hash);

    let maybe_cache_suffix = if Environment::get().compile_cache.is_some() {
        ""
    } else {
        &format!(".{}", util::general::random_bytes(8))
    };

    let tmp_dir = PathBuf::from("/tmp");
    let binary_path_name = format!("{code_hash_hex}{maybe_cache_suffix}.bin");
    let (file_path, stderr_path, done_path) = (
        tmp_dir.join(&binary_path_name),
        tmp_dir.join(format!("{code_hash_hex}{maybe_cache_suffix}.stderr")),
        tmp_dir.join(format!("{code_hash_hex}{maybe_cache_suffix}.done")),
    );

    CompilationCtx {
        code,
        language,
        box_id,
        hash: code_hash,
        hash_hex: code_hash_hex,
        binary_file_name: binary_path_name,
        binary_path: file_path,
        stderr_path,
        done_path,
    }
}

lazy_static! {
    // TODO: memory leak
    static ref COMPILATION_LOCKS: Mutex<HashMap<[u8; 32], Arc<Mutex<()>>>> =
        Mutex::new(HashMap::new());
}

fn mutex_by_code(ctx: &CompilationCtx) -> Arc<Mutex<()>> {
    let mut map_lock = COMPILATION_LOCKS.lock().expect("map_mutex poisoned");

    let Some(code_mutex) = map_lock.get(&ctx.hash) else {
        map_lock.insert(ctx.hash, Arc::new(Mutex::new(())));

        drop(map_lock);

        return mutex_by_code(ctx);
    };

    Arc::clone(code_mutex)
}

fn compile(
    code: &str,
    language: &EvaluationLanguage,
    box_id: u8,
) -> Result<CompilationResult, CompilationError> {
    let ctx = create_compile_ctx(code, language, box_id);

    let code_mutex = mutex_by_code(&ctx);
    let _guard = code_mutex.lock().expect("code_mutex poisoned");
    debug!("taken lock for {}", ctx.hash_hex);

    if let Some(result) = check_cached_compile(&ctx)? {
        return Ok(result);
    }

    debug!("compiling new binary");

    let (compiler, args, dir_mounts) = ctx
        .language
        .get_compiler_command(&ctx.binary_file_name)
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

    process.move_out_of_box(&ctx.binary_file_name, &ctx.binary_path)?;
    process.cleanup_and_reset()?;

    if Environment::get().compile_cache.is_some() {
        finalize_compile_cache(&ctx, &output.stderr)?;
    }

    Ok(CompilationResult {
        process: match ctx.language {
            EvaluationLanguage::Java => RunnableProcess::Java(JavaProcessData {
                built_class_name: ctx.binary_path,
            }),
            _ => RunnableProcess::Compiled(CompiledProcessData {
                executable_path: ctx.binary_path,
            }),
        },
        compiler_stderr,
    })
}

fn finalize_compile_cache(
    cache_ctx: &CompilationCtx,
    stderr: &[u8],
) -> Result<(), CompilationError> {
    let mut stderr_file = File::create(&cache_ctx.stderr_path)?;
    stderr_file.write_all(stderr)?;
    nix::unistd::fsync(stderr_file.as_raw_fd())?;

    let binary_file = File::open(&cache_ctx.stderr_path)?;
    nix::unistd::fsync(binary_file.as_raw_fd())?;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&cache_ctx.done_path)?;

    Ok(())
}
