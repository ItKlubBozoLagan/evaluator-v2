use crate::constants::{CACHE_TMP_DIR, CACHE_TMP_TIMEOUT_SECS};
use crate::environment::Environment;
use crate::evaluate::CompilationPaths;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{error, info};

fn make_paths(
    cache_dir: PathBuf,
    done_path: &Path,
) -> anyhow::Result<(CompilationPaths, CompilationPaths)> {
    let done_name = done_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .ok_or_else(|| anyhow::anyhow!("failed to get file name"))?;
    let target_prefix = done_name
        .strip_suffix(".done")
        .ok_or_else(|| anyhow::anyhow!("strip_suffix returned None"))?;

    let binary_name = format!("{target_prefix}.bin");
    let stderr_name = format!("{target_prefix}.stderr");
    let done_name = format!("{target_prefix}.done");

    Ok((
        CompilationPaths::new_with_root(
            PathBuf::from(CACHE_TMP_DIR),
            &binary_name,
            &stderr_name,
            &done_name,
        ),
        CompilationPaths::new_with_root(cache_dir, &binary_name, &stderr_name, &done_name),
    ))
}

async fn do_cleanup() -> anyhow::Result<u64> {
    let mut entries = tokio::fs::read_dir(CACHE_TMP_DIR).await?;

    let persistent_dir = Environment::get()
        .compile_cache
        .as_ref()
        .map(|cache| PathBuf::from(&cache.cache_dir));

    let mut cleared = 0;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if !path.extension().map(|ext| ext == "done").unwrap_or(false) {
            continue;
        }

        let now = SystemTime::now();

        let accessed_at = entry.metadata().await.and_then(|m| m.accessed())?;
        if now
            .duration_since(accessed_at)
            .expect("file accessed in the future")
            .as_secs()
            < CACHE_TMP_TIMEOUT_SECS
        {
            continue;
        }

        // remove if cache is not enabled
        let Some(ref cache_dir) = persistent_dir else {
            let (tmp_paths, _) = make_paths(PathBuf::new(), &path)?;

            tokio::fs::remove_file(tmp_paths.done).await?;
            tokio::fs::remove_file(tmp_paths.stderr).await?;
            tokio::fs::remove_file(tmp_paths.binary).await?;

            continue;
        };

        let (tmp_paths, persistent_paths) = make_paths(cache_dir.clone(), &path)?;

        // move if cache is enabled
        rename_cross_fs(tmp_paths.done, persistent_paths.done).await?;
        rename_cross_fs(tmp_paths.stderr, persistent_paths.stderr).await?;
        rename_cross_fs(tmp_paths.binary, persistent_paths.binary).await?;

        cleared += 1;
    }

    Ok(cleared)
}

async fn rename_cross_fs(src: PathBuf, dst: PathBuf) -> anyhow::Result<()> {
    tokio::fs::copy(&src, &dst).await?;
    // maybe fsync?
    tokio::fs::remove_file(&src).await?;

    Ok(())
}

pub async fn cleanup_cached_compiles() {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_mins(10));

    tokio::fs::create_dir_all(CACHE_TMP_DIR)
        .await
        .expect("failed to create cache dir");

    loop {
        interval.tick().await;

        match do_cleanup().await {
            Ok(removed) => {
                info!(evicted = removed, "compile cleanup cache ran successfully");
            }
            Err(err) => {
                error!(error = ?err, "compile cleanup cache failed");
            }
        };
    }
}
