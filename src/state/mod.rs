use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct Admission {
    pub box_ids: Vec<u8>,
    free_box_ids: Arc<Mutex<Vec<u8>>>,
    _box_permit: OwnedSemaphorePermit,
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
    total_boxes: usize,
    pub accepting: AtomicBool,
    pub redis_connected: AtomicBool,
    pub jobs_started: AtomicU64,
    pub jobs_completed: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub publish_retries: AtomicU64,
}

impl AppState {
    pub fn new(total_boxes: u8) -> Arc<Self> {
        Arc::new(Self {
            boxes: Arc::new(Semaphore::new(total_boxes as usize)),
            free_box_ids: Arc::new(Mutex::new((0..total_boxes).rev().collect())),
            total_boxes: total_boxes as usize,
            accepting: AtomicBool::new(false),
            redis_connected: AtomicBool::new(false),
            jobs_started: AtomicU64::new(0),
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            publish_retries: AtomicU64::new(0),
        })
    }

    pub async fn admit(self: &Arc<Self>, needed_boxes: u32) -> anyhow::Result<Admission> {
        let box_permit = self
            .boxes
            .clone()
            .acquire_many_owned(needed_boxes)
            .await
            .map_err(|_| anyhow::anyhow!("worker admission is closed"))?;

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
        })
    }

    pub fn metrics(&self) -> String {
        let available_boxes = self.boxes.available_permits();
        format!(
            concat!(
                "evaluator_accepting_work {}\n",
                "evaluator_redis_connected {}\n",
                "evaluator_isolate_boxes_free {}\n",
                "evaluator_isolate_boxes_used {}\n",
                "evaluator_jobs_started_total {}\n",
                "evaluator_jobs_completed_total {}\n",
                "evaluator_jobs_failed_total {}\n",
                "evaluator_publish_retries_total {}\n"
            ),
            self.accepting.load(Ordering::Relaxed) as u8,
            self.redis_connected.load(Ordering::Relaxed) as u8,
            available_boxes,
            self.total_boxes - available_boxes,
            self.jobs_started.load(Ordering::Relaxed),
            self.jobs_completed.load(Ordering::Relaxed),
            self.jobs_failed.load(Ordering::Relaxed),
            self.publish_retries.load(Ordering::Relaxed),
        )
    }
}
