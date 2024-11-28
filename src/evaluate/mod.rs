mod compilation;
mod language;
mod output;
pub mod queue_handler;
mod runnable;
mod types;

use crate::evaluate::compilation::CompilationError;
use crate::messages::{CheckerData, Evaluation, EvaluationLanguage};
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

// FIXME: don't look under this point, big W.I.P.

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EvaluationDashboard {
    pub id: u64,
    pub request_json: String,
    pub request_json_pretty: String,
    pub kind: EvaluationKind,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum EvaluationKind {
    Batch {
        code: String,
        language: EvaluationLanguage,
        time_limit: u32,
        memory_limit: u32,
        checker: Option<CheckerData>,
    },
    Interactive {
        code: String,
        language: EvaluationLanguage,
        time_limit: u32,
        memory_limit: u32,
        checker: CheckerData,
    },
    OutputOnly {
        output: String,
        checker: Option<CheckerData>,
    },
}

pub fn begin_evaluation(
    evaluation: &Evaluation,
    boxes: &[u8],
) -> Result<SuccessfulEvaluation, CompilationError> {
    // FIXME:
    let client = reqwest::blocking::Client::new();

    // TODO: env
    // FIXME: unwraps
    client
        .post("http://localhost:8888/api/evaluations")
        .json(&EvaluationDashboard::from((
            evaluation,
            serde_json::to_string(&evaluation).unwrap(),
        )))
        .send()
        .unwrap();

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

impl From<(&Evaluation, String)> for EvaluationDashboard {
    fn from((value, initial_payload): (&Evaluation, String)) -> Self {
        Self {
            id: value.get_evaluation_id(),
            request_json: initial_payload.to_string(),
            // FIXME: repetition
            request_json_pretty: initial_payload,
            kind: match value.clone() {
                Evaluation::Batch(batch) => EvaluationKind::Batch {
                    code: batch.code,
                    memory_limit: batch.memory_limit,
                    time_limit: batch.time_limit,
                    language: batch.language,
                    checker: batch.checker,
                },
                Evaluation::Interactive(interactive) => EvaluationKind::Interactive {
                    code: interactive.code,
                    memory_limit: interactive.memory_limit,
                    time_limit: interactive.time_limit,
                    language: interactive.language,
                    checker: interactive.checker,
                },
                Evaluation::OutputOnly(output_only) => EvaluationKind::OutputOnly {
                    output: output_only.output,
                    checker: output_only.checker,
                },
            },
        }
    }
}
