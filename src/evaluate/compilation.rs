use crate::evaluate::runnable::RunnableProcess;
use crate::evaluate::{CompilationError, CompilationResult};
use crate::isolate::wrap_isolate;
use crate::messages::EvaluationLanguage;
use crate::util;

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    match language {
        EvaluationLanguage::Python => Ok(CompilationResult {
            process: RunnableProcess::Python(code.to_string()),
        }),
        _ => compile(code, language),
    }
}

fn compile(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::random_bytes(32);

    let (compiler, args) = language
        .get_compiler_command(output_file.clone())
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let child = wrap_isolate((compiler, &args), None, code.as_bytes())?.spawn()?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(CompilationError::CompilationError(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(CompilationResult {
        process: RunnableProcess::Compiled(output_file),
    })
}
