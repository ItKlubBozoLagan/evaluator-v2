use crate::environment::Environment;
use crate::evaluate::queue_handler::run_evaluation;
use crate::messages::{Message, SystemMessage};
use crate::state::AppState;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{info, warn};

enum PullResult {
    Message(String),
    Empty,
    Exit,
}

pub async fn handle_messages(
    state: Arc<AppState>,
    mut connection: ConnectionManager,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut jobs = JoinSet::new();
    let mut failure = None;
    let mut reconnect_delay = Duration::from_millis(200);
    state.accepting.store(true, Ordering::Relaxed);
    state.redis_connected.store(true, Ordering::Relaxed);

    'intake: loop {
        while let Some(result) = jobs.try_join_next() {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    failure = Some(err);
                    break 'intake;
                }
                Err(err) => {
                    failure = Some(anyhow::anyhow!("evaluation task failed: {err}"));
                    break 'intake;
                }
            }
        }

        let pulled = tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    break 'intake;
                }
                continue;
            }
            result = pull_redis_message(&mut connection) => result,
        };

        let pulled = match pulled {
            Ok(pulled) => {
                state.redis_connected.store(true, Ordering::Relaxed);
                reconnect_delay = Duration::from_millis(200);
                pulled
            }
            Err(err) => {
                state.redis_connected.store(false, Ordering::Relaxed);
                warn!("Redis intake failed: {err}");
                tokio::select! {
                    _ = tokio::time::sleep(reconnect_delay) => {}
                    _ = shutdown.changed() => break 'intake,
                }
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(5));
                continue;
            }
        };

        let raw_message = match pulled {
            PullResult::Message(message) => message,
            PullResult::Empty => continue,
            PullResult::Exit => break,
        };

        let message = match serde_json::from_str::<Message>(&raw_message) {
            Ok(message) => message,
            Err(err) => {
                let hash = hex::encode(crate::util::hash::sha256(raw_message.as_bytes()));
                warn!(
                    payload_bytes = raw_message.len(),
                    payload_sha256 = hash,
                    "discarding malformed message: {err}"
                );
                continue;
            }
        };

        match message {
            Message::System(SystemMessage::Exit) => {
                info!("received system exit, draining worker");
                break;
            }
            Message::BeginEvaluation(meta) => {
                run_evaluation(state.clone(), connection.clone(), meta, &mut jobs).await?;
            }
        }
    }

    state.accepting.store(false, Ordering::Relaxed);
    info!(active_jobs = jobs.len(), "worker draining");
    while let Some(result) = jobs.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) if failure.is_none() => failure = Some(err),
            Err(err) if failure.is_none() => {
                failure = Some(anyhow::anyhow!("evaluation task failed: {err}"))
            }
            _ => {}
        }
    }

    match failure {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

async fn pull_redis_message(connection: &mut ConnectionManager) -> redis::RedisResult<PullResult> {
    if Environment::get().exit_on_empty_queue {
        let in_queue: usize = connection.llen(&Environment::get().redis_queue_key).await?;
        if in_queue == 0 {
            return Ok(PullResult::Exit);
        }
    }

    let value: Option<(String, String)> = connection
        .blpop(&Environment::get().redis_queue_key, 1.0)
        .await?;
    Ok(match value {
        Some((_, message)) => PullResult::Message(message),
        None => PullResult::Empty,
    })
}
