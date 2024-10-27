use std::fmt::{Display, Formatter};

pub mod handler;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum ProblemType {}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EvaluationLanguage {
    C,
    Cpp,
    Python,
    Rust,
    Java,
    Go,
    None,
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

impl Evaluation {
    pub fn get_evaluation_id(&self) -> u64 {
        match self {
            Evaluation::Batch(BatchEvaluation { id, .. })
            | Evaluation::Interactive(InteractiveEvaluation { id, .. })
            | Evaluation::OutputOnly(OutputOnlyEvaluation { id, .. }) => *id,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Testcase {
    pub id: String,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Message {
    BeginEvaluation(Evaluation),
}
