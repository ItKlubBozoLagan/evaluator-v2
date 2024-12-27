use std::collections::HashSet;
use tokio::sync::Mutex;

pub struct AppState {
    pub used_box_ids: Mutex<HashSet<u8>>,
}
