use crate::messages::Message;
use crate::state::AppState;
use crate::tracing::setup_tracing;
use ::tracing::info;
use std::sync::Arc;

mod messages;
mod state;
mod tracing;

mod evaluate;
mod isolate;
mod util;

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
        // evaluation_wg: WaitGroup::new(),
        // max_evaluations: 1
    });

    let client = redis::Client::open("redis://localhost:6379")?;

    let (tx, _) = tokio::sync::broadcast::channel::<Message>(16);

    let evaluation_handler = tokio::spawn(evaluate::queue_handler::handle(
        state.clone(),
        tx.subscribe(),
        client.get_connection_manager().await?,
    ));

    info!("Started");

    messages::handler::handle_messages(state.clone(), client.get_connection_manager().await?, tx)
        .await;

    let _ = evaluation_handler.await;

    Ok(())
}
