use std::collections::HashSet;
use tokio::sync::{Mutex, Notify};

pub struct AppState {
    pub used_box_ids: Mutex<HashSet<u8>>,
    pub available_boxes_notify: Notify,
}
