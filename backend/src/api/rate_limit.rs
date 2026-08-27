//! In-memory sliding-window rate limit per client IP and the `ClientIp` extractor.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use crate::config::RateLimit;

/// At most `count` hits per `per` for each key; state lives in this process only.
#[derive(Debug)]
pub struct RateLimiter {
    limit: RateLimit,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(limit: RateLimit) -> Self {
        Self {
            limit,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn limit(&self) -> RateLimit {
        self.limit
    }

    /// Record a hit for `key` at `now`; `false` when the limit is exhausted.
    pub fn check_at(&self, key: &str, now: Instant) -> bool {
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        let window: Duration = self.limit.per;
        // Drop expired entries of other keys now and then so the map stays bounded.
        if hits.len() > 1024 {
            hits.retain(|_, q| {
                q.back()
                    .is_some_and(|last| now.duration_since(*last) < window)
            });
        }
        let queue = hits.entry(key.to_owned()).or_default();
        while queue
            .front()
            .is_some_and(|first| now.duration_since(*first) >= window)
        {
            queue.pop_front();
        }
        if queue.len() >= self.limit.count as usize {
            return false;
        }
        queue.push_back(now);
        true
    }

    pub fn check(&self, key: &str) -> bool {
        self.check_at(key, Instant::now())
    }
}

/// Client address: first entry of `X-Forwarded-For`, else the peer address
/// (`ConnectInfo`), else `"unknown"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIp(pub String);

pub fn client_ip_from(parts: &Parts) -> String {
    if let Some(forwarded) = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
        && !first.trim().is_empty()
    {
        return first.trim().to_owned();
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(client_ip_from(parts)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn sliding_window_limits_per_key() {
        let limiter = RateLimiter::new(RateLimit {
            count: 2,
            per: Duration::from_secs(60),
        });
        let t0 = Instant::now();
        assert!(limiter.check_at("a", t0));
        assert!(limiter.check_at("a", t0 + Duration::from_secs(1)));
        assert!(!limiter.check_at("a", t0 + Duration::from_secs(2)));
        // Other keys are independent.
        assert!(limiter.check_at("b", t0 + Duration::from_secs(2)));
        // The first hit (t0) expires after the window, the second (t0+1) not yet.
        assert!(limiter.check_at("a", t0 + Duration::from_secs(60)));
        assert!(!limiter.check_at("a", t0 + Duration::from_secs(60)));
        // Both old hits gone: only the one from t0+60 remains.
        assert!(limiter.check_at("a", t0 + Duration::from_secs(61)));
        assert!(!limiter.check_at("a", t0 + Duration::from_secs(62)));
        assert_eq!(limiter.limit().count, 2);
    }

    #[test]
    fn client_ip_prefers_forwarded_header() {
        let request = Request::builder()
            .header("x-forwarded-for", " 203.0.113.9 , 10.0.0.1")
            .body(())
            .unwrap();
        let (parts, _) = request.into_parts();
        assert_eq!(client_ip_from(&parts), "203.0.113.9");

        let mut request = Request::builder().body(()).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo("192.0.2.4:5555".parse::<SocketAddr>().unwrap()));
        let (parts, _) = request.into_parts();
        assert_eq!(client_ip_from(&parts), "192.0.2.4");

        let request = Request::builder()
            .header("x-forwarded-for", "")
            .body(())
            .unwrap();
        let (parts, _) = request.into_parts();
        assert_eq!(client_ip_from(&parts), "unknown");
    }
}
