use crate::environment::Environment;
use crate::evaluate::{begin_evaluation, EvaluationError, SuccessfulEvaluation, Verdict};
use crate::messages::EvaluationMeta;
use crate::state::AppState;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

pub async fn run_evaluation(
    state: Arc<AppState>,
    connection: ConnectionManager,
    EvaluationMeta {
        output_queue,
        evaluation,
    }: EvaluationMeta,
    jobs: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let evaluation_id = evaluation.get_evaluation_id();
    let admission = state.admit(evaluation.needed_boxes()).await?;

    state.jobs_started.fetch_add(1, Ordering::Relaxed);
    let state_for_job = state.clone();
    jobs.spawn(async move {
        let box_ids = admission.box_ids.clone();
        info!(evaluation_id, ?box_ids, "starting evaluation");
        let execution = tokio::task::spawn_blocking(move || {
            let result = match begin_evaluation(&evaluation, &box_ids) {
                Ok(result) => (result, false),
                Err(error) => (evaluation_error(&evaluation, error), true),
            };
            drop(admission);
            result
        })
        .await
        .map_err(|err| anyhow::anyhow!("evaluation {evaluation_id} task failed: {err}"))?;

        let (result, failed) = execution;
        if failed {
            state_for_job.jobs_failed.fetch_add(1, Ordering::Relaxed);
        }
        publish_result(state_for_job.clone(), connection, &output_queue, &result).await?;
        state_for_job.jobs_completed.fetch_add(1, Ordering::Relaxed);
        info!(evaluation_id, "evaluation result published");
        Ok(())
    });

    Ok(())
}

fn evaluation_error(
    evaluation: &crate::messages::Evaluation,
    error: EvaluationError,
) -> SuccessfulEvaluation {
    match error {
        EvaluationError::ContestantCompilation(inner) => SuccessfulEvaluation::error(
            evaluation.get_evaluation_id(),
            Verdict::CompilationError(inner.to_string()),
            inner.to_string(),
        ),
        EvaluationError::Judging(inner) => SuccessfulEvaluation::error_for_evaluation(
            evaluation,
            Verdict::JudgingError,
            inner.to_string(),
        ),
        EvaluationError::System(inner) => SuccessfulEvaluation::error_for_evaluation(
            evaluation,
            Verdict::SystemError,
            inner.to_string(),
        ),
    }
}

async fn publish_result(
    state: Arc<AppState>,
    mut connection: ConnectionManager,
    output_queue: &str,
    result: &SuccessfulEvaluation,
) -> anyhow::Result<()> {
    let output_json = serde_json::to_string(result)?;
    let attempts = Environment::get().redis_publish_attempts;
    let mut delay = Duration::from_millis(200);

    for attempt in 1..=attempts {
        let publish_result: redis::RedisResult<()> =
            connection.rpush(output_queue, &output_json).await;
        match publish_result {
            Ok(()) => {
                state.redis_connected.store(true, Ordering::Relaxed);
                return Ok(());
            }
            Err(err) if attempt < attempts => {
                state.redis_connected.store(false, Ordering::Relaxed);
                state.publish_retries.fetch_add(1, Ordering::Relaxed);
                warn!(attempt, "result publication failed, retrying: {err}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(2));
            }
            Err(err) => {
                state.redis_connected.store(false, Ordering::Relaxed);
                state.jobs_failed.fetch_add(1, Ordering::Relaxed);
                error!("result publication failed after {attempts} attempts: {err}");
                return Err(anyhow::anyhow!(
                    "result publication failed after {attempts} attempts"
                ));
            }
        }
    }

    unreachable!()
}
