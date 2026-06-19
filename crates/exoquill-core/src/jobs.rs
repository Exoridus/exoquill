//! Asynchronous job queue (product spec §17.3).
//!
//! A single worker thread runs jobs off the UI thread, so only one heavy job
//! runs at a time and the UI never blocks. Jobs are cancellable and report
//! progress through the [`EventSink`]. Provider runs (`exoquill-ai`) are wrapped
//! as job tasks by the platform layer.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Serialize;
use uuid::Uuid;

use crate::cancel::CancelToken;
use crate::clock::now_rfc3339;
use crate::events::{Event, EventSink};

pub type JobId = String;

/// The work a job performs. Runs on the worker thread; it should check
/// [`JobHandle::is_cancelled`] periodically and report progress.
pub type JobTask = Box<dyn FnOnce(&JobHandle) -> Result<(), String> + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// An observable record of a job. Mirrors the `jobs` table (product spec §16).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    pub job_type: String,
    pub status: JobStatus,
    pub note_id: Option<String>,
    pub progress: f32,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

/// Handle passed into a running task for progress + cancellation.
pub struct JobHandle {
    id: JobId,
    cancel: CancelToken,
    sink: Arc<dyn EventSink>,
}

impl JobHandle {
    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The cancellation token, e.g. to forward to a provider run.
    pub fn cancel_token(&self) -> &CancelToken {
        &self.cancel
    }

    /// Report progress in `[0.0, 1.0]`.
    pub fn report_progress(&self, progress: f32) {
        self.sink.emit(Event::JobProgress {
            id: self.id.clone(),
            progress: progress.clamp(0.0, 1.0),
        });
    }
}

struct QueuedJob {
    job: Job,
    cancel: CancelToken,
    task: JobTask,
}

/// A queue that runs jobs on a single background worker thread.
pub struct JobQueue {
    sender: Sender<QueuedJob>,
    cancels: Arc<Mutex<HashMap<JobId, CancelToken>>>,
    jobs: Arc<Mutex<HashMap<JobId, Job>>>,
}

impl JobQueue {
    /// Create a queue and spawn its worker thread.
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        let (sender, receiver) = mpsc::channel::<QueuedJob>();
        let cancels: Arc<Mutex<HashMap<JobId, CancelToken>>> = Arc::default();
        let jobs: Arc<Mutex<HashMap<JobId, Job>>> = Arc::default();

        let worker_sink = Arc::clone(&sink);
        let worker_cancels = Arc::clone(&cancels);
        let worker_jobs = Arc::clone(&jobs);
        thread::Builder::new()
            .name("exoquill-jobs".into())
            .spawn(move || worker_loop(receiver, worker_sink, worker_cancels, worker_jobs))
            .expect("spawn job worker thread");

        Self {
            sender,
            cancels,
            jobs,
        }
    }

    /// Enqueue a job and return its id immediately. The task runs on the worker.
    pub fn enqueue(
        &self,
        job_type: impl Into<String>,
        note_id: Option<String>,
        task: JobTask,
    ) -> JobId {
        let id = Uuid::new_v4().to_string();
        let cancel = CancelToken::new();
        let job = Job {
            id: id.clone(),
            job_type: job_type.into(),
            status: JobStatus::Queued,
            note_id,
            progress: 0.0,
            error: None,
            created_at: now_rfc3339(),
            started_at: None,
            finished_at: None,
        };
        self.cancels
            .lock()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        self.jobs.lock().unwrap().insert(id.clone(), job.clone());
        // The worker outlives the queue's lifetime concerns here; ignore send
        // errors (only possible if the worker thread is gone).
        let _ = self.sender.send(QueuedJob { job, cancel, task });
        id
    }

    /// Request cancellation of a job (whether queued or running).
    pub fn cancel(&self, id: &str) {
        if let Some(token) = self.cancels.lock().unwrap().get(id) {
            token.cancel();
        }
    }

    /// Snapshot of a single job's current record.
    pub fn job(&self, id: &str) -> Option<Job> {
        self.jobs.lock().unwrap().get(id).cloned()
    }

    /// Snapshot of all known job records.
    pub fn jobs(&self) -> Vec<Job> {
        self.jobs.lock().unwrap().values().cloned().collect()
    }
}

