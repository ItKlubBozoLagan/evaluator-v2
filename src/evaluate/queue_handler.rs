use crate::evaluate::{begin_evaluation, SuccessfulEvaluation, Verdict};
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
        debug!("got evaluation request: {evaluation:#?}");
        // TODO: lock to thread
        let res = begin_evaluation(&evaluation);

        debug!("evaluation finished: {res:#?}");

        let result = match &res {
            Ok(result) => result,
            Err(err) => &SuccessfulEvaluation {
                evaluation_id: evaluation.get_evaluation_id(),
                verdict: Verdict::CompilationError(err.to_string()),
                testcases: vec![],
                max_time: 0,
                max_memory: 0,
            },
        };

        let output_json =
            serde_json::to_string(result).expect("evaluation to json should have worked");

        let publish_result = redis_connection
            .publish::<_, _, ()>(REDIS_EVALUATION_PUBSUB, output_json)
            .await;

        if let Err(err) = publish_result {
            error!("Failed to publish evaluation result: {err}");
        }
    }
}
