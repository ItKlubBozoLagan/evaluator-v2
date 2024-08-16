pub struct AppState {
    pub redis_queue_key: String,
    pub max_evaluations: u8,
    pub evaluation_wg: waitgroup::WaitGroup
}