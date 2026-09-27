//! The outcome and latency of the server's own calls to GitHub and Stripe, over a sliding five-minute window, for
//! `GET /api/status` (docs/trust-and-status.md §2). Only counts and timings: nothing about who or what was called.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{RequestBuilder, Response, StatusCode};

use crate::github::now_secs;

/// How far back the window reaches.
pub const WINDOW: Duration = Duration::from_secs(5 * 60);
/// Each bucket covers this many seconds; the window is a ring of them.
const BUCKET_SECS: u64 = 10;
/// One more than the window needs, for the bucket being filled.
const BUCKETS: usize = (WINDOW.as_secs() / BUCKET_SECS) as usize + 1;
/// Upper bounds of the latency histogram, in milliseconds; slower calls fall in a last, open-ended bin.
const LATENCY_BOUNDS_MS: [u64; 18] = [
    10, 20, 30, 50, 75, 100, 150, 200, 300, 400, 500, 750, 1000, 1500, 2000, 3000, 5000, 10000,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Any answer, including a 404 that means "not found" and other refusals of a well-formed request.
    Ok,
    /// No answer, or a server error.
    Error,
    RateLimited,
}

impl Outcome {
    /// Classifies a response: rate limits are 429s and GitHub's 403s with no requests remaining, errors are 5xx.
    pub fn of(response: &Response) -> Self {
        let status = response.status();
        let exhausted = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v.as_bytes() == b"0");
        if status == StatusCode::TOO_MANY_REQUESTS || (status == StatusCode::FORBIDDEN && exhausted)
        {
            Self::RateLimited
        } else if status.is_server_error() {
            Self::Error
        } else {
            Self::Ok
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    /// The first second this bucket covers; a stale bucket is reset when its slot comes round again.
    start: u64,
    ok: u64,
    errors: u64,
    rate_limited: u64,
    latencies: [u64; LATENCY_BOUNDS_MS.len() + 1],
}

/// The window's counts, as `GET /api/status` reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub requests: u64,
    pub errors: u64,
    pub rate_limited: u64,
    /// Upper bounds of the histogram bins holding the median and 95th percentile; `None` without requests.
    pub latency_ms_p50: Option<u64>,
    pub latency_ms_p95: Option<u64>,
    /// When a call last failed or was rate limited, in Unix seconds, since the server started.
    pub last_error_at: Option<u64>,
}

/// A ring of ten-second buckets. Recording takes a short lock on the ring; nothing waits on the network under it.
pub struct Metrics {
    ring: Mutex<[Bucket; BUCKETS]>,
    last_error_at: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            ring: Mutex::new([Bucket::default(); BUCKETS]),
            last_error_at: AtomicU64::new(0),
        }
    }
}

impl Metrics {
    pub fn record(&self, outcome: Outcome, latency: Duration) {
        self.record_at(now_secs(), outcome, latency);
    }

    fn record_at(&self, at: u64, outcome: Outcome, latency: Duration) {
        let start = at - at % BUCKET_SECS;
        let slot = (start / BUCKET_SECS) as usize % BUCKETS;
        let ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
        let bin = LATENCY_BOUNDS_MS
            .iter()
            .position(|&bound| ms <= bound)
            .unwrap_or(LATENCY_BOUNDS_MS.len());
        {
            let mut ring = self.ring.lock().expect("metrics lock");
            let bucket = &mut ring[slot];
            if bucket.start != start {
                *bucket = Bucket {
                    start,
                    ..Bucket::default()
                };
            }
            match outcome {
                Outcome::Ok => bucket.ok += 1,
                Outcome::Error => bucket.errors += 1,
                Outcome::RateLimited => bucket.rate_limited += 1,
            }
            bucket.latencies[bin] += 1;
        }
        if outcome != Outcome::Ok {
            self.last_error_at.fetch_max(at, Ordering::Relaxed);
        }
    }

    pub fn summary(&self) -> Summary {
        self.summary_at(now_secs())
    }

    fn summary_at(&self, now: u64) -> Summary {
        let oldest = now.saturating_sub(WINDOW.as_secs());
        let mut summary = Summary::default();
        let mut latencies = [0u64; LATENCY_BOUNDS_MS.len() + 1];
        let ring = *self.ring.lock().expect("metrics lock");
        // A bucket counts while any of its seconds are in the window, so the window is five minutes to within a
        // bucket.
        for bucket in ring
            .iter()
            .filter(|b| b.start + BUCKET_SECS > oldest && b.start <= now)
        {
            summary.requests += bucket.ok + bucket.errors + bucket.rate_limited;
            summary.errors += bucket.errors;
            summary.rate_limited += bucket.rate_limited;
            for (total, count) in latencies.iter_mut().zip(bucket.latencies) {
                *total += count;
            }
        }
        summary.latency_ms_p50 = percentile(&latencies, summary.requests, 50);
        summary.latency_ms_p95 = percentile(&latencies, summary.requests, 95);
        summary.last_error_at = Some(self.last_error_at.load(Ordering::Relaxed)).filter(|&t| t > 0);
        summary
    }
}