fn worker_loop(
    receiver: Receiver<QueuedJob>,
    sink: Arc<dyn EventSink>,
    cancels: Arc<Mutex<HashMap<JobId, CancelToken>>>,
    jobs: Arc<Mutex<HashMap<JobId, Job>>>,
) {
    while let Ok(QueuedJob {
        mut job,
        cancel,
        task,
    }) = receiver.recv()
    {
        // Cancelled while still queued: skip execution entirely.
        if cancel.is_cancelled() {
            finalize(&sink, &cancels, &jobs, &mut job, JobStatus::Cancelled, None);
            continue;
        }

        job.status = JobStatus::Running;
        job.started_at = Some(now_rfc3339());
        publish(&sink, &jobs, &job);

        let handle = JobHandle {
            id: job.id.clone(),
            cancel: cancel.clone(),
            sink: Arc::clone(&sink),
        };
        let result = task(&handle);

        let (status, error) = if cancel.is_cancelled() {
            (JobStatus::Cancelled, None)
        } else {
            match result {
                Ok(()) => {
                    job.progress = 1.0;
                    (JobStatus::Completed, None)
                }
                Err(message) => (JobStatus::Failed, Some(message)),
            }
        };
        finalize(&sink, &cancels, &jobs, &mut job, status, error);
    }
}

fn publish(sink: &Arc<dyn EventSink>, jobs: &Arc<Mutex<HashMap<JobId, Job>>>, job: &Job) {
    jobs.lock().unwrap().insert(job.id.clone(), job.clone());
    sink.emit(Event::JobUpdated { job: job.clone() });
}

fn finalize(
    sink: &Arc<dyn EventSink>,
    cancels: &Arc<Mutex<HashMap<JobId, CancelToken>>>,
    jobs: &Arc<Mutex<HashMap<JobId, Job>>>,
    job: &mut Job,
    status: JobStatus,
    error: Option<String>,
) {
    job.status = status;
    job.error = error;
    job.finished_at = Some(now_rfc3339());
    cancels.lock().unwrap().remove(&job.id);
    publish(sink, jobs, job);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Test sink that forwards every event over a channel.
    struct ChannelSink(Mutex<Sender<Event>>);
    impl EventSink for ChannelSink {
        fn emit(&self, event: Event) {
            let _ = self.0.lock().unwrap().send(event);
        }
    }

    fn queue_with_events() -> (JobQueue, Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let queue = JobQueue::new(Arc::new(ChannelSink(Mutex::new(tx))));
        (queue, rx)
    }

    /// Wait for the terminal (Completed/Failed/Cancelled) JobUpdated of `id`.
    fn wait_terminal(rx: &Receiver<Event>, id: &str) -> Job {
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).expect("event") {
                Event::JobUpdated { job } if job.id == id && job.status != JobStatus::Running => {
                    return job;
                }
                _ => {}
            }
        }
    }

    #[test]
    fn job_runs_to_completion_and_reports_progress() {
        let (queue, rx) = queue_with_events();
        let id = queue.enqueue(
            "test",
            Some("note-1".into()),
            Box::new(|h| {
                h.report_progress(0.5);
                Ok(())
            }),
        );

        let mut saw_progress = false;
        let terminal = loop {
            match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                Event::JobProgress { id: pid, .. } if pid == id => saw_progress = true,
                Event::JobUpdated { job } if job.id == id && job.status != JobStatus::Running => {
                    break job;
                }
                _ => {}
            }
        };
        assert_eq!(terminal.status, JobStatus::Completed);
        assert_eq!(terminal.progress, 1.0);
        assert_eq!(terminal.note_id.as_deref(), Some("note-1"));
        assert!(saw_progress);
    }

    #[test]
    fn failing_job_reports_error() {
        let (queue, rx) = queue_with_events();
        let id = queue.enqueue("test", None, Box::new(|_| Err("boom".into())));
        let job = wait_terminal(&rx, &id);
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.as_deref(), Some("boom"));
    }

    #[test]
    fn cancel_while_queued_skips_execution() {
        let (queue, rx) = queue_with_events();

        // Occupy the worker with a blocker until we release it.
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let gate = Mutex::new(gate_rx);
        queue.enqueue(
            "blocker",
            None,
            Box::new(move |_| {
                gate.lock().unwrap().recv().ok();
                Ok(())
            }),
        );

        // Second job is cancelled before the worker can reach it.
        let ran = Arc::new(AtomicBool::new(false));
        let ran_in_task = Arc::clone(&ran);
        let victim = queue.enqueue(
            "victim",
            None,
            Box::new(move |_| {
                ran_in_task.store(true, Ordering::SeqCst);
                Ok(())
            }),
        );
        queue.cancel(&victim);

        gate_tx.send(()).unwrap(); // release the blocker

        let job = wait_terminal(&rx, &victim);
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(
            !ran.load(Ordering::SeqCst),
            "cancelled job must not execute"
        );
    }
}
