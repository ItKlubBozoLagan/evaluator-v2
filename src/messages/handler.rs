use crate::environment::ENVIRONMENT;
use crate::messages::{Message, SystemMessage};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tracing::{info, warn};

#[derive(Debug, thiserror::Error)]
pub enum MessageHandlerError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),
}

pub async fn handle_messages(
    mut connection: ConnectionManager,
    channel: tokio::sync::broadcast::Sender<Message>,
) {
    loop {
        let msg = do_handle_message(&mut connection).await;

        let message = match msg {
            Err(err) => {
                warn!("Error handling message: {err}");
                continue;
            }
            Ok(msg) => msg,
        };

        if let Some(msg) = message {
            let is_exit = matches!(msg, Message::System(SystemMessage::Exit));

            let _ = channel.send(msg);

            if is_exit {
                info!("Received system exit, stopping message handler");
                break;
            }
        }
    }
}

async fn do_handle_message(
    connection: &mut ConnectionManager,
) -> Result<Option<Message>, MessageHandlerError> {
    if ENVIRONMENT.exit_on_empty_queue {
        let in_queue: usize = connection.llen(&ENVIRONMENT.redis_queue_key).await?;

        if in_queue == 0 {
            info!("Work queue empty, broadcasting exit");
            return Ok(Some(Message::System(SystemMessage::Exit)));
        }
    }

    let val: Option<(String, String)> = connection.blpop(&ENVIRONMENT.redis_queue_key, 0.0).await?;

    let Some((_, val)) = val else {
        return Ok(None);
    };

    let message = serde_json::from_str::<Message>(&val);

    let Ok(msg) = message else {
        return Ok(None);
    };

    Ok(Some(msg))
}