/// The upper bound of the histogram bin holding the `pct`th percentile; the open-ended last bin reports its lower
/// bound.
fn percentile(histogram: &[u64], total: u64, pct: u64) -> Option<u64> {
    if total == 0 {
        return None;
    }
    let rank = (total * pct).div_ceil(100);
    let mut seen = 0;
    histogram.iter().enumerate().find_map(|(bin, &count)| {
        seen += count;
        (seen >= rank).then(|| {
            LATENCY_BOUNDS_MS
                .get(bin)
                .or(LATENCY_BOUNDS_MS.last())
                .copied()
                .unwrap_or_default()
        })
    })
}

/// Sends a request and records its outcome and latency: the one place outgoing GitHub and Stripe calls pass through.
pub(crate) trait SendRecorded {
    async fn send_recorded(self, metrics: &Metrics) -> reqwest::Result<Response>;
}

impl SendRecorded for RequestBuilder {
    async fn send_recorded(self, metrics: &Metrics) -> reqwest::Result<Response> {
        let started = Instant::now();
        let result = self.send().await;
        let outcome = result.as_ref().map_or(Outcome::Error, Outcome::of);
        metrics.record(outcome, started.elapsed());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_800_000_000;
    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn an_empty_window() {
        let summary = Metrics::default().summary_at(T);
        assert_eq!(summary, Summary::default());
    }

    #[test]
    fn counts_by_outcome_with_percentiles() {
        let metrics = Metrics::default();
        for i in 0..90 {
            metrics.record_at(T + i % 60, Outcome::Ok, 100 * MS);
        }
        for _ in 0..8 {
            metrics.record_at(T + 30, Outcome::Ok, 2 * MS * 1000);
        }
        metrics.record_at(T + 40, Outcome::Error, 12 * MS);
        metrics.record_at(T + 50, Outcome::RateLimited, 40_000 * MS);
        let summary = metrics.summary_at(T + 60);
        assert_eq!(summary.requests, 100);
        assert_eq!(summary.errors, 1);
        assert_eq!(summary.rate_limited, 1);
        assert_eq!(summary.latency_ms_p50, Some(100));
        assert_eq!(summary.latency_ms_p95, Some(2000));
        assert_eq!(summary.last_error_at, Some(T + 50));
    }

    #[test]
    fn old_calls_leave_the_window() {
        let metrics = Metrics::default();
        metrics.record_at(T, Outcome::Error, 10 * MS);
        metrics.record_at(T + 200, Outcome::Ok, 10 * MS);
        assert_eq!(metrics.summary_at(T + 299).requests, 2);
        let later = metrics.summary_at(T + 310);
        assert_eq!(later.requests, 1);
        assert_eq!(later.errors, 0);
        // The last error is remembered after it leaves the window.
        assert_eq!(later.last_error_at, Some(T));
        assert_eq!(metrics.summary_at(T + 600).requests, 0);
    }

    #[test]
    fn a_reused_slot_starts_again() {
        let metrics = Metrics::default();
        metrics.record_at(T, Outcome::Ok, 10 * MS);
        // The same slot, a full ring later.
        let next = T + BUCKETS as u64 * BUCKET_SECS;
        metrics.record_at(next, Outcome::Ok, 10 * MS);
        assert_eq!(metrics.summary_at(next).requests, 1);
    }

    #[test]
    fn latencies_beyond_the_last_bound() {
        let metrics = Metrics::default();
        metrics.record_at(T, Outcome::Ok, 60_000 * MS);
        assert_eq!(metrics.summary_at(T).latency_ms_p95, Some(10_000));
    }

    #[tokio::test]
    async fn responses_are_classified() {
        use axum::{Router, http::HeaderMap, routing::get};
        let app = Router::new().route(
            "/{status}",
            get(
                |axum::extract::Path(status): axum::extract::Path<u16>| async move {
                    let mut headers = HeaderMap::new();
                    if status == 403 {
                        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
                    }
                    (StatusCode::from_u16(status).unwrap(), headers)
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let client = reqwest::Client::new();
        let metrics = Metrics::default();
        for status in [200, 404, 422, 403, 429, 502] {
            client
                .get(format!("{url}/{status}"))
                .send_recorded(&metrics)
                .await
                .unwrap();
        }
        // Nothing listens on port 9 (discard): the call fails without an answer.
        assert!(
            client
                .get("http://127.0.0.1:9/")
                .send_recorded(&metrics)
                .await
                .is_err()
        );
        let summary = metrics.summary();
        assert_eq!(summary.requests, 7);
        assert_eq!(summary.rate_limited, 2);
        assert_eq!(summary.errors, 2);
    }
}
