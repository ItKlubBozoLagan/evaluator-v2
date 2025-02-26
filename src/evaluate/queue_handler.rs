use crate::environment::Environment;
use crate::evaluate::compilation::CompilationError;
use crate::evaluate::{begin_evaluation, SuccessfulEvaluation, Verdict};
use crate::messages::{Evaluation, EvaluationMeta, Message, SystemMessage};
use crate::state::AppState;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::broadcast::Receiver;
use tracing::{debug, error, info, warn};

pub async fn handle(
    state: Arc<AppState>,
    mut rx: Receiver<Message>,
    redis_connection: ConnectionManager,
) {
    while let Ok(message) = rx.recv().await {
        if let Message::System(SystemMessage::Exit) = message {
            info!("Received system exit, stopping evaluation handler");
            break;
        }

        let Message::BeginEvaluation(EvaluationMeta {
            output_queue,
            evaluation,
        }) = message
        else {
            unreachable!();
        };

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
            continue;
        }

        let available_box_ids = (0..Environment::get().max_evaluations)
            .filter(|id| !used_box_ids.contains(id))
            .take(needed_boxes)
            .collect::<Vec<_>>();

        used_box_ids.extend(&available_box_ids);

        let used_box_ids_cnt = used_box_ids.len();

        drop(used_box_ids);

        let mut redis = redis_connection.clone();
        let handle_state = state.clone();
        let handle = Handle::current().spawn_blocking(move || {
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

                drop(used_box_ids)
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

            let publish_result = Handle::current()
                .block_on(async move { redis.rpush::<_, _, ()>(output_queue, output_json).await });

            // // maximum hold of 2.8 seconds (400ms + 800ms + 1600ms)
            // let publish_result = Handle::current().block_on(async move {
            //     publish_with_backoff(
            //         &mut redis,
            //         &Environment::get().redis_response_pubsub,
            //         &output_json,
            //         4,
            //     )
            //     .await
            // });

            if let Err(err) = publish_result {
                error!("Failed to publish evaluation result: {err}");
            }
        });

        if Environment::get().max_evaluations as usize - used_box_ids_cnt <= 1 {
            let handle_result = handle.await;

            if let Err(err) = handle_result {
                warn!("Execution handle failed: {err}");
            }
        }
    }
}

// pub async fn publish_with_backoff(
//     redis: &mut ConnectionManager,
//     channel: &str,
//     data: &str,
//     attempts: u8,
// ) -> Result<(), RedisError> {
//     let mut iter: u8 = 0;
//     loop {
//         let result = redis.publish::<_, _, ()>(channel, data).await;
//
//         let err = match result {
//             Ok(_) => return Ok(()),
//             Err(err) => err,
//         };
//
//         if iter + 1 >= attempts {
//             return Err(err);
//         }
//
//         let wait_duration = 400 * Duration::from_millis(2u64.pow(iter as u32));
//
//         debug!(
//             "Failed to publish to redis, waiting for {:?}",
//             wait_duration
//         );
//
//         tokio::time::sleep(wait_duration).await;
//         iter += 1;
//     }
// }
