use crate::environment::Environment;
use crate::messages::EvaluationLanguage;
use crate::util;
use std::path::PathBuf;

pub(super) struct CompilationCtx<'a> {
    pub(super) code: &'a str,
    pub(super) language: &'a EvaluationLanguage,
    pub(super) box_id: u8,

    pub(super) hash: [u8; 32],
    pub(super) binary_name: String,

    pub(super) tmp_paths: CompilationPaths,
    pub(super) persistent_paths: Option<CompilationPaths>,
}

pub(super) struct CompilationPaths {
    pub(super) binary: PathBuf,
    pub(super) stderr: PathBuf,
    pub(super) done: PathBuf,
}

fn gen_paths(
    root: PathBuf,
    binary_name: &str,
    stderr_name: &str,
    done_name: &str,
) -> CompilationPaths {
    CompilationPaths {
        binary: root.join(binary_name),
        stderr: root.join(stderr_name),
        done: root.join(done_name),
    }
}

impl<'a> CompilationCtx<'a> {
    pub(super) fn new(code: &'a str, language: &'a EvaluationLanguage, box_id: u8) -> Self {
        let code_hash = util::hash::sha256(code.as_bytes());
        let code_hash_hex = hex::encode(code_hash);

        let maybe_cache_suffix = if Environment::get().compile_cache.is_some() {
            &format!(".{}", language)
        } else {
            &format!(".{}", util::general::random_bytes(8))
        };

        let (binary_name, stderr_name, done_name) = (
            format!("{code_hash_hex}{maybe_cache_suffix}.bin"),
            format!("{code_hash_hex}{maybe_cache_suffix}.stderr"),
            format!("{code_hash_hex}{maybe_cache_suffix}.done"),
        );

        let persistent_paths = Environment::get().compile_cache.as_ref().map(|cache| {
            gen_paths(
                PathBuf::from(&cache.cache_dir),
                &binary_name,
                &stderr_name,
                &done_name,
            )
        });

        Self {
            code,
            language,
            box_id,
            hash: code_hash,
            tmp_paths: gen_paths(
                PathBuf::from("/tmp"),
                &binary_name,
                &stderr_name,
                &done_name,
            ),
            persistent_paths,
            binary_name,
        }
    }
}

impl CompilationPaths {
    pub(super) fn check_if_exist(&self) -> std::io::Result<bool> {
        Ok(std::fs::exists(&self.binary)?
            && std::fs::exists(&self.stderr)?
            && std::fs::exists(&self.done)?)
    }
}
