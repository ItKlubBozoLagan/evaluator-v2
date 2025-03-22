use crate::environment::Environment;
use crate::evaluate::queue_handler::handle_evaluation;
use crate::messages::{Message, SystemMessage};
use crate::state::AppState;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use std::sync::Arc;
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
    connection: &mut ConnectionManager,
) -> MessageResult {
    match message {
        Message::System(SystemMessage::Exit) => MessageResult::Exit,
        Message::BeginEvaluation(meta) => handle_evaluation(state, connection, meta).await,
    }
}

pub async fn handle_messages(state: Arc<AppState>, redis_client: Client) {
    let mut msg_connection = redis_client
        .get_connection_manager()
        .await
        .expect("Redis connection manager");

    let mut evaluation_connection = redis_client
        .get_connection_manager()
        .await
        .expect("Redis connection manager");

    'outer: loop {
        let msg = pull_redis_message(&mut msg_connection).await;

        let message = match msg {
            Err(err) => {
                warn!("Error handling message: {err}");
                continue;
            }
            Ok(msg) => msg,
        };

        if let Some(msg) = message {
            let result =
                handle_single_message(state.clone(), msg, &mut evaluation_connection).await;
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
        .blpop(&Environment::get().redis_queue_key, 0.0)
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
