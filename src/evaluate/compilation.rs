use crate::evaluate::runnable::{CompiledProcessData, PythonProcessData, RunnableProcess};
use crate::evaluate::{CompilationError, CompilationResult};
use crate::isolate::{CommandMeta, IsolatedProcess};
use crate::messages::EvaluationLanguage;
use crate::util;

pub fn process_compilation(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    match language {
        EvaluationLanguage::Python => Ok(CompilationResult {
            process: RunnableProcess::Python(PythonProcessData {
                code: code.to_string(),
            }),
        }),
        _ => compile(code, language),
    }
}

fn compile(
    code: &str,
    language: &EvaluationLanguage,
) -> Result<CompilationResult, CompilationError> {
    let output_file = util::random_bytes(32);

    // TODO: copy out of box
    //  NOTE(antony): spawn hooks (pre/post)
    let (compiler, args) = language
        .get_compiler_command(&output_file)
        .ok_or_else(|| CompilationError::UnsupportedLanguage(language.clone()))?;

    let mut process = IsolatedProcess::new(
        0,
        CommandMeta {
            executable: compiler.to_string(),
            args,
            in_path: true,
        },
    )?;
    process.spawn(code.as_bytes())?;
    let output = process.wait_for_output()?;

    process.cleanup_and_reset()?;

    if !output.status.success() {
        return Err(CompilationError::CompilationError(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(CompilationResult {
        process: RunnableProcess::Compiled(CompiledProcessData {
            executable_name: output_file,
        }),
    })
}
