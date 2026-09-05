use std::fmt::{Display, Formatter};

pub mod handler;

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationLanguage {
    C,
    Cpp,
    Python,
    Rust,
    Java,
    Go,
    GnuAsmX86Linux,
    #[serde(rename = "ocaml")]
    OCaml,
}

impl Display for EvaluationLanguage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CheckerData {
    pub script: String,
    pub language: EvaluationLanguage,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct BatchEvaluation {
    pub id: u64,
    pub code: String,
    pub language: EvaluationLanguage,
    pub testcases: Vec<Testcase>,
    pub time_limit: u32,
    pub memory_limit: u32,
    pub evaluate_all: bool,
    pub checker: Option<CheckerData>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InteractiveEvaluation {
    pub id: u64,
    pub code: String,
    pub language: EvaluationLanguage,
    pub testcases: Vec<Testcase>,
    pub time_limit: u32,
    pub memory_limit: u32,
    pub evaluate_all: bool,
    pub checker: CheckerData,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct OutputOnlyEvaluation {
    pub id: u64,
    pub output: String,
    pub testcase: Testcase,
    pub checker: Option<CheckerData>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Evaluation {
    Batch(BatchEvaluation),
    Interactive(InteractiveEvaluation),
    OutputOnly(OutputOnlyEvaluation),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EvaluationMeta {
    pub output_queue: String,
    pub evaluation: Evaluation,
}

impl Evaluation {
    pub fn get_evaluation_id(&self) -> u64 {
        match self {
            Evaluation::Batch(BatchEvaluation { id, .. })
            | Evaluation::Interactive(InteractiveEvaluation { id, .. })
            | Evaluation::OutputOnly(OutputOnlyEvaluation { id, .. }) => *id,
        }
    }

    pub fn needed_boxes(&self) -> u32 {
        if matches!(self, Evaluation::Interactive(_)) {
            2
        } else {
            1
        }
    }

    pub fn memory_limit_kib(&self) -> u32 {
        match self {
            Evaluation::Batch(value) => value.memory_limit,
            Evaluation::Interactive(value) => value.memory_limit,
            Evaluation::OutputOnly(_) => 0,
        }
    }

    pub fn testcases(&self) -> &[Testcase] {
        match self {
            Evaluation::Batch(value) => &value.testcases,
            Evaluation::Interactive(value) => &value.testcases,
            Evaluation::OutputOnly(value) => std::slice::from_ref(&value.testcase),
        }
    }

    pub fn validate(&self, serialized_size: usize) -> Result<(), String> {
        let environment = crate::environment::Environment::get();
        if serialized_size > environment.max_request_bytes {
            return Err("serialized request exceeds EVALUATOR_MAX_REQUEST_BYTES".to_string());
        }

        let (source, checker, testcases) = match self {
            Evaluation::Batch(value) => (
                Some(value.code.as_str()),
                value
                    .checker
                    .as_ref()
                    .map(|checker| checker.script.as_str()),
                value.testcases.as_slice(),
            ),
            Evaluation::Interactive(value) => (
                Some(value.code.as_str()),
                Some(value.checker.script.as_str()),
                value.testcases.as_slice(),
            ),
            Evaluation::OutputOnly(value) => {
                if value.output.len() > environment.max_output_bytes {
                    return Err("submitted output exceeds EVALUATOR_MAX_OUTPUT_BYTES".to_string());
                }
                (
                    None,
                    value
                        .checker
                        .as_ref()
                        .map(|checker| checker.script.as_str()),
                    std::slice::from_ref(&value.testcase),
                )
            }
        };

        if source.is_some_and(|source| source.len() > environment.max_source_bytes) {
            return Err("source exceeds EVALUATOR_MAX_SOURCE_BYTES".to_string());
        }
        if checker.is_some_and(|checker| checker.len() > environment.max_checker_bytes) {
            return Err("checker exceeds EVALUATOR_MAX_CHECKER_BYTES".to_string());
        }
        if testcases.len() > environment.max_testcases {
            return Err("testcase count exceeds EVALUATOR_MAX_TESTCASES".to_string());
        }

        let testcase_bytes = testcases.iter().try_fold(0_usize, |total, testcase| {
            total
                .checked_add(testcase.input.len())?
                .checked_add(testcase.output.len())
        });
        if testcase_bytes.is_none_or(|bytes| bytes > environment.max_testcase_bytes) {
            return Err("testcase data exceeds EVALUATOR_MAX_TESTCASE_BYTES".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Testcase {
    pub id: String,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum SystemMessage {
    Exit,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Message {
    BeginEvaluation(EvaluationMeta),
    System(SystemMessage),
}
