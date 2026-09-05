use crate::environment::Environment;
use crate::state::AppState;
use crate::tracing::setup_tracing;
use ::tracing::{error, info};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

mod messages;
mod state;
mod tracing;

mod environment;
mod evaluate;
mod isolate;
mod redis_client;
mod util;

fn main() -> anyhow::Result<()> {
    if let Err(err) = Environment::init() {
        error!("Error initializing environment: {err}");
        return Err(anyhow::anyhow!("Error initializing environment"));
    }

    setup_tracing();

    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;

    rt.block_on(start())?;

    rt.shutdown_background();

    Ok(())
}

async fn start() -> anyhow::Result<()> {
    // TODO: doesn't wait for current evaluation to finish

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    tokio::select! {
        _ = entrypoint() => {},
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down");
        },
        _ = sigterm.recv() => {
            info!("Shutting down");
        }
    }

    Ok(())
}

async fn entrypoint() -> anyhow::Result<()> {
    info!("Starting...");

    let state = Arc::new(AppState {
        used_box_ids: Mutex::from(HashSet::new()),
        available_boxes_notify: Notify::new(),
    });

    let client = redis_client::build_client()?;
    let connection = redis_client::connect(&client).await?;

    info!("Started");

    info!(
        "Using max evaluations: {}",
        Environment::get().max_evaluations
    );

    messages::handler::handle_messages(state, connection).await;

    Ok(())
}
