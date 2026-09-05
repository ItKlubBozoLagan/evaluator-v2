use crate::environment::Environment;
use crate::evaluate::queue_handler::handle_evaluation;
use crate::messages::{Message, SystemMessage};
use crate::state::AppState;
use redis::AsyncCommands;
use redis::Client;
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, thiserror::Error)]
pub enum MessageHandlerError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),
}

pub enum MessageResult {
    Continue,
    Exit,
}

async fn handle_single_message(
    state: Arc<AppState>,
    message: Message,
    redis_client: &Client,
) -> MessageResult {
    match message {
        Message::System(SystemMessage::Exit) => MessageResult::Exit,
        Message::BeginEvaluation(meta) => handle_evaluation(state, redis_client, meta).await,
    }
}

pub async fn handle_messages(
    state: Arc<AppState>,
    redis_client: Client,
    mut redis_connection: ConnectionManager,
) {
    let mut reconnect_delay = Duration::from_millis(200);

    'outer: loop {
        let msg = pull_redis_message(&mut redis_connection).await;

        let message = match msg {
            Err(err) => {
                warn!("Redis intake failed: {err}");
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(5));
                continue;
            }
            Ok(msg) => {
                reconnect_delay = Duration::from_millis(200);
                msg
            }
        };

        if let Some(msg) = message {
            let result = handle_single_message(state.clone(), msg, &redis_client).await;
            match result {
                MessageResult::Continue => {}
                MessageResult::Exit => {
                    info!("Received system exit, stopping evaluation handler");
                    break 'outer;
                }
            }
        }
    }
}

async fn pull_redis_message(
    connection: &mut ConnectionManager,
) -> Result<Option<Message>, MessageHandlerError> {
    if Environment::get().exit_on_empty_queue {
        let in_queue: usize = connection.llen(&Environment::get().redis_queue_key).await?;

        if in_queue == 0 {
            info!("Work queue empty, broadcasting exit");
            return Ok(Some(Message::System(SystemMessage::Exit)));
        }
    }

    let val: Option<(String, String)> = connection
        .blpop(&Environment::get().redis_queue_key, 1.0)
        .await?;

    let Some((_, val)) = val else {
        return Ok(None);
    };

    let message = serde_json::from_str::<Message>(&val);

    let Ok(msg) = message else {
        return Ok(None);
    };

    Ok(Some(msg))
}
