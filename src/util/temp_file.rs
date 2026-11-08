use crate::util::general::random_bytes;
use std::fs;
use std::path::{Path, PathBuf};

pub const TEMP_FILE_PREFIX: &str = ".tmp-";

/// Returns a unique path in `dir`, nothing is created.
pub fn temp_path_in(dir: &Path) -> PathBuf {
    dir.join(format!("{TEMP_FILE_PREFIX}{}", random_bytes(8)))
}

/// A unique file path that is removed when this value is dropped.
#[derive(Debug)]
pub struct TempFile(PathBuf);

impl TempFile {
    pub fn new_in(dir: &Path) -> Self {
        Self(temp_path_in(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
