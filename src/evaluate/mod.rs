mod runnable;
mod language;
pub mod queue_handler;
mod types;
mod compilation;
mod output;

use crate::evaluate::runnable::RunnableProcess;
use crate::messages::{Evaluation, EvaluationLanguage, ProblemType};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvaluationError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Compilation error: {0}")]
    CompilationError(#[from] CompilationError)
}

#[derive(Debug)]
pub struct SuccessfulEvaluation {
    verdict: Verdict,
    max_time: u32,
    max_memory: u32,
    testcases: Vec<TestcaseResult>
}

#[derive(Debug)]
pub struct TestcaseResult {
    id: u32,
    verdict: Verdict,
    time: u32,
    memory: u32,
    error: Option<String>
}

#[derive(Debug)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    CompilationError,
    JudgingError,
    SystemError,
    Skipped
}

#[derive(Debug)]
pub struct CompilationResult {
    process: RunnableProcess,
}

#[derive(Error, Debug)]
pub enum CompilationError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to compile: {0}")]
    CompilationError(String),

    #[error("Tried to compile a non-compiled language: ${0}")]
    UnsupportedLanguage(EvaluationLanguage)
}

// TODO: wrap everything in isolate
pub fn begin_evaluation(evaluation: Evaluation) -> Result<SuccessfulEvaluation, EvaluationError> {
    match evaluation.problem_type {
        ProblemType::Batch => types::batch::evaluate(&evaluation),
        ProblemType::OutputOnly => types::output_only::evaluate(&evaluation),
        ProblemType::Interactive => types::interactive::evaluate(&evaluation)
    }
}