use crate::evaluate::compilation::ctx::CompilationCtx;
use crate::evaluate::compilation::{CompilationError, CompilationResult};
use crate::evaluate::runnable::RunnableProcess;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex, Weak};
use tracing::debug;

lazy_static! {
    static ref COMPILATION_LOCKS: Mutex<HashMap<[u8; 32], Weak<Mutex<()>>>> =
        Mutex::new(HashMap::new());
}

pub(super) fn check_cached_compile(
    ctx: &CompilationCtx,
) -> Result<Option<CompilationResult>, CompilationError> {
    // if this is None, then cache is disabled, so even the saved files in tmp should not be re-used
    let Some(ref persistent_paths) = ctx.persistent_paths else {
        return Ok(None);
    };

    let tmp_paths = &ctx.tmp_paths;

    if tmp_paths.check_if_exist()? {
        let stderr = std::fs::read(&tmp_paths.stderr)
            .map_err(|err| CompilationError::InvalidCache(err.to_string().into()))?;
        let stderr = String::from_utf8_lossy(&stderr).to_string();

        debug!("cached compiled binary found in /tmp");

        return Ok(Some(CompilationResult {
            process: RunnableProcess::new_compiled(ctx.language, tmp_paths.binary.clone()),
            compiler_stderr: Some(stderr),
        }));
    }

    // TODO: optimize this path
    if persistent_paths.check_if_exist()? {
        std::fs::copy(&persistent_paths.binary, &tmp_paths.binary)?;

        let stderr = std::fs::read(&persistent_paths.stderr)
            .map_err(|err| CompilationError::InvalidCache(err.to_string().into()))?;
        let stderr = String::from_utf8_lossy(&stderr).to_string();

        // will write stderr to /tmp, fsync everything and create a .done file
        finalize_compile_cache(ctx, stderr.as_bytes())?;

        assert!(matches!(tmp_paths.check_if_exist(), Ok(true)));

        return check_cached_compile(ctx);
    }

    Ok(None)
}

pub(super) fn cleanup_mutex_map(map: &mut HashMap<[u8; 32], Weak<Mutex<()>>>) {
    map.retain(|_, v| Weak::strong_count(v) > 0);
}

pub(super) fn mutex_by_code(ctx: &CompilationCtx) -> Arc<Mutex<()>> {
    let mut map_lock = COMPILATION_LOCKS.lock().expect("map_mutex poisoned");

    cleanup_mutex_map(&mut map_lock);

    if let Some(code_mutex) = map_lock.get(&ctx.hash).and_then(Weak::upgrade) {
        return code_mutex;
    };

    let new_code_mutex = Arc::new(Mutex::new(()));
    map_lock.insert(ctx.hash, Arc::downgrade(&new_code_mutex));

    drop(map_lock);

    new_code_mutex
}
pub(super) fn finalize_compile_cache(
    cache_ctx: &CompilationCtx,
    stderr: &[u8],
) -> Result<(), CompilationError> {
    let paths = &cache_ctx.tmp_paths;

    let mut stderr_file = File::create(&paths.stderr)?;
    stderr_file.write_all(stderr)?;
    nix::unistd::fsync(stderr_file.as_raw_fd())?;

    let binary_file = File::open(&paths.binary)?;
    nix::unistd::fsync(binary_file.as_raw_fd())?;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.done)?;

    Ok(())
}
