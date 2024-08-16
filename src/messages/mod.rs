pub mod handler;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Evaluation {
    pub code: String
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Message {
    BeginEvaluation(Evaluation)
}