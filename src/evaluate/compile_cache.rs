//! On-disk cache of compilation results.
//!
//! Layout of the cache directory:
//!  - `<key>/` a complete entry with `status`, `stderr` and, for successful compilations, `bin`
//!  - `.tmp-*` entries being written or removed, and binaries in use by evaluations
//!
//! Entries are written under a temporary name and renamed into place, so an entry is either
//!  complete or absent, even when multiple evaluators share the directory.

use crate::environment::CompileCache;
use crate::evaluate::compilation::{BINARY_NAME, CompilationError, CompilerOutput};
use crate::messages::EvaluationLanguage;
use crate::util;
use crate::util::temp_file::{TEMP_FILE_PREFIX, TempFile, temp_path_in};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs::{self, File, Metadata};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError, TryLockError, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

const STATUS_SUCCESS: &str = "success";
const STATUS_REJECTED: &str = "rejected";

// temporary files older than this are leftovers from a crashed evaluator
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

lazy_static! {
    static ref KEY_LOCKS: Mutex<HashMap<[u8; 32], Weak<Mutex<()>>>> = Mutex::new(HashMap::new());
    static ref COMPILER_VERSIONS: Mutex<HashMap<&'static str, Option<String>>> =
        Mutex::new(HashMap::new());
    static ref EVICTION_LOCK: Mutex<()> = Mutex::new(());
}

pub fn get_or_compile(
    cache: &CompileCache,
    code: &str,
    language: &EvaluationLanguage,
    compile: impl FnOnce(&Path) -> Result<CompilerOutput, CompilationError>,
) -> Result<CompilerOutput, CompilationError> {
    let Some(key) = cache_key(code, language) else {
        return compile(Path::new("/tmp"));
    };

    let key_lock = lock_for_key(key);
    let guard = key_lock.lock().unwrap_or_else(PoisonError::into_inner);

    let entry = cache.cache_dir.join(hex::encode(key));

    match read_entry(&entry, &cache.cache_dir) {
        Ok(Some(output)) => {
            debug!("compile cache hit");
            return Ok(output);
        }
        Ok(None) => {}
        Err(err) => {
            warn!("discarding compile cache entry {}: {err}", entry.display());
            remove_entry(&cache.cache_dir, &entry);
        }
    }

    let output = compile(&cache.cache_dir)?;

    if let Err(err) = store_entry(&cache.cache_dir, &entry, &output) {
        warn!("failed to store compile cache entry: {err}");
    }

    drop(guard);

    if let Err(err) = evict(cache) {
        warn!("compile cache eviction failed: {err}");
    }

    Ok(output)
}

// None if the compiler version can't be determined, such compilations aren't cached
fn cache_key(code: &str, language: &EvaluationLanguage) -> Option<[u8; 32]> {
    let version = compiler_version(language)?;
    let (compiler, args, _) = language.get_compiler_command(BINARY_NAME)?;

    Some(util::hash::sha256_parts(&[
        format!("{language:?}").as_bytes(),
        compiler.as_bytes(),
        args.join("\0").as_bytes(),
        version.as_bytes(),
        code.as_bytes(),
    ]))
}

fn compiler_version(language: &EvaluationLanguage) -> Option<String> {
    let (program, args) = language.get_compiler_version_command()?;

    let mut versions = COMPILER_VERSIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    versions
        .entry(program)
        .or_insert_with(|| match Command::new(program).args(args).output() {
            Ok(output) if output.status.success() => Some(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )),
            _ => {
                warn!("failed to determine version of {program}, its compilations won't be cached");
                None
            }
        })
        .clone()
}

fn lock_for_key(key: [u8; 32]) -> Arc<Mutex<()>> {
    let mut locks = KEY_LOCKS.lock().unwrap_or_else(PoisonError::into_inner);

    locks.retain(|_, lock| lock.strong_count() > 0);

    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }

    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));

    lock
}

