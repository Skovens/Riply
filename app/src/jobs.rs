use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::instrument;
use tracing::{error, info};

/// Cooperative cancellation token. The spawned task checks
/// `is_cancelled()` periodically and returns early when true.
#[derive(Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    #[instrument(skip_all)]
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    #[instrument(skip(self))]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Handle for reporting progress from inside a running job.
#[derive(Clone)]
pub struct ProgressHandle {
    total: Arc<AtomicUsize>,
    processed: Arc<AtomicUsize>,
}

impl ProgressHandle {
    fn new() -> Self {
        Self {
            total: Arc::new(AtomicUsize::new(0)),
            processed: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[instrument(skip(self))]
    pub fn set_total(&self, total: usize) {
        self.total.store(total, Ordering::Relaxed);
    }

    #[instrument(skip(self))]
    pub fn increment(&self) {
        self.processed.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> JobProgress {
        JobProgress {
            total: self.total.load(Ordering::Relaxed),
            processed: self.processed.load(Ordering::Relaxed),
        }
    }
}

/// Current status of a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Queued => write!(f, "Queued"),
            JobStatus::Running => write!(f, "Running"),
            JobStatus::Complete => write!(f, "Complete"),
            JobStatus::Failed => write!(f, "Failed"),
            JobStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Describes what kind of work a job is doing.
#[derive(Debug, Clone)]
pub enum JobKind {
    Scan { directory: String },
    Analyze,
    Move { movies_dir: String, tv_dir: String },
    Custom { name: String },
}

impl std::fmt::Display for JobKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobKind::Scan { directory } => write!(f, "Scan ({})", directory),
            JobKind::Analyze => write!(f, "Analyze"),
            JobKind::Move { movies_dir, tv_dir } => {
                write!(f, "Move ({} -> {})", movies_dir, tv_dir)
            }
            JobKind::Custom { name } => write!(f, "{}", name),
        }
    }
}

/// Progress information for a job.
#[derive(Debug, Clone, Default)]
pub struct JobProgress {
    pub total: usize,
    pub processed: usize,
}

/// Public view of a running or completed job.
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: JobProgress,
}

/// Background job scheduler with cancellation and progress tracking.
pub struct JobScheduler {
    jobs: Arc<RwLock<HashMap<String, JobHandle>>>,
    next_id: AtomicUsize,
}

struct JobHandle {
    kind: JobKind,
    status: JobStatus,
    progress_snapshot: JobProgress,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl Default for JobScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl JobScheduler {
    #[instrument(skip_all)]
    pub fn new() -> Self {
        info!("Job scheduler initialized");
        JobScheduler {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicUsize::new(1),
        }
    }

    /// Start a scan job. The closure receives a cancellation token and a
    /// progress handle and runs inside a spawned tokio task.
    #[instrument(skip(self, work))]
    pub async fn start_scan_job<F, Fut>(
        &self,
        directory: &str,
        work: F,
    ) -> Result<String, anyhow::Error>
    where
        F: FnOnce(CancellationToken, ProgressHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let kind = JobKind::Scan {
            directory: directory.to_string(),
        };
        self.start_job_inner(kind, work).await
    }

    /// Start an analyze job.
    #[instrument(skip(self, work))]
    pub async fn start_analyze_job<F, Fut>(&self, work: F) -> Result<String, anyhow::Error>
    where
        F: FnOnce(CancellationToken, ProgressHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.start_job_inner(JobKind::Analyze, work).await
    }

    /// Start a move job.
    #[instrument(skip(self, work))]
    pub async fn start_move_job<F, Fut>(
        &self,
        movies_dir: &str,
        tv_dir: &str,
        work: F,
    ) -> Result<String, anyhow::Error>
    where
        F: FnOnce(CancellationToken, ProgressHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let kind = JobKind::Move {
            movies_dir: movies_dir.to_string(),
            tv_dir: tv_dir.to_string(),
        };
        self.start_job_inner(kind, work).await
    }

    /// Start an arbitrary job.
    #[instrument(skip(self, work))]
    pub async fn start_custom_job<F, Fut>(
        &self,
        name: &str,
        work: F,
    ) -> Result<String, anyhow::Error>
    where
        F: FnOnce(CancellationToken, ProgressHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let kind = JobKind::Custom {
            name: name.to_string(),
        };
        self.start_job_inner(kind, work).await
    }

    /// Cancel a running or queued job. The task is aborted and the
    /// cancellation flag is set so cooperative checks return early.
    #[instrument(skip(self), fields(job_id = %job_id))]
    pub async fn cancel_job(&self, job_id: &str) -> Result<(), anyhow::Error> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        if job.status == JobStatus::Complete
            || job.status == JobStatus::Failed
            || job.status == JobStatus::Cancelled
        {
            return Ok(());
        }

        job.cancel.cancel();
        if let Some(handle) = job.handle.take() {
            handle.abort();
        }
        job.status = JobStatus::Cancelled;
        job.progress_snapshot = JobProgress {
            total: 0,
            processed: 0,
        };

        info!("Cancelled job");
        Ok(())
    }

