use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct RateLimitPolicy {
    pub window: Duration,
    pub max_events: usize,
    pub block_for: Duration,
}

impl RateLimitPolicy {
    pub const fn new(window_secs: u64, max_events: usize, block_for_secs: u64) -> Self {
        Self {
            window: Duration::from_secs(window_secs),
            max_events,
            block_for: Duration::from_secs(block_for_secs),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub just_blocked: bool,
    pub retry_after_seconds: u64,
}

impl RateLimitDecision {
    fn allowed() -> Self {
        Self {
            allowed: true,
            just_blocked: false,
            retry_after_seconds: 0,
        }
    }

    fn blocked(just_blocked: bool, retry_after_seconds: u64) -> Self {
        Self {
            allowed: false,
            just_blocked,
            retry_after_seconds,
        }
    }
}

#[derive(Default)]
pub struct RateLimitStore {
    buckets: HashMap<String, RateLimitBucket>,
}

#[derive(Default)]
struct RateLimitBucket {
    events: VecDeque<Instant>,
    blocked_until: Option<Instant>,
    limited: bool,
}

pub const LOGIN_FAILURE_POLICY: RateLimitPolicy = RateLimitPolicy::new(5 * 60, 3, 30 * 60);
pub const COMMENT_ACTION_POLICY: RateLimitPolicy = RateLimitPolicy::new(60, 5, 0);
pub const LIKE_ACTION_POLICY: RateLimitPolicy = RateLimitPolicy::new(60, 10, 0);

impl RateLimitStore {
    pub fn check_block(&mut self, key: &str, policy: RateLimitPolicy) -> RateLimitDecision {
        self.check_block_at(key, policy, Instant::now())
    }

    pub fn record_event(&mut self, key: &str, policy: RateLimitPolicy) -> RateLimitDecision {
        self.record_event_at(key, policy, Instant::now())
    }

    pub fn reset(&mut self, key: &str) {
        self.buckets.remove(key);
    }

    fn check_block_at(
        &mut self,
        key: &str,
        policy: RateLimitPolicy,
        now: Instant,
    ) -> RateLimitDecision {
        let Some(bucket) = self.buckets.get_mut(key) else {
            return RateLimitDecision::allowed();
        };

        bucket.prune(policy.window, now);

        if let Some(blocked_until) = bucket.blocked_until {
            if blocked_until > now {
                return RateLimitDecision::blocked(false, retry_after_seconds(blocked_until, now));
            }
            bucket.blocked_until = None;
        }

        if bucket.events.len() < policy.max_events {
            bucket.limited = false;
        }

        if bucket.events.is_empty() {
            self.buckets.remove(key);
        }

        RateLimitDecision::allowed()
    }

    fn record_event_at(
        &mut self,
        key: &str,
        policy: RateLimitPolicy,
        now: Instant,
    ) -> RateLimitDecision {
        let bucket = self.buckets.entry(key.to_string()).or_default();
        bucket.prune(policy.window, now);

        if let Some(blocked_until) = bucket.blocked_until {
            if blocked_until > now {
                return RateLimitDecision::blocked(false, retry_after_seconds(blocked_until, now));
            }
            bucket.blocked_until = None;
        }

        if bucket.events.len() < policy.max_events {
            bucket.limited = false;
        }

        if bucket.events.len() >= policy.max_events {
            let just_blocked = !bucket.limited;
            bucket.limited = true;

            if policy.block_for.is_zero() {
                let retry_after_seconds = bucket
                    .events
                    .front()
                    .map(|oldest| {
                        let window_end = *oldest + policy.window;
                        retry_after_seconds(window_end, now)
                    })
                    .unwrap_or(1);
                return RateLimitDecision::blocked(just_blocked, retry_after_seconds);
            }

            let blocked_until = now + policy.block_for;
            bucket.blocked_until = Some(blocked_until);
            return RateLimitDecision::blocked(just_blocked, retry_after_seconds(blocked_until, now));
        }

        bucket.events.push_back(now);
        RateLimitDecision::allowed()
    }
}

impl RateLimitBucket {
    fn prune(&mut self, window: Duration, now: Instant) {
        while let Some(oldest) = self.events.front() {
            if now.duration_since(*oldest) >= window {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
}

fn retry_after_seconds(until: Instant, now: Instant) -> u64 {
    let remaining = until.saturating_duration_since(now);
    let secs = remaining.as_secs();
    if remaining.subsec_nanos() > 0 {
        secs.saturating_add(1)
    } else {
        secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_event_allows_until_threshold_then_blocks() {
        let mut store = RateLimitStore::default();
        let policy = RateLimitPolicy::new(60, 2, 300);
        let start = Instant::now();

        assert!(store.record_event_at("k", policy, start).allowed);
        assert!(store.record_event_at("k", policy, start + Duration::from_secs(1)).allowed);

        let blocked = store.record_event_at("k", policy, start + Duration::from_secs(2));
        assert!(!blocked.allowed);
        assert!(blocked.just_blocked);
        assert!(blocked.retry_after_seconds >= 299);
    }

    #[test]
    fn blocked_key_recovers_after_block_window() {
        let mut store = RateLimitStore::default();
        let policy = RateLimitPolicy::new(60, 1, 10);
        let start = Instant::now();

        assert!(store.record_event_at("k", policy, start).allowed);
        assert!(!store.record_event_at("k", policy, start + Duration::from_secs(1)).allowed);
        assert!(!store.check_block_at("k", policy, start + Duration::from_secs(5)).allowed);
        assert!(store.check_block_at("k", policy, start + Duration::from_secs(11)).allowed);
    }

    #[test]
    fn reset_clears_bucket() {
        let mut store = RateLimitStore::default();
        let policy = RateLimitPolicy::new(60, 1, 10);
        let start = Instant::now();

        assert!(store.record_event_at("k", policy, start).allowed);
        store.reset("k");
        assert!(store.check_block_at("k", policy, start + Duration::from_secs(1)).allowed);
    }

    #[test]
    fn zero_block_policy_only_marks_first_limited_event_as_just_blocked() {
        let mut store = RateLimitStore::default();
        let policy = RateLimitPolicy::new(60, 2, 0);
        let start = Instant::now();

        assert!(store.record_event_at("k", policy, start).allowed);
        assert!(store.record_event_at("k", policy, start + Duration::from_secs(1)).allowed);

        let first_limited = store.record_event_at("k", policy, start + Duration::from_secs(2));
        assert!(!first_limited.allowed);
        assert!(first_limited.just_blocked);
        assert!(first_limited.retry_after_seconds > 0);

        let second_limited = store.record_event_at("k", policy, start + Duration::from_secs(3));
        assert!(!second_limited.allowed);
        assert!(!second_limited.just_blocked);
    }
}
