use std::collections::HashSet;
use tokio::sync::Mutex;

pub struct AppState {
    pub redis_queue_key: String,
    pub max_evaluations: u8,
    pub used_box_ids: Mutex<HashSet<u8>>,
}
