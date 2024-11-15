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
}

#[derive(Debug, Serialize)]
pub struct TestcaseResult {
    pub id: String,
    pub verdict: Verdict,
    pub time: u32,
    pub memory: u32,
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

pub fn begin_evaluation(
    evaluation: &Evaluation,
    boxes: &[u8],
) -> Result<SuccessfulEvaluation, CompilationError> {
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
