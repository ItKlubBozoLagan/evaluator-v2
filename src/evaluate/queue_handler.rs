use crate::evaluate::begin_evaluation;
use crate::messages::Message;
use crate::state::AppState;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;
use tracing::{debug, error};

const REDIS_EVALUATION_PUBSUB: &str = "evaluator_evaluations";

pub async fn handle(
    _state: Arc<AppState>,
    mut rx: Receiver<Message>,
    mut redis_connection: ConnectionManager,
) {
    while let Ok(Message::BeginEvaluation(evaluation)) = rx.recv().await {
        debug!("Got evaluation request: {evaluation:?}");
        // TODO: lock to thread
        let res = begin_evaluation(evaluation);

        dbg!(&res);

        let output_json = match &res {
            Ok(result) => {
                serde_json::to_string(result).expect("evaluation to json should have worked")
            }
            Err(_) => {
                // TODO: handle appropriately
                String::new()
            }
        };

        let publish_result = redis_connection
            .publish::<_, _, ()>(REDIS_EVALUATION_PUBSUB, output_json)
            .await;

        if let Err(err) = publish_result {
            error!("Failed to publish evaluation result: {err}");
        }
    }
}
