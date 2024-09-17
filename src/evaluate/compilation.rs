use crate::evaluate::runnable::{CompiledProcessData, PythonProcessData, RunnableProcess};
use crate::evaluate::{CompilationError, CompilationResult};
use crate::isolate::{make_program_work_dir, IsolatedProcess};
use crate::messages::EvaluationLanguage;
use crate::util;
use std::path::Path;

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let work_dir = make_program_work_dir()?;

    match language {
        EvaluationLanguage::Python => Ok(CompilationResult {
            process: RunnableProcess::Python(PythonProcessData {
                work_dir,
                code: code.to_string(),
            }),
        }),
        _ => compile(&work_dir, code, language),
    }
}

fn compile(
    work_dir: &Path,
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::random_bytes(32);

    let (compiler, args) = language
        .get_compiler_command(work_dir.join(&output_file).display().to_string())
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let mut process = IsolatedProcess::new(work_dir, (compiler, &args), true, None)?;
    process.spawn(code.as_bytes())?;
    let output = process.wait_for_output()?;

    if !output.status.success() {
        return Err(CompilationError::CompilationError(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(CompilationResult {
        process: RunnableProcess::Compiled(CompiledProcessData {
            work_dir: work_dir.to_path_buf(),
            executable_name: output_file,
        }),
    })
}
