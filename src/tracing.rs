use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn setup_tracing() {
    let filter = EnvFilter::new(format!(
        "kontestis_evaluator_v2={}",
        if cfg!(debug_assertions) {
            Level::DEBUG
        } else {
            Level::INFO
        }
    ));

    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
