use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error("evaluation needs {requested_kib} KiB but the worker budget is {budget_kib} KiB")]
    OverMemoryBudget { requested_kib: u32, budget_kib: u32 },
    #[error("worker admission is closed")]
    Closed,
}

pub struct Admission {
    pub box_ids: Vec<u8>,
    free_box_ids: Arc<Mutex<Vec<u8>>>,
    _box_permit: OwnedSemaphorePermit,
    _memory_permit: OwnedSemaphorePermit,
}

impl Drop for Admission {
    fn drop(&mut self) {
        self.free_box_ids
            .lock()
            .expect("free box ID mutex poisoned")
            .extend(self.box_ids.drain(..));
    }
}

pub struct AppState {
    boxes: Arc<Semaphore>,
    free_box_ids: Arc<Mutex<Vec<u8>>>,
    memory: Arc<Semaphore>,
    total_boxes: usize,
    total_memory_kib: u32,
    pub accepting: AtomicBool,
    pub redis_connected: AtomicBool,
    pub jobs_started: AtomicU64,
    pub jobs_completed: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub publish_retries: AtomicU64,
    pub dead_lettered: AtomicU64,
}

impl AppState {
    pub fn new(total_boxes: u8, total_memory_kib: u32) -> Arc<Self> {
        Arc::new(Self {
            boxes: Arc::new(Semaphore::new(total_boxes as usize)),
            free_box_ids: Arc::new(Mutex::new((0..total_boxes).rev().collect())),
            memory: Arc::new(Semaphore::new(total_memory_kib as usize)),
            total_boxes: total_boxes as usize,
            total_memory_kib,
            accepting: AtomicBool::new(false),
            redis_connected: AtomicBool::new(false),
            jobs_started: AtomicU64::new(0),
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            publish_retries: AtomicU64::new(0),
            dead_lettered: AtomicU64::new(0),
        })
    }

    pub async fn admit(
        self: &Arc<Self>,
        needed_boxes: u32,
        requested_memory_kib: u32,
    ) -> Result<Admission, AdmissionError> {
        if requested_memory_kib > self.total_memory_kib {
            return Err(AdmissionError::OverMemoryBudget {
                requested_kib: requested_memory_kib,
                budget_kib: self.total_memory_kib,
            });
        }

        let memory_permit = self
            .memory
            .clone()
            .acquire_many_owned(requested_memory_kib)
            .await
            .map_err(|_| AdmissionError::Closed)?;
        let box_permit = self
            .boxes
            .clone()
            .acquire_many_owned(needed_boxes)
            .await
            .map_err(|_| AdmissionError::Closed)?;

        let mut free_box_ids = self
            .free_box_ids
            .lock()
            .expect("free box ID mutex poisoned");
        let mut box_ids = Vec::with_capacity(needed_boxes as usize);
        for _ in 0..needed_boxes {
            box_ids.push(free_box_ids.pop().expect("box permit without a free ID"));
        }

        Ok(Admission {
            box_ids,
            free_box_ids: self.free_box_ids.clone(),
            _box_permit: box_permit,
            _memory_permit: memory_permit,
        })
    }

    pub fn metrics(&self) -> String {
        let available_boxes = self.boxes.available_permits();
        let available_memory = self.memory.available_permits();
        format!(
            concat!(
                "evaluator_accepting_work {}\n",
                "evaluator_redis_connected {}\n",
                "evaluator_isolate_boxes_free {}\n",
                "evaluator_isolate_boxes_used {}\n",
                "evaluator_memory_configured_kib {}\n",
                "evaluator_memory_allocated_kib {}\n",
                "evaluator_jobs_started_total {}\n",
                "evaluator_jobs_completed_total {}\n",
                "evaluator_jobs_failed_total {}\n",
                "evaluator_publish_retries_total {}\n",
                "evaluator_dead_lettered_total {}\n"
            ),
            self.accepting.load(Ordering::Relaxed) as u8,
            self.redis_connected.load(Ordering::Relaxed) as u8,
            available_boxes,
            self.total_boxes - available_boxes,
            self.total_memory_kib,
            self.total_memory_kib as usize - available_memory,
            self.jobs_started.load(Ordering::Relaxed),
            self.jobs_completed.load(Ordering::Relaxed),
            self.jobs_failed.load(Ordering::Relaxed),
            self.publish_retries.load(Ordering::Relaxed),
            self.dead_lettered.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{AdmissionError, AppState};

    #[tokio::test]
    async fn ordinary_jobs_receive_distinct_boxes() {
        let state = AppState::new(2, 4096);
        let first = state.admit(1, 1024).await.unwrap();
        let second = state.admit(1, 1024).await.unwrap();
        assert_ne!(first.box_ids, second.box_ids);
    }

    #[tokio::test]
    async fn dropping_admission_releases_box_and_memory_permits() {
        let state = AppState::new(2, 2048);
        let first_ids = {
            let admission = state.admit(2, 2048).await.unwrap();
            admission.box_ids.clone()
        };
        let second = state.admit(2, 2048).await.unwrap();
        assert_eq!(first_ids.len(), second.box_ids.len());
    }

    #[tokio::test]
    async fn oversized_memory_request_is_rejected() {
        let state = AppState::new(2, 1024);
        assert!(matches!(
            state.admit(1, 1025).await,
            Err(AdmissionError::OverMemoryBudget { .. })
        ));
    }
}
