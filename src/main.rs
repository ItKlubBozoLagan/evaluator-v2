use crate::messages::Message;
use crate::state::AppState;
use crate::tracing::setup_tracing;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use ::tracing::info;

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
        max_evaluations: std::cmp::max(
            env::var("EVALUATOR_MAX_EVALUATIONS")
                .unwrap_or("2".to_string())
                .parse::<u8>()
                .expect("EVALUATOR_MAX_EVALUATIONS must be a number"),
            2,
        ),
        used_box_ids: Mutex::from(HashSet::new()),
    });

    let redis_url = env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string());

    let client = redis::Client::open(redis_url)?;

    let (tx, _) = tokio::sync::broadcast::channel::<Message>(16);

    let evaluation_handler = tokio::spawn(evaluate::queue_handler::handle(
        state.clone(),
        tx.subscribe(),
        client.get_connection_manager().await?,
    ));

    info!("Started");

    info!("Using max evaluations: {}", state.max_evaluations);

    messages::handler::handle_messages(state.clone(), client.get_connection_manager().await?, tx)
        .await;

    let _ = evaluation_handler.await;

    Ok(())
}
