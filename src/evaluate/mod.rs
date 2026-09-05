mod compilation;
mod language;
mod output;
pub mod queue_handler;
mod runnable;
mod types;

use crate::evaluate::compilation::CompilationError;
use crate::messages::Evaluation;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SuccessfulEvaluation {
    evaluation_id: u64,
    verdict: Verdict,
    max_time: u32,
    max_memory: u32,
    testcases: Vec<TestcaseResult>,
    compiler_output: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestcaseResult {
    pub id: String,
    pub verdict: Verdict,
    pub time: u32,
    pub memory: u32,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Verdict {
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "wrong_answer")]
    WrongAnswer,
    #[serde(rename = "custom")]
    Custom(String),
    #[serde(rename = "time_limit_exceeded")]
    TimeLimitExceeded,
    #[serde(rename = "memory_limit_exceeded")]
    MemoryLimitExceeded,
    #[serde(rename = "runtime_error")]
    RuntimeError,
    #[serde(rename = "judging_error")]
    JudgingError,
    #[serde(rename = "system_error")]
    SystemError,
    #[serde(rename = "compilation_error")]
    CompilationError(String),
    #[serde(rename = "skipped")]
    Skipped,
}

#[derive(Debug, thiserror::Error)]
pub enum EvaluationError {
    #[error("contestant compilation failed: {0}")]
    ContestantCompilation(#[from] CompilationError),
    #[error("judging failed: {0}")]
    Judging(CompilationError),
    #[error("worker failed: {0}")]
    System(CompilationError),
}

impl EvaluationError {
    pub fn contestant_compilation(error: CompilationError) -> Self {
        match error {
            CompilationError::CompilationProcessError(_) => Self::ContestantCompilation(error),
            error => Self::System(error),
        }
    }

    pub fn judging_compilation(error: CompilationError) -> Self {
        match error {
            CompilationError::CompilationProcessError(_) => Self::Judging(error),
            error => Self::System(error),
        }
    }
}

impl SuccessfulEvaluation {
    pub fn error(evaluation_id: u64, verdict: Verdict, message: String) -> Self {
        Self {
            evaluation_id,
            verdict,
            max_time: 0,
            max_memory: 0,
            testcases: vec![],
            compiler_output: Some(message),
        }
    }

    pub fn error_for_evaluation(
        evaluation: &Evaluation,
        verdict: Verdict,
        message: String,
    ) -> Self {
        let testcases = evaluation
            .testcases()
            .iter()
            .map(|testcase| TestcaseResult {
                id: testcase.id.clone(),
                verdict: verdict.clone(),
                time: 0,
                memory: 0,
                output: None,
                error: Some(message.clone()),
            })
            .collect();
        Self {
            evaluation_id: evaluation.get_evaluation_id(),
            verdict,
            max_time: 0,
            max_memory: 0,
            testcases,
            compiler_output: None,
        }
    }
}

pub fn aggregate_verdict(current: Verdict, next: Verdict) -> Verdict {
    let next_is_failure = !matches!(next, Verdict::Accepted | Verdict::Custom(_));

    // Preserve custom success details unless a later testcase fails.
    if current == Verdict::Accepted || (matches!(current, Verdict::Custom(_)) && next_is_failure) {
        return next;
    }
    current
}

pub fn begin_evaluation(
    evaluation: &Evaluation,
    boxes: &[u8],
) -> Result<SuccessfulEvaluation, EvaluationError> {
    match evaluation {
        Evaluation::Batch(batch_evaluation) => types::batch::evaluate(batch_evaluation, boxes[0]),
        Evaluation::OutputOnly(output_only_evaluation) => {
            types::output_only::evaluate(output_only_evaluation, boxes[0])
        }
        Evaluation::Interactive(interactive_evaluation) => {
            types::interactive::evaluate(interactive_evaluation, boxes[0], boxes[1])
        }
    }
}
