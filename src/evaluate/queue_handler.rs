use crate::evaluate::begin_evaluation;
use crate::messages::Message;
use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;
use tracing::debug;

pub async fn handle(_state: Arc<AppState>, mut rx: Receiver<Message>) {
    while let Ok(Message::BeginEvaluation(evaluation)) = rx.recv().await {
        debug!("Got evaluation request: {evaluation:?}");
        // TODO: lock to thread
        begin_evaluation(evaluation);
    }
}