use crate::environment::Environment;
use crate::state::AppState;
use crate::tracing::setup_tracing;
use ::tracing::{error, info};
use std::sync::atomic::Ordering;
use std::time::Duration;

mod environment;
mod evaluate;
mod health;
mod isolate;
mod messages;
mod redis_client;
mod state;
mod tracing;
mod util;

fn main() -> anyhow::Result<()> {
    setup_tracing();
    if let Err(err) = Environment::init() {
        error!("failed to initialize environment: {err}");
        return Err(err);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run());
    runtime.shutdown_timeout(Duration::from_secs(5));
    result
}

async fn run() -> anyhow::Result<()> {
    let environment = Environment::get();
    let client = redis_client::build_client()?;
    let connection = redis_client::connect(&client).await?;
    let state = AppState::new(environment.max_evaluations, environment.memory_budget_kib);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut health = tokio::spawn(health::serve(
        state.clone(),
        &environment.health_bind,
        shutdown_rx.clone(),
    ));
    let mut worker = tokio::spawn(messages::handler::handle_messages(
        state.clone(),
        connection,
        shutdown_rx,
    ));
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    info!("worker started");
    enum Finished {
        Worker(Result<anyhow::Result<()>, tokio::task::JoinError>),
        Health(Result<anyhow::Result<()>, tokio::task::JoinError>),
        Signal,
    }

    let finished = tokio::select! {
        result = &mut worker => Finished::Worker(result),
        result = &mut health => Finished::Health(result),
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, stopping intake");
            Finished::Signal
        }
        _ = sigterm.recv() => {
            info!("received SIGTERM, stopping intake");
            Finished::Signal
        }
    };

    state.accepting.store(false, Ordering::Relaxed);
    let _ = shutdown_tx.send(true);
    let (worker_result, health_result) = match finished {
        Finished::Worker(result) => (result, health.await),
        Finished::Health(result) => (worker.await, result),
        Finished::Signal => (worker.await, health.await),
    };

    worker_result??;
    health_result??;
    info!("worker stopped after draining active jobs");
    Ok(())
}