// the binary is hard-linked into a temporary file, so the entry can be evicted while in use
fn read_entry(entry: &Path, temp_dir: &Path) -> io::Result<Option<CompilerOutput>> {
    if !fs::exists(entry)? {
        return Ok(None);
    }

    let status = fs::read_to_string(entry.join("status"))?;
    let stderr = String::from_utf8_lossy(&fs::read(entry.join("stderr"))?).into_owned();

    let output = match status.as_str() {
        STATUS_SUCCESS => {
            let binary = TempFile::new_in(temp_dir);
            fs::hard_link(entry.join("bin"), binary.path())?;
            CompilerOutput::Success { binary, stderr }
        }
        STATUS_REJECTED => CompilerOutput::Rejected { stderr },
        other => return Err(io::Error::other(format!("unknown status: {other}"))),
    };

    // mark as recently used for eviction
    File::open(entry)?.set_modified(SystemTime::now())?;

    Ok(Some(output))
}

fn store_entry(cache_dir: &Path, entry: &Path, output: &CompilerOutput) -> io::Result<()> {
    let (status, stderr, binary) = match output {
        CompilerOutput::Success { binary, stderr } => (STATUS_SUCCESS, stderr, Some(binary)),
        CompilerOutput::Rejected { stderr } => (STATUS_REJECTED, stderr, None),
        CompilerOutput::Aborted { .. } => return Ok(()),
    };

    let staging = temp_path_in(cache_dir);

    let result =
        write_entry(&staging, status, stderr, binary).and_then(|()| fs::rename(&staging, entry));

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }

    match result {
        // another evaluator stored the same entry first
        Err(err) if err.kind() == ErrorKind::DirectoryNotEmpty => Ok(()),
        result => result,
    }
}

fn write_entry(
    dir: &Path,
    status: &str,
    stderr: &str,
    binary: Option<&TempFile>,
) -> io::Result<()> {
    fs::create_dir(dir)?;

    if let Some(binary) = binary {
        fs::hard_link(binary.path(), dir.join("bin"))?;
        File::open(dir.join("bin"))?.sync_all()?;
    }

    write_synced(&dir.join("stderr"), stderr.as_bytes())?;
    write_synced(&dir.join("status"), status.as_bytes())
}

fn write_synced(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(data)?;
    file.sync_all()
}

// renamed away first so readers never observe a partially removed entry
fn remove_entry(cache_dir: &Path, entry: &Path) {
    let trash = temp_path_in(cache_dir);
    if fs::rename(entry, &trash).is_ok() {
        let _ = fs::remove_dir_all(trash);
    }
}

// removes stale temporary files, then least recently used entries until the cache fits its size limit
fn evict(cache: &CompileCache) -> io::Result<()> {
    let _guard = match EVICTION_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(err)) => err.into_inner(),
        Err(TryLockError::WouldBlock) => return Ok(()),
    };

    let mut entries = Vec::new();
    let mut total_size = 0;

    for dir_entry in fs::read_dir(&cache.cache_dir)? {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();

        // removed concurrently
        let Ok(metadata) = dir_entry.metadata() else {
            continue;
        };

        if dir_entry
            .file_name()
            .to_string_lossy()
            .starts_with(TEMP_FILE_PREFIX)
        {
            if is_stale(&metadata) {
                let _ = if metadata.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
            }
            continue;
        }

        let size = entry_size(&path).unwrap_or(0);
        total_size += size;
        entries.push((metadata.modified()?, size, path));
    }

    entries.sort_unstable();

    for (_, size, path) in entries {
        if total_size <= cache.max_size_bytes {
            break;
        }

        remove_entry(&cache.cache_dir, &path);
        total_size -= size;
    }

    Ok(())
}

// uses ctime because hard links share the mtime of the cached binary, while linking updates ctime
fn is_stale(metadata: &Metadata) -> bool {
    let changed = UNIX_EPOCH + Duration::from_secs(metadata.ctime().max(0) as u64);
    changed.elapsed().is_ok_and(|age| age > STALE_TEMP_AGE)
}

fn entry_size(entry: &Path) -> io::Result<u64> {
    fs::read_dir(entry)?
        .map(|file| Ok(file?.metadata()?.len()))
        .sum()
}
