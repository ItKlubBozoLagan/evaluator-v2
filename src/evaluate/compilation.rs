use crate::evaluate::runnable::{CompiledProcessData, PythonProcessData, RunnableProcess};
use crate::evaluate::{CompilationError, CompilationResult};
use crate::isolate::{make_program_work_dir, wrap_isolate};
use crate::messages::EvaluationLanguage;
use crate::util;
use std::path::PathBuf;

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
    work_dir: &PathBuf,
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::random_bytes(32);

    let (compiler, args) = language
        .get_compiler_command(output_file.clone())
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let child = wrap_isolate(work_dir, (compiler, &args), None, code.as_bytes())?.spawn()?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(CompilationError::CompilationError(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(CompilationResult {
        process: RunnableProcess::Compiled(CompiledProcessData {
            work_dir: work_dir.clone(),
            executable_name: output_file,
        }),
    })
}
