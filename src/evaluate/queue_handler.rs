use crate::environment::Environment;
use crate::evaluate::compilation::CompilationError;
use crate::evaluate::{SuccessfulEvaluation, Verdict, begin_evaluation};
use crate::messages::handler::MessageResult;
use crate::messages::{Evaluation, EvaluationMeta};
use crate::state::AppState;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tracing::{debug, error, info, warn};

pub async fn wait_for_available_boxes(state: Arc<AppState>) {
    loop {
        let used_box_ids = state.used_box_ids.lock().await;
        let used_box_ids_cnt = used_box_ids.len();
        drop(used_box_ids);

        if Environment::get().max_evaluations as usize - used_box_ids_cnt >= 2 {
            break;
        }

        state.available_boxes_notify.notified().await;
    }
}

pub async fn handle_evaluation(
    state: Arc<AppState>,
    redis_connection: &ConnectionManager,
    EvaluationMeta {
        output_queue,
        evaluation,
    }: EvaluationMeta,
) -> MessageResult {
    debug!("got evaluation request: {evaluation:#?}");

    let needed_boxes = match evaluation {
        Evaluation::Interactive(_) => 2,
        _ => 1,
    };

    let mut used_box_ids = state.used_box_ids.lock().await;
    let used_box_ids_cnt = used_box_ids.len();
    if Environment::get().max_evaluations as usize - used_box_ids_cnt < needed_boxes {
        // TODO: maybe system error to client
        error!("not enough boxes, woop woop");
        return MessageResult::Continue;
    }

    let available_box_ids = (0..Environment::get().max_evaluations)
        .filter(|id| !used_box_ids.contains(id))
        .take(needed_boxes)
        .collect::<Vec<_>>();

    used_box_ids.extend(&available_box_ids);

    let used_box_ids_cnt = used_box_ids.len();

    drop(used_box_ids);

    let publish_connection = redis_connection.clone();
    let handle_state = state.clone();
    Handle::current().spawn_blocking(move || {
        info!(
            "Starting evaluation {} with boxes {:?}",
            &evaluation.get_evaluation_id(),
            &available_box_ids
        );
        let res = begin_evaluation(&evaluation, &available_box_ids);
        info!(
            "Evaluation finished for {}",
            &evaluation.get_evaluation_id()
        );
        debug!("evaluation finished: {res:#?}");

        Handle::current().block_on(async move {
            let mut used_box_ids = handle_state.used_box_ids.lock().await;
            for id in &available_box_ids {
                used_box_ids.remove(id);
            }

            drop(used_box_ids);

            handle_state.available_boxes_notify.notify_waiters();
        });

        let result = match res {
            Ok(result) => result,
            Err(err) => {
                let error = match err {
                    CompilationError::CompilationProcessError(err) => err,
                    _ => err.to_string(),
                };

                SuccessfulEvaluation {
                    evaluation_id: evaluation.get_evaluation_id(),
                    verdict: Verdict::CompilationError(error.clone()),
                    testcases: vec![],
                    max_time: 0,
                    max_memory: 0,
                    compiler_output: Some(error),
                }
            }
        };

        let output_json =
            serde_json::to_string(&result).expect("evaluation to json should have worked");

        let publish_result = Handle::current().block_on(publish_result(
            publish_connection,
            &output_queue,
            &output_json,
        ));

        if let Err(err) = publish_result {
            error!("Failed to publish evaluation result: {err}");
        }
    });

    if Environment::get().max_evaluations as usize - used_box_ids_cnt <= 1 {
        wait_for_available_boxes(state.clone()).await;
        return MessageResult::Continue;
    }

    MessageResult::Continue
}

async fn publish_result(
    mut connection: ConnectionManager,
    output_queue: &str,
    output_json: &str,
) -> redis::RedisResult<()> {
    let attempts = Environment::get().redis_publish_attempts;
    let mut delay = Duration::from_millis(200);

    for attempt in 1..=attempts {
        match connection.rpush(output_queue, output_json).await {
            Ok(()) => return Ok(()),
            Err(err) if attempt < attempts => {
                warn!(attempt, "result publication failed, retrying: {err}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(2));
            }
            Err(err) => return Err(err),
        }
    }

    unreachable!()
}
