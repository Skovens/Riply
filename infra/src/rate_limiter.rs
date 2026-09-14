use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

use chrono::Datelike;

// ── Token-bucket rate limiter (per-second, for TMDb) ────────────────────────

/// A simple token-bucket rate limiter.
///
/// Maintains a bucket of tokens that refill at a given rate.
/// Calls `acquire()` block until a token is available.
pub struct RateLimiter {
    state: Mutex<State>,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

struct State {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// * `max_tokens` - maximum burst size (bucket capacity)
    /// * `refill_per_second` - how many tokens refill per second
    pub fn new(max_tokens: u32, refill_per_second: f64) -> Self {
        RateLimiter {
            state: Mutex::new(State {
                tokens: max_tokens as f64,
                last_refill: Instant::now(),
            }),
            max_tokens: max_tokens as f64,
            refill_rate: refill_per_second,
        }
    }

    /// Wait until a token is available, then consume it.
    pub async fn acquire(&self) {
        loop {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let elapsed = now.duration_since(state.last_refill).as_secs_f64();
            state.tokens = (state.tokens + elapsed * self.refill_rate).min(self.max_tokens);
            state.last_refill = now;

            if state.tokens >= 1.0 {
                state.tokens -= 1.0;
                return;
            }

            let wait = Duration::from_secs_f64((1.0 - state.tokens) / self.refill_rate);
            drop(state);
            tokio::time::sleep(wait).await;
        }
    }
}

// ── Sliding-window rate limiter (per-minute, for OMDb) ──────────────────────

/// A sliding-window rate limiter that enforces N requests per 60-second window.
///
/// Tracks timestamps of recent requests. If the number of requests in the last
/// 60 seconds exceeds the limit, the caller waits until the oldest request falls
/// outside the window.
pub struct SlidingWindowLimiter {
    state: Mutex<SlidingState>,
    max_requests: u32,
    window_secs: u64,
}

struct SlidingState {
    timestamps: Vec<Instant>,
}

impl SlidingWindowLimiter {
    /// Create a new sliding-window rate limiter.
    ///
    /// * `max_requests` - maximum requests allowed within the window
    /// * `window_secs` - window size in seconds (typically 60)
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        SlidingWindowLimiter {
            state: Mutex::new(SlidingState {
                timestamps: Vec::new(),
            }),
            max_requests,
            window_secs,
        }
    }

    /// Wait until a request is allowed, then record it.
    pub async fn acquire(&self) {
        loop {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let window = Duration::from_secs(self.window_secs);

            // Remove timestamps outside the window
            state.timestamps.retain(|t| now.duration_since(*t) < window);

            if state.timestamps.len() < self.max_requests as usize {
                state.timestamps.push(now);
                return;
            }

            // Must wait until the oldest timestamp expires from the window
            let oldest = state.timestamps[0];
            let time_since_oldest = now.duration_since(oldest);
            let wait = window.checked_sub(time_since_oldest).unwrap_or(Duration::ZERO);
            drop(state);
            if wait > Duration::ZERO {
                tokio::time::sleep(wait).await;
            }
            // Loop back to check again after sleep
        }
    }

    /// Returns the number of requests remaining in the current window.
    pub async fn remaining(&self) -> u32 {
        let state = self.state.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);
        let used = state.timestamps.iter().filter(|t| now.duration_since(**t) < window).count();
        (self.max_requests as usize - used) as u32
    }
}

// ── Monthly quota tracker ───────────────────────────────────────────────────

/// Tracks API request counts per calendar month and enforces a monthly quota.
pub struct MonthlyQuota {
    state: Mutex<QuotaState>,
    limit: u64,
}

struct QuotaState {
    current_month: u32, // YYYYMM
    count: u64,
}

impl MonthlyQuota {
    /// Create a new monthly quota tracker.
    ///
    /// * `limit` - maximum requests allowed per calendar month
    pub fn new(limit: u64) -> Self {
        MonthlyQuota {
            state: Mutex::new(QuotaState {
                current_month: current_month(),
                count: 0,
            }),
            limit,
        }
    }