    /// Get info about a specific job.
    #[instrument(skip(self))]
    pub async fn get_job_status(&self, job_id: &str) -> Result<JobInfo, anyhow::Error> {
        let jobs = self.jobs.read().await;
        let job = jobs
            .get(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        Ok(JobInfo {
            id: job_id.to_string(),
            kind: job.kind.clone(),
            status: job.status.clone(),
            progress: job.progress_snapshot.clone(),
        })
    }

    /// List all jobs.
    #[instrument(skip(self))]
    pub async fn list_jobs(&self) -> Vec<JobInfo> {
        let jobs = self.jobs.read().await;
        jobs.iter()
            .map(|(id, job)| JobInfo {
                id: id.clone(),
                kind: job.kind.clone(),
                status: job.status.clone(),
                progress: job.progress_snapshot.clone(),
            })
            .collect()
    }

    /// Remove completed, failed, or cancelled jobs from the registry.
    #[instrument(skip(self))]
    pub async fn cleanup_finished(&self) -> usize {
        let mut jobs = self.jobs.write().await;
        let before = jobs.len();
        jobs.retain(|_, job| matches!(job.status, JobStatus::Queued | JobStatus::Running));
        before - jobs.len()
    }

    // --- internal ---

    async fn start_job_inner<F, Fut>(&self, kind: JobKind, work: F) -> Result<String, anyhow::Error>
    where
        F: FnOnce(CancellationToken, ProgressHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let id = format!("job_{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let cancel = CancellationToken::new();
        let progress = ProgressHandle::new();

        let cancel_clone = cancel.clone();
        let cancel_for_check = cancel.clone();
        let progress_clone = progress.clone();
        let jobs_arc = self.jobs.clone();
        let id_clone = id.clone();

        // Insert as Queued
        {
            let mut jobs = jobs_arc.write().await;
            jobs.insert(
                id.clone(),
                JobHandle {
                    kind: kind.clone(),
                    status: JobStatus::Queued,
                    progress_snapshot: JobProgress::default(),
                    cancel: cancel.clone(),
                    handle: None,
                },
            );
        }

        let handle = tokio::spawn(async move {
            // Transition to Running
            {
                let mut jobs = jobs_arc.write().await;
                if let Some(job) = jobs.get_mut(&id_clone) {
                    job.status = JobStatus::Running;
                }
            }

            let result = work(cancel_clone, progress_clone.clone()).await;

            // Finalize status
            {
                let mut jobs = jobs_arc.write().await;
                if let Some(job) = jobs.get_mut(&id_clone) {
                    job.progress_snapshot = progress_clone.snapshot();
                    if cancel_for_check.is_cancelled() {
                        job.status = JobStatus::Cancelled;
                    } else {
                        job.status = match result {
                            Ok(()) => JobStatus::Complete,
                            Err(e) => {
                                error!(job_id = %id_clone, error = %e, "Job failed");
                                JobStatus::Failed
                            }
                        };
                    }
                }
            }
        });

        // Store the JoinHandle
        {
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.get_mut(&id) {
                job.handle = Some(handle);
            }
        }

        info!(job_id = %id, kind = %kind, "Started job");
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_display() {
        assert_eq!(format!("{}", JobStatus::Queued), "Queued");
        assert_eq!(format!("{}", JobStatus::Running), "Running");
        assert_eq!(format!("{}", JobStatus::Complete), "Complete");
        assert_eq!(format!("{}", JobStatus::Failed), "Failed");
        assert_eq!(format!("{}", JobStatus::Cancelled), "Cancelled");
    }

    #[test]
    fn test_cancellation_token_default() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_progress_handle() {
        let handle = ProgressHandle::new();
        assert_eq!(handle.snapshot().total, 0);
        assert_eq!(handle.snapshot().processed, 0);

        handle.set_total(10);
        assert_eq!(handle.snapshot().total, 10);

        handle.increment();
        handle.increment();
        assert_eq!(handle.snapshot().processed, 2);
    }

    #[tokio::test]
    async fn test_start_and_complete_job() {
        let scheduler = JobScheduler::new();
        let id = scheduler
            .start_custom_job("test", |_cancel, _progress| async { Ok(()) })
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Give the spawned task time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
    }

    #[tokio::test]
    async fn test_start_job_with_progress() {
        let scheduler = JobScheduler::new();
        let id = scheduler
            .start_custom_job("test", |_cancel, progress| async move {
                progress.set_total(5);
                for _ in 0..5 {
                    progress.increment();
                }
                Ok(())
            })
            .await
            .unwrap();

        // Wait for completion
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
        assert_eq!(info.progress.total, 5);
        assert_eq!(info.progress.processed, 5);
    }

    #[tokio::test]
    async fn test_cancel_running_job() {
        let scheduler = JobScheduler::new();
        let id = scheduler
            .start_custom_job("test", |cancel, _progress| async move {
                loop {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();

        // Wait for it to start running
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        scheduler.cancel_job(&id).await.unwrap();

        let info = scheduler.get_job_status(&id).await.unwrap();
        assert_eq!(info.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_job() {
        let scheduler = JobScheduler::new();
        let result = scheduler.cancel_job("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_nonexistent_job() {
        let scheduler = JobScheduler::new();
        let result = scheduler.get_job_status("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_jobs() {
        let scheduler = JobScheduler::new();

        let id1 = scheduler
            .start_custom_job("job1", |_c, _p| async { Ok(()) })
            .await
            .unwrap();
        let id2 = scheduler
            .start_custom_job("job2", |_c, _p| async { Ok(()) })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().any(|j| j.id == id1));
        assert!(jobs.iter().any(|j| j.id == id2));
    }

    #[tokio::test]
    async fn test_start_scan_job() {
        let scheduler = JobScheduler::new();
        let job_id = scheduler
            .start_scan_job("/test", |_cancel, _progress| async { Ok(()) })
            .await
            .unwrap();
        assert!(!job_id.is_empty());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&job_id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
        assert!(matches!(info.kind, JobKind::Scan { .. }));
    }

    #[tokio::test]
    async fn test_start_analyze_job() {
        let scheduler = JobScheduler::new();
        let job_id = scheduler
            .start_analyze_job(|_cancel, _progress| async { Ok(()) })
            .await
            .unwrap();
        assert!(!job_id.is_empty());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&job_id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
        assert!(matches!(info.kind, JobKind::Analyze));
    }

    #[tokio::test]
    async fn test_start_move_job() {
        let scheduler = JobScheduler::new();
        let job_id = scheduler
            .start_move_job("/movies", "/tv", |_cancel, _progress| async { Ok(()) })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&job_id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
        assert!(matches!(info.kind, JobKind::Move { .. }));
    }

    #[tokio::test]
    async fn test_failed_job() {
        let scheduler = JobScheduler::new();
        let id = scheduler
            .start_custom_job("failing", |_cancel, _progress| async {
                Err(anyhow::anyhow!("something went wrong"))
            })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let info = scheduler.get_job_status(&id).await.unwrap();
        assert_eq!(info.status, JobStatus::Failed);
    }

    #[tokio::test]
    async fn test_cleanup_finished() {
        let scheduler = JobScheduler::new();

        let id1 = scheduler
            .start_custom_job("done", |_c, _p| async { Ok(()) })
            .await
            .unwrap();
        let _id2 = scheduler
            .start_custom_job("stuck", |_c, _p| async {
                // This job never completes
                std::future::pending::<Result<(), anyhow::Error>>().await
            })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let removed = scheduler.cleanup_finished().await;
        assert_eq!(removed, 1);

        // id2 should still be there
        let info = scheduler.get_job_status(&id1).await;
        // After cleanup, the finished job is gone
        assert!(info.is_err());
    }

    #[tokio::test]
    async fn test_cancel_already_complete_job() {
        let scheduler = JobScheduler::new();
        let id = scheduler
            .start_custom_job("fast", |_c, _p| async { Ok(()) })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancelling an already-complete job is a no-op (not an error)
        let result = scheduler.cancel_job(&id).await;
        assert!(result.is_ok());

        let info = scheduler.get_job_status(&id).await.unwrap();
        assert_eq!(info.status, JobStatus::Complete);
    }

    #[tokio::test]
    async fn test_concurrent_jobs() {
        let scheduler = JobScheduler::new();
        let mut ids = Vec::new();

        for i in 0..5 {
            let name = format!("job_{}", i);
            let id = scheduler
                .start_custom_job(&name, |_c, _p| async move {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    Ok(())
                })
                .await
                .unwrap();
            ids.push(id);
        }

        // Wait for all to finish
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        for id in &ids {
            let info = scheduler.get_job_status(id).await.unwrap();
            assert_eq!(info.status, JobStatus::Complete);
        }

        assert_eq!(scheduler.list_jobs().await.len(), 5);
    }
}
