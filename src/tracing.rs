use crate::environment::ENVIRONMENT;
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn setup_tracing() {
    let filter = EnvFilter::new(format!(
        "{}={}",
        env!("CARGO_PKG_NAME").replace("-", "_"),
        if cfg!(debug_assertions) || ENVIRONMENT.force_debug_logs {
            Level::DEBUG
        } else {
            Level::INFO
        }
    ));

    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
