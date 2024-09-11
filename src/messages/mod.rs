use std::fmt::{Display, Formatter};

pub mod handler;


#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum ProblemType {
    Batch,
    Interactive,
    OutputOnly,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
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
pub struct Evaluation {
    pub id: u64,
    pub problem_type: ProblemType,
    pub code: String,
    pub language: EvaluationLanguage,
    pub testcases: Vec<Testcase>,
    pub time_limit: u64,
    pub memory_limit: u64,
    pub checker_script: Option<String>,
    pub checker_language: Option<EvaluationLanguage>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Testcase {
    pub id: u64,
    pub input: String,
    pub output: String
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Message {
    BeginEvaluation(Evaluation)
}