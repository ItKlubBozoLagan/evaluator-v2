use crate::deadline::run_with_deadline;
use crate::environment::Environment;
use crate::evaluate::{EvaluationError, SuccessfulEvaluation, Verdict, begin_evaluation};
use crate::messages::EvaluationMeta;
use crate::state::AppState;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

pub async fn run_evaluation(
    state: Arc<AppState>,
    connection: ConnectionManager,
    serialized_size: usize,
    EvaluationMeta {
        output_queue,
        evaluation,
    }: EvaluationMeta,
    jobs: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let evaluation_id = evaluation.get_evaluation_id();
    let dequeued_at = Instant::now();
    if !response_key_matches(&Environment::get().redis_response_key_prefix, &output_queue) {
        anyhow::bail!("evaluation {evaluation_id} supplied an invalid response key");
    }

    if let Err(message) = evaluation.validate(serialized_size) {
        let result =
            SuccessfulEvaluation::error_for_evaluation(&evaluation, Verdict::SystemError, message);
        return publish_result(state, connection, &output_queue, &result).await;
    }

    let requested_memory = evaluation
        .memory_limit_kib()
        .checked_add(Environment::get().system_memory_reserve_kib)
        .ok_or_else(|| anyhow::anyhow!("evaluation {evaluation_id} memory request overflowed"))?;
    let admission = match tokio::time::timeout(
        Environment::get().job_timeout,
        state.admit(evaluation.needed_boxes(), requested_memory),
    )
    .await
    {
        Err(_) => {
            let result = SuccessfulEvaluation::error_for_evaluation(
                &evaluation,
                Verdict::SystemError,
                "evaluation timed out while waiting for worker capacity".to_string(),
            );
            return publish_result(state, connection, &output_queue, &result).await;
        }
        Ok(Ok(admission)) => admission,
        Ok(Err(err)) => {
            let result = SuccessfulEvaluation::error_for_evaluation(
                &evaluation,
                Verdict::SystemError,
                format!("evaluation rejected: {err}"),
            );
            return publish_result(state, connection, &output_queue, &result).await;
        }
    };

    state.jobs_started.fetch_add(1, Ordering::Relaxed);
    let state_for_job = state.clone();
    let execution_timeout = Environment::get()
        .job_timeout
        .saturating_sub(dequeued_at.elapsed());
    jobs.spawn(async move {
        let box_ids = admission.box_ids.clone();
        info!(evaluation_id, ?box_ids, "starting evaluation");
        let execution = tokio::task::spawn_blocking(move || {
            let result = run_with_deadline(execution_timeout, || {
                match begin_evaluation(&evaluation, &box_ids) {
                    Ok(result) => (result, false),
                    Err(error) => (evaluation_error(&evaluation, error), true),
                }
            });
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

fn response_key_matches(prefix: &str, key: &str) -> bool {
    if prefix.ends_with([':', '_']) {
        return key.starts_with(prefix);
    }
    key == prefix
        || key
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with([':', '_']))
}

fn evaluation_error(
    evaluation: &crate::messages::Evaluation,
    error: EvaluationError,
) -> SuccessfulEvaluation {
    match error {
        EvaluationError::ContestantCompilation(message) => SuccessfulEvaluation::error(
            evaluation.get_evaluation_id(),
            Verdict::CompilationError(message.clone()),
            message,
        ),
        EvaluationError::Judging(message) => {
            SuccessfulEvaluation::error_for_evaluation(evaluation, Verdict::JudgingError, message)
        }
        EvaluationError::System(message) => {
            SuccessfulEvaluation::error_for_evaluation(evaluation, Verdict::SystemError, message)
        }
        EvaluationError::Deadline => SuccessfulEvaluation::error_for_evaluation(
            evaluation,
            Verdict::SystemError,
            "evaluation exceeded the overall deadline".to_string(),
        ),
    }
}

pub async fn publish_result(
    state: Arc<AppState>,
    mut connection: ConnectionManager,
    output_queue: &str,
    result: &SuccessfulEvaluation,
) -> anyhow::Result<()> {
    let output_json = serde_json::to_string(result)?;
    let attempts = Environment::get().redis_publish_attempts;
    let mut delay = Duration::from_millis(200);
    let mut last_error = None;

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
                last_error = Some(err.to_string());
            }
        }
    }

    let reason = last_error.unwrap_or_else(|| "unknown Redis error".to_string());
    let retained = serde_json::to_string(&ResultDeadLetter {
        kind: "completed_result",
        output_queue,
        result: &output_json,
        reason: &reason,
    })?;
    push_dead_letter(&state, &mut connection, &retained)
        .await
        .map_err(|err| {
            anyhow::anyhow!("result publication and dead-letter retention both failed: {err}")
        })?;
    warn!("completed result moved to the Redis dead-letter list");
    Ok(())
}

#[derive(Serialize)]
struct ResultDeadLetter<'a> {
    kind: &'static str,
    output_queue: &'a str,
    result: &'a str,
    reason: &'a str,
}

#[derive(Serialize)]
pub struct DeadLetter<'a> {
    pub payload_bytes: usize,
    pub payload_sha256: &'a str,
    pub reason: &'a str,
    pub payload: Option<&'a str>,
}

pub async fn dead_letter(
    state: Arc<AppState>,
    mut connection: ConnectionManager,
    payload: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let hash = hex::encode(crate::util::hash::sha256(payload.as_bytes()));
    let record = serde_json::to_string(&DeadLetter {
        payload_bytes: payload.len(),
        payload_sha256: &hash,
        reason,
        payload: (payload.len() <= Environment::get().max_request_bytes).then_some(payload),
    })?;
    push_dead_letter(&state, &mut connection, &record).await
}

async fn push_dead_letter(
    state: &Arc<AppState>,
    connection: &mut ConnectionManager,
    record: &str,
) -> anyhow::Result<()> {
    let attempts = Environment::get().redis_publish_attempts;
    let mut delay = Duration::from_millis(200);
    for attempt in 1..=attempts {
        match connection
            .rpush::<_, _, ()>(&Environment::get().redis_dead_letter_key, record)
            .await
        {
            Ok(()) => {
                state.dead_lettered.fetch_add(1, Ordering::Relaxed);
                state.redis_connected.store(true, Ordering::Relaxed);
                return Ok(());
            }
            Err(err) if attempt < attempts => {
                state.publish_retries.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(2));
                warn!(attempt, "dead-letter publication failed, retrying: {err}");
            }
            Err(err) => return Err(err.into()),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::response_key_matches;

    #[test]
    fn response_key_requires_a_prefix_boundary() {
        assert!(response_key_matches(
            "evaluator_evaluations",
            "evaluator_evaluations_worker-1"
        ));
        assert!(response_key_matches("results:", "results:worker-1"));
        assert!(!response_key_matches(
            "evaluator_evaluations",
            "evaluator_evaluationsevil"
        ));
    }
}