    /// Try to consume one request. Returns Ok(()) if allowed, Err if quota exceeded.
    pub async fn try_acquire(&self) -> Result<(), QuotaExceeded> {
        let mut state = self.state.lock().await;

        // Reset counter if month changed
        let now = chrono::Utc::now();
        let month = (now.year() as u32) * 100 + now.month();
        if month != state.current_month {
            state.current_month = month;
            state.count = 0;
        }

        if state.count >= self.limit {
            return Err(QuotaExceeded {
                remaining: 0,
                resets_at: next_month_start(),
            });
        }

        state.count += 1;
        Ok(())
    }

    /// Returns the number of requests remaining this month.
    pub async fn remaining(&self) -> u64 {
        let state = self.state.lock().await;
        let now = chrono::Utc::now();
        let month = (now.year() as u32) * 100 + now.month();
        if month != state.current_month {
            return self.limit;
        }
        (self.limit as i64 - state.count as i64).max(0) as u64
    }
}

#[derive(Debug, Clone)]
pub struct QuotaExceeded {
    pub remaining: u64,
    pub resets_at: String,
}

impl std::fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OMDb monthly quota exceeded. 0 remaining, resets at {}",
            self.resets_at
        )
    }
}

impl std::error::Error for QuotaExceeded {}

/// Returns the current month as YYYYMM.
fn current_month() -> u32 {
    let now = chrono::Utc::now();
    (now.year() as u32) * 100 + now.month()
}

/// Returns the first day of next month as an RFC3339 string.
fn next_month_start() -> String {
    let now = chrono::Utc::now();
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let next = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    next.and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_immediate_acquire() {
        let limiter = RateLimiter::new(5, 10.0);
        limiter.acquire().await; // should not hang
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let limiter = RateLimiter::new(2, 1000.0); // 2 burst, fast refill
        let start = Instant::now();
        limiter.acquire().await;
        limiter.acquire().await;
        // Third acquire should take ~1ms since refill is 1000/s
        limiter.acquire().await;
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_slow_refill() {
        let limiter = RateLimiter::new(1, 100.0); // 1 token, 100/s
        limiter.acquire().await; // consume initial token
        let start = Instant::now();
        limiter.acquire().await; // must wait ~10ms
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(8));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_sliding_window_allows_burst() {
        let limiter = SlidingWindowLimiter::new(3, 60);
        // First 3 should go through immediately
        limiter.acquire().await;
        limiter.acquire().await;
        limiter.acquire().await;
    }

    #[tokio::test]
    async fn test_sliding_window_enforces_limit() {
        let limiter = SlidingWindowLimiter::new(2, 1); // 2 per second
        limiter.acquire().await;
        limiter.acquire().await;
        // Third should wait ~1 second
        let start = Instant::now();
        limiter.acquire().await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(900), "Should wait ~1s, waited {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_sliding_window_remaining() {
        let limiter = SlidingWindowLimiter::new(5, 60);
        assert_eq!(limiter.remaining().await, 5);
        limiter.acquire().await;
        assert_eq!(limiter.remaining().await, 4);
        limiter.acquire().await;
        assert_eq!(limiter.remaining().await, 3);
    }

    #[tokio::test]
    async fn test_monthly_quota_allows_within_limit() {
        let quota = MonthlyQuota::new(100);
        for _ in 0..99 {
            quota.try_acquire().await.unwrap();
        }
        quota.try_acquire().await.unwrap();
    }

    #[tokio::test]
    async fn test_monthly_quota_exceeds_limit() {
        let quota = MonthlyQuota::new(5);
        for _ in 0..5 {
            quota.try_acquire().await.unwrap();
        }
        let result = quota.try_acquire().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_monthly_quota_remaining() {
        let quota = MonthlyQuota::new(100);
        assert_eq!(quota.remaining().await, 100);
        quota.try_acquire().await.unwrap();
        assert_eq!(quota.remaining().await, 99);
    }
}
