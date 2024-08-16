use crate::messages::{Evaluation, Message};
use crate::state::AppState;
use crate::tracing::setup_tracing;
use std::sync::Arc;
use ::tracing::{debug, info};
use waitgroup::WaitGroup;

mod messages;
mod redis;
mod state;
mod tracing;

fn main() -> anyhow::Result<()> {
    setup_tracing();

    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;

    rt.block_on(entrypoint())?;

    rt.shutdown_background();

    Ok(())
}

async fn entrypoint() -> anyhow::Result<()> {
    info!("Starting...");

    let state = Arc::new(AppState {
        redis_queue_key: "evaluator_msg_queue".to_string(),
        evaluation_wg: WaitGroup::new(),
        max_evaluations: 1
    });

    let mut connection = redis::get_connection("redis://localhost:6379").await?;

    let (tx, mut rx) = tokio::sync::broadcast::channel::<Message>(16);

    let a = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            debug!("{msg:?}");
        }
    });

    messages::handler::handle_messages(state.clone(), &mut connection, tx).await;

    let _ = a.await;

    Ok(())
}
