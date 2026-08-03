use std::thread;

use crossbeam::channel::{Receiver, Sender, TryRecvError, unbounded};

use crate::backend::Job;
use crate::backend::query::{QueryParams, fetch_jobs};

/// Runs `squeue` in a background thread so the UI never blocks on job refreshes.
///
/// Rapid submissions are deduplicated: the worker drains the request channel
/// and only executes the most recent `QueryParams`.
pub struct JobFetcher {
    request_tx: Sender<QueryParams>,
    result_rx: Receiver<Result<Vec<Job>, String>>,
    pub in_flight: bool,
}

impl Default for JobFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl JobFetcher {
    pub fn new() -> Self {
        let (req_tx, req_rx) = unbounded::<QueryParams>();
        let (res_tx, res_rx) = unbounded::<Result<Vec<Job>, String>>();

        thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = res_tx.send(Err(format!("job fetch worker failed to start: {}", e)));
                    return;
                }
            };

            while let Ok(mut params) = req_rx.recv() {
                // Drain queued requests, keep only the latest
                while let Ok(newer) = req_rx.try_recv() {
                    params = newer;
                }
                let result = rt.block_on(fetch_jobs(&params)).map_err(|e| e.to_string());
                let _ = res_tx.send(result);
            }
        });

        Self {
            request_tx: req_tx,
            result_rx: res_rx,
            in_flight: false,
        }
    }

    /// Submit a job list fetch. Non-blocking.
    pub fn submit(&mut self, params: QueryParams) {
        let _ = self.request_tx.send(params);
        self.in_flight = true;
    }

    /// Poll for a completed result. Returns `None` if nothing is ready.
    pub fn poll(&mut self) -> Option<Result<Vec<Job>, String>> {
        match self.result_rx.try_recv() {
            Ok(result) => {
                self.in_flight = false;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.in_flight = false;
                Some(Err("job fetch worker stopped".to_string()))
            }
        }
    }
}
