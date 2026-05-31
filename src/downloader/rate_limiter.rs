use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use indicatif::MultiProgress;

// Rate limiting state shared across all downloads
pub struct RateLimitState {
    is_rate_limited: AtomicBool,
    consecutive_errors: AtomicU64,
    last_backoff_time: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    rate_limit_lock: Arc<tokio::sync::Mutex<()>>,
    multi_progress: Arc<tokio::sync::Mutex<Option<MultiProgress>>>,
}

impl RateLimitState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_rate_limited: AtomicBool::new(false),
            consecutive_errors: AtomicU64::new(0),
            last_backoff_time: Arc::new(tokio::sync::Mutex::new(None)),
            rate_limit_lock: Arc::new(tokio::sync::Mutex::new(())),
            multi_progress: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub async fn set_multi_progress(&self, mp: MultiProgress) {
        let mut guard = self.multi_progress.lock().await;
        *guard = Some(mp);
    }

    pub async fn on_error(&self) {
        let errors = self.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;

        // If we hit 3 consecutive errors, trigger rate limit backoff
        if errors >= 3 {
            // Use lock to ensure only one thread prints the message
            let _guard = self.rate_limit_lock.lock().await;

            // Check again after acquiring lock
            if !self.is_rate_limited.swap(true, Ordering::SeqCst) {
                // Suspend multi-progress to stop all updates
                // if let Some(mp) = self.multi_progress.lock().await.as_ref() {
                //     mp.suspend(|| {
                //         println!("\nrate limit detected! pausing all downloads for 5 seconds...");
                //     });
                // } else {
                //     println!("\nrate limit detected! pausing all downloads for 5 seconds...");
                // }
                let mut last_time = self.last_backoff_time.lock().await;
                *last_time = Some(std::time::Instant::now());
            }
        }
    }

    pub async fn on_success(&self) {
        // Only reset if not rate limited
        if !self.is_rate_limited.load(Ordering::SeqCst) {
            self.consecutive_errors.store(0, Ordering::SeqCst);
        }
    }

    pub async fn wait_if_rate_limited(&self) {
        if self.is_rate_limited.load(Ordering::SeqCst) {
            // Use lock to ensure only one thread does the wait and reset
            let _guard = self.rate_limit_lock.lock().await;

            // Check again after acquiring lock
            if self.is_rate_limited.load(Ordering::SeqCst) {
                let mut last_time = self.last_backoff_time.lock().await;

                if let Some(backoff_start) = *last_time {
                    let elapsed = backoff_start.elapsed();
                    let backoff_duration = std::time::Duration::from_secs(5);

                    if elapsed < backoff_duration {
                        let remaining = backoff_duration - elapsed;
                        drop(last_time); // Release lock before sleeping
                        tokio::time::sleep(remaining).await;
                        last_time = self.last_backoff_time.lock().await;
                    }

                    // Reset rate limit state
                    *last_time = None;
                    self.is_rate_limited.store(false, Ordering::SeqCst);
                    self.consecutive_errors.store(0, Ordering::SeqCst);
                    // if let Some(mp) = self.multi_progress.lock().await.as_ref() {
                    //     mp.suspend(|| {
                    //         println!("resuming downloads...");
                    //     });
                    // } else {
                    //     println!("resuming downloads...");
                    // }
                }
            }
        }
    }
}
