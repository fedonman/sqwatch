use std::collections::{HashMap, VecDeque};
use std::thread;

use crossbeam::channel::{Receiver, Sender, unbounded};

use crate::backend::commands::{JobDetail, scontrol_show_job};

/// Runs `scontrol show job` in a background thread and caches results.
///
/// Only one lookup is in-flight at a time. Rapid requests are deduplicated
/// by draining the channel and keeping only the latest job ID.
/// Maximum number of job details retained in the LRU cache.
const CACHE_CAP: usize = 64;

pub struct JobDetailResolver {
    request_tx: Sender<String>,
    result_rx: Receiver<(String, Option<JobDetail>)>,
    pending: Option<String>,
    cache: HashMap<String, JobDetail>,
    /// Job IDs in least-recently-used order (front = LRU, back = MRU).
    order: VecDeque<String>,
}

impl Default for JobDetailResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl JobDetailResolver {
    pub fn new() -> Self {
        let (req_tx, req_rx) = unbounded::<String>();
        let (res_tx, res_rx) = unbounded::<(String, Option<JobDetail>)>();

        thread::spawn(move || {
            while let Ok(mut job_id) = req_rx.recv() {
                // Drain queued requests, keep only the latest
                while let Ok(newer_id) = req_rx.try_recv() {
                    job_id = newer_id;
                }
                let result = scontrol_show_job(&job_id);
                let _ = res_tx.send((job_id, result));
            }
        });

        Self {
            request_tx: req_tx,
            result_rx: res_rx,
            pending: None,
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Request a job detail lookup. No-op if already cached or in-flight.
    pub fn request(&mut self, job_id: &str) {
        if self.cache.contains_key(job_id) {
            return;
        }
        if self.pending.as_deref() == Some(job_id) {
            return;
        }
        self.pending = Some(job_id.to_string());
        let _ = self.request_tx.send(job_id.to_string());
    }

    /// Poll for resolved results and update the cache.
    pub fn poll(&mut self) {
        while let Ok((job_id, detail)) = self.result_rx.try_recv() {
            if self.pending.as_deref() == Some(&job_id) {
                self.pending = None;
            }
            if let Some(d) = detail {
                self.cache_put(job_id, d);
            }
        }
    }

    /// Get a cached detail, marking it most-recently-used.
    pub fn get_cached(&mut self, job_id: &str) -> Option<&JobDetail> {
        if self.cache.contains_key(job_id) {
            self.touch(job_id);
            self.cache.get(job_id)
        } else {
            None
        }
    }

    /// Insert or refresh a cache entry, evicting the least-recently-used
    /// entry when the cache is full.
    fn cache_put(&mut self, job_id: String, detail: JobDetail) {
        if self.cache.contains_key(&job_id) {
            self.cache.insert(job_id.clone(), detail);
            self.touch(&job_id);
            return;
        }
        if self.cache.len() >= CACHE_CAP
            && let Some(evicted) = self.order.pop_front()
        {
            self.cache.remove(&evicted);
        }
        self.order.push_back(job_id.clone());
        self.cache.insert(job_id, detail);
    }

    /// Move `job_id` to the most-recently-used end of the order queue.
    fn touch(&mut self, job_id: &str) {
        if let Some(pos) = self.order.iter().position(|k| k == job_id)
            && let Some(k) = self.order.remove(pos)
        {
            self.order.push_back(k);
        }
    }
}
