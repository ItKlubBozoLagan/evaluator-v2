use crate::messages::Message;
use crate::state::AppState;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::Arc;
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum MessageHandlerError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),
}

pub async fn handle_messages(
    state: Arc<AppState>,
    mut connection: ConnectionManager,
    channel: tokio::sync::broadcast::Sender<Message>,
) {
    loop {
        let msg = do_handle_message(state.clone(), &mut connection).await;

        let message = match msg {
            Err(err) => {
                warn!("Error handling message: {err}");
                continue;
            }
            Ok(msg) => msg,
        };

        if let Some(msg) = message {
            let _ = channel.send(msg);
        }
    }
}

async fn do_handle_message(
    state: Arc<AppState>,
    connection: &mut ConnectionManager,
) -> Result<Option<Message>, MessageHandlerError> {
    let val: Option<(String, String)> = connection.blpop(&state.redis_queue_key, 0.0).await?;

    let Some((_, val)) = val else {
        return Ok(None);
    };

    let message = serde_json::from_str::<Message>(&val);

    let Ok(msg) = message else {
        return Ok(None);
    };

    Ok(Some(msg))
}
