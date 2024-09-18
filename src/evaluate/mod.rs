mod compilation;
mod language;
mod output;
pub mod queue_handler;
mod runnable;
mod types;

use crate::evaluate::compilation::CompilationError;
use crate::messages::Evaluation;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvaluationError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Compilation error: {0}")]
    CompilationError(#[from] CompilationError),
}

#[derive(Debug)]
pub struct SuccessfulEvaluation {
    verdict: Verdict,
    max_time: u32,
    max_memory: u32,
    testcases: Vec<TestcaseResult>,
}

#[derive(Debug)]
pub struct TestcaseResult {
    id: u64,
    verdict: Verdict,
    time: u32,
    memory: u32,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    Custom(String),
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    CompilationError,
    JudgingError,
    SystemError,
    Skipped,
}

// TODO: wrap everything in isolate
pub fn begin_evaluation(evaluation: Evaluation) -> Result<SuccessfulEvaluation, EvaluationError> {
    match evaluation {
        Evaluation::Batch(batch_evaluation) => types::batch::evaluate(&batch_evaluation),
        Evaluation::OutputOnly(output_only_evaluation) => {
            types::output_only::evaluate(&output_only_evaluation)
        }
        Evaluation::Interactive(interactive_evaluation) => {
            types::interactive::evaluate(&interactive_evaluation)
        }
    }
}
