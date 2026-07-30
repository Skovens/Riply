use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

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
}
