use crate::environment::ENVIRONMENT;
use crate::messages::Message;
use crate::state::AppState;
use crate::tracing::setup_tracing;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use ::tracing::info;

mod messages;
mod state;
mod tracing;

mod environment;
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
        used_box_ids: Mutex::from(HashSet::new()),
    });

    let client = redis::Client::open(&*ENVIRONMENT.redis_url)?;

    let (tx, _) = tokio::sync::broadcast::channel::<Message>(16);

    let evaluation_handler = tokio::spawn(evaluate::queue_handler::handle(
        state.clone(),
        tx.subscribe(),
        client.get_connection_manager().await?,
    ));

    info!("Started");

    info!("Using max evaluations: {}", ENVIRONMENT.max_evaluations);

    messages::handler::handle_messages(client.get_connection_manager().await?, tx).await;

    let _ = evaluation_handler.await;

    Ok(())
}
