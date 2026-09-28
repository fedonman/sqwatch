use std::collections::{HashMap, VecDeque};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, Sender, unbounded};

use crate::backend::commands::{JobDetail, scontrol_show_job};

/// Runs `scontrol show job` in a background thread and caches results.
///
/// Only one lookup is in-flight at a time. Rapid requests are deduplicated
/// by draining the channel and keeping only the latest job ID.
/// Maximum number of job details retained in the LRU cache.
const CACHE_CAP: usize = 64;

/// How long a failed lookup stands before the job is looked up again.
const RETRY_AFTER: Duration = Duration::from_secs(10);

/// A finished lookup. Failures are cached too, so a job `scontrol` no longer
/// knows is not looked up again on every frame.
enum Lookup {
    Found(JobDetail),
    Failed(Instant),
}

pub struct JobDetailResolver {
    request_tx: Sender<String>,
    result_rx: Receiver<(String, Option<JobDetail>)>,
    pending: Option<String>,
    cache: HashMap<String, Lookup>,
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
        Self::with_lookup(scontrol_show_job)
    }

    /// A resolver that runs `lookup` in place of `scontrol show job`.
    fn with_lookup(lookup: impl Fn(&str) -> Option<JobDetail> + Send + 'static) -> Self {
        let (req_tx, req_rx) = unbounded::<String>();
        let (res_tx, res_rx) = unbounded::<(String, Option<JobDetail>)>();

        thread::spawn(move || {
            while let Ok(mut job_id) = req_rx.recv() {
                // Drain queued requests, keep only the latest
                while let Ok(newer_id) = req_rx.try_recv() {
                    job_id = newer_id;
                }
                let result = lookup(&job_id);
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

    /// Request a job detail lookup. No-op if already cached or in-flight, or
    /// if the last lookup failed less than `RETRY_AFTER` ago.
    pub fn request(&mut self, job_id: &str) {
        match self.cache.get(job_id) {
            Some(Lookup::Found(_)) => return,
            Some(Lookup::Failed(at)) if at.elapsed() < RETRY_AFTER => return,
            _ => {}
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
            let lookup = detail.map_or_else(|| Lookup::Failed(Instant::now()), Lookup::Found);
            self.cache_put(job_id, lookup);
        }
    }

    /// Get a cached detail, marking it most-recently-used.
    pub fn get_cached(&mut self, job_id: &str) -> Option<&JobDetail> {
        if self.cache.contains_key(job_id) {
            self.touch(job_id);
            match self.cache.get(job_id) {
                Some(Lookup::Found(detail)) => Some(detail),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Insert or refresh a cache entry, evicting the least-recently-used
    /// entry when the cache is full.
    fn cache_put(&mut self, job_id: String, lookup: Lookup) {
        if self.cache.contains_key(&job_id) {
            self.cache.insert(job_id.clone(), lookup);
            self.touch(&job_id);
            return;
        }
        if self.cache.len() >= CACHE_CAP
            && let Some(evicted) = self.order.pop_front()
        {
            self.cache.remove(&evicted);
        }
        self.order.push_back(job_id.clone());
        self.cache.insert(job_id, lookup);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// A resolver whose lookups return `result`, and a count of the lookups.
    fn resolver(result: Option<JobDetail>) -> (JobDetailResolver, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let resolver = JobDetailResolver::with_lookup(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            result.clone()
        });
        (resolver, calls)
    }

    /// Request `job_id` and wait for any lookup it started to finish.
    fn request_and_wait(resolver: &mut JobDetailResolver, job_id: &str) {
        resolver.request(job_id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while resolver.pending.is_some() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
            resolver.poll();
        }
        assert!(
            resolver.pending.is_none(),
            "lookup for {} never finished",
            job_id
        );
    }

    fn detail() -> JobDetail {
        JobDetail {
            stdout_file: Some("/work/slurm-1.out".into()),
            stderr_file: None,
            command: None,
            work_dir: None,
        }
    }

    #[test]
    fn a_found_detail_is_looked_up_once() {
        let (mut resolver, calls) = resolver(Some(detail()));
        for _ in 0..20 {
            request_and_wait(&mut resolver, "1");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(resolver.get_cached("1").is_some());
    }

    #[test]
    fn a_failed_lookup_is_not_repeated_on_every_request() {
        let (mut resolver, calls) = resolver(None);
        for _ in 0..20 {
            request_and_wait(&mut resolver, "1");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(resolver.get_cached("1").is_none());
    }

    #[test]
    fn a_failed_lookup_is_tried_again_after_a_while() {
        let (mut resolver, calls) = resolver(None);
        request_and_wait(&mut resolver, "1");

        let long_ago = Instant::now().checked_sub(RETRY_AFTER).unwrap();
        resolver.cache.insert("1".into(), Lookup::Failed(long_ago));
        request_and_wait(&mut resolver, "1");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
