//! Client-side pacing for ABW's per-user LLM budget.
//!
//! GPUI-free by construction so it can be tested with
//! `cargo test -p fermi-console --lib` in seconds, and clock-free so the
//! tests do not sleep: every entry point takes `now` as an argument.
//!
//! ═══════════════════════════════════════════════════════════════════
//!
//! ABW meters agent execution per user, not per route. Both
//! `/api/agents/:id/execute` and `/api/agents/:id/execute/stream` draw on
//! one bucket — `RateLimitConfig::llm`, 10 requests per 60s by default —
//! and a 429 carries the true retry delay in the response BODY, because
//! the server sets no `Retry-After` header.
//!
//! The console used to have no notion of any of this. Observed
//! 2026-08-22, on a five-driver forecast where the operator accepted the
//! recommended agent for each driver in turn:
//!
//! ```text
//! 11:07:23  fermi_base_rate                started
//! 11:07:25  energy_advisor                 started
//! 11:07:35  macro_forecaster               started
//! 11:07:47  macro_forecaster    FAILED     429, retry after 28s
//! 11:07:50  entity_investigator FAILED     429, retry after 25s
//! 11:07:56  macro_forecaster    FAILED     429, retry after 19s
//! ```
//!
//! Three paid-for assignments died inside nine seconds, permanently:
//! there was no retry, and the one recovery path that did exist — falling
//! back from the streaming endpoint to the non-streaming one — spent a
//! SECOND token from the same exhausted bucket, deepening the deficit for
//! the siblings still in flight. The operator saw three drivers that
//! never moved and an error naming a 5-second delay that nothing waited
//! for and that was not the delay the server had asked for.
//!
//! The fix is to stop treating the limit as an error condition. A launch
//! is reserved against a local model of the server's window before it is
//! issued, so a fan-out of five queues itself into a legal cadence
//! instead of racing into a 429. [`LaunchPacer::penalise`] folds any 429
//! we still take back into that model, so the server teaches the client
//! rather than merely refusing it.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Launches permitted per [`DEFAULT_WINDOW`].
///
/// Two below the server's default of 10, on purpose. The console is not
/// the only thing spending from this bucket — base-rate refreshes,
/// decomposition and URL ingestion all charge the same user — and a
/// client that paces itself exactly to the documented ceiling will cross
/// it on clock skew alone. Headroom is cheaper than a dead run.
pub const DEFAULT_CAPACITY: usize = 8;

/// The window those launches are counted over.
///
/// Five seconds longer than the server's 60s for the same reason: the
/// server's window closes on ITS clock, and being early is a 429 while
/// being late is a wait.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(65);

/// How long a rate-limited run waits before its next attempt when the
/// server declines to say. The server does say, in the response body —
/// see [`retry_after_secs`] — so this is the floor for a malformed or
/// truncated message, not the expected path.
pub const FALLBACK_RETRY: Duration = Duration::from_secs(15);

/// The number of times a rate-limited agent run is re-attempted.
///
/// A 429 is not a failure of the agent, it is a statement about when the
/// agent may run. Giving up on the first one turned a scheduling problem
/// into a lost assignment.
pub const MAX_RATE_LIMIT_RETRIES: u32 = 3;

/// A local model of the server's sliding window.
///
/// Reservations are the unit, not requests: [`LaunchPacer::reserve`]
/// decides *when* a launch may go out and books that instant in one
/// step, so N tasks reserving concurrently stagger instead of each
/// observing an empty bucket and all firing at once.
#[derive(Debug)]
pub struct LaunchPacer {
    capacity: usize,
    window: Duration,
    /// Instants at which launches are booked, oldest first. Entries may
    /// be in the future — those are the queued ones.
    booked: VecDeque<Instant>,
    /// A floor imposed by the server via a 429. Nothing goes out before
    /// this, whatever the local model believes.
    blocked_until: Option<Instant>,
}

impl Default for LaunchPacer {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY, DEFAULT_WINDOW)
    }
}

impl LaunchPacer {
    pub fn new(capacity: usize, window: Duration) -> Self {
        Self {
            capacity: capacity.max(1),
            window,
            booked: VecDeque::new(),
            blocked_until: None,
        }
    }

    /// Book the next launch and report how long its caller must wait.
    ///
    /// `Duration::ZERO` means "go now". Anything else is a queue position,
    /// and the caller is expected to actually wait it out — the booking has
    /// already been made on the assumption that it will.
    pub fn reserve(&mut self, now: Instant) -> Duration {
        self.forget_expired(now);

        let mut at = now;
        if let Some(floor) = self.blocked_until {
            at = at.max(floor);
        }
        if self.booked.len() >= self.capacity {
            // The launch `capacity` places back must fall out of the
            // window before this one may go out.
            let oldest_blocking = self.booked[self.booked.len() - self.capacity];
            at = at.max(oldest_blocking + self.window);
        }

        self.booked.push_back(at);
        at.saturating_duration_since(now)
    }

    /// Fold a server 429 into the model.
    ///
    /// Everything already booked inside the penalty is pushed out behind
    /// it. Without that, the siblings of a run that just took a 429 would
    /// keep their earlier slots and walk straight into the same wall — which
    /// is precisely the cascade the log above recorded.
    pub fn penalise(&mut self, now: Instant, delay: Duration) {
        let floor = now + delay;
        self.blocked_until = Some(match self.blocked_until {
            Some(existing) if existing > floor => existing,
            _ => floor,
        });
        for slot in self.booked.iter_mut() {
            if *slot < floor {
                *slot = floor;
            }
        }
    }

    /// Launches currently booked inside the window, including queued ones.
    pub fn in_window(&mut self, now: Instant) -> usize {
        self.forget_expired(now);
        self.booked.len()
    }

    fn forget_expired(&mut self, now: Instant) {
        while let Some(front) = self.booked.front() {
            if *front + self.window <= now {
                self.booked.pop_front();
            } else {
                break;
            }
        }
        if self.blocked_until.is_some_and(|b| b <= now) {
            self.blocked_until = None;
        }
    }
}

/// The retry delay a 429 is really asking for.
///
/// ABW puts it in the response body and nowhere else:
///
/// ```text
/// LLM rate limit exceeded (10/min). Retry after 28 seconds.
/// ```
///
/// `header` is still consulted first, because `Retry-After` is the
/// standard place to look and a proxy or a future server version may set
/// it. Both are absent → `None`, and the caller applies
/// [`FALLBACK_RETRY`] rather than inventing a number and reporting it as
/// though the server had said it. That invention is the reason the
/// console's own error message read "retry after 5s" in the same sentence
/// as the server's "Retry after 28 seconds".
pub fn retry_after_secs(header: Option<&str>, body: &str) -> Option<u64> {
    if let Some(h) = header.map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(secs) = h.parse::<u64>() {
            return Some(secs);
        }
    }

    // "Retry after 28 seconds." — take the first integer following the
    // word "after", so a leading "(10/min)" is not mistaken for a delay.
    let lower = body.to_ascii_lowercase();
    let tail = lower.split("after").nth(1)?;
    let digits: String = tail
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<u64>().ok()
}

/// Whether an error string names a rate limit.
///
/// The streaming path never builds a typed `ApiError` — it flattens its
/// failures to `format!("HTTP {}: {}", status, body)` — so the only thing
/// a caller downstream of it has to classify is prose.
pub fn is_rate_limited(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("429") || m.contains("rate limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_fan_out_within_capacity_goes_out_immediately() {
        let mut p = LaunchPacer::new(8, Duration::from_secs(65));
        let now = t0();
        for i in 0..8 {
            assert_eq!(
                p.reserve(now),
                Duration::ZERO,
                "launch {i} should not have been delayed"
            );
        }
    }

    #[test]
    fn the_ninth_launch_waits_for_the_first_to_age_out() {
        // The whole point: five drivers assigned in thirty seconds must
        // queue rather than take a 429 each.
        let mut p = LaunchPacer::new(3, Duration::from_secs(60));
        let now = t0();
        p.reserve(now);
        p.reserve(now + Duration::from_secs(1));
        p.reserve(now + Duration::from_secs(2));

        let wait = p.reserve(now + Duration::from_secs(3));
        // The first launch was at t+0 and occupies the bucket until t+60,
        // so the fourth may go out at t+60 — 57s after it asked.
        assert_eq!(wait, Duration::from_secs(57));
    }

    #[test]
    fn concurrent_reservations_stagger_instead_of_all_seeing_an_empty_bucket() {
        // Every one of these reserves at the SAME instant, which is what
        // a `for` loop over five staged drivers actually does.
        let mut p = LaunchPacer::new(2, Duration::from_secs(60));
        let now = t0();
        let waits: Vec<Duration> = (0..5).map(|_| p.reserve(now)).collect();
        assert_eq!(waits[0], Duration::ZERO);
        assert_eq!(waits[1], Duration::ZERO);
        assert_eq!(waits[2], Duration::from_secs(60));
        assert_eq!(waits[3], Duration::from_secs(60));
        assert_eq!(waits[4], Duration::from_secs(120));
    }

    #[test]
    fn a_server_penalty_pushes_out_everything_already_queued() {
        // The cascade from the log: one run takes a 429 and its siblings
        // keep their slots, so they walk into the same wall in sequence.
        let mut p = LaunchPacer::new(4, Duration::from_secs(60));
        let now = t0();
        assert_eq!(p.reserve(now), Duration::ZERO);
        assert_eq!(p.reserve(now), Duration::ZERO);
        p.penalise(now + Duration::from_secs(1), Duration::from_secs(28));

        // Anything reserved after the penalty waits it out, even though
        // the local model still thinks the bucket has room.
        let wait = p.reserve(now + Duration::from_secs(2));
        assert_eq!(wait, Duration::from_secs(27));
    }

    #[test]
    fn a_penalty_also_moves_the_slots_that_were_already_queued() {
        // The booked-but-not-yet-fired launches are the siblings of the
        // run that took the 429. Leaving them where they were is what
        // turned one 429 into three.
        let mut p = LaunchPacer::new(1, Duration::from_secs(60));
        let now = t0();
        assert_eq!(p.reserve(now), Duration::ZERO);
        // Queued behind it, at t+60.
        assert_eq!(p.reserve(now), Duration::from_secs(60));
        p.penalise(now, Duration::from_secs(90));

        // Both slots moved to t+90, so the third waits a full window
        // behind the second rather than inheriting the old schedule.
        assert_eq!(p.reserve(now), Duration::from_secs(150));
    }

    #[test]
    fn a_penalty_never_shortens_an_existing_one() {
        let mut p = LaunchPacer::new(4, Duration::from_secs(60));
        let now = t0();
        p.penalise(now, Duration::from_secs(30));
        p.penalise(now, Duration::from_secs(5));
        assert_eq!(p.reserve(now), Duration::from_secs(30));
    }

    #[test]
    fn the_window_empties_again() {
        let mut p = LaunchPacer::new(2, Duration::from_secs(60));
        let now = t0();
        p.reserve(now);
        p.reserve(now);
        assert_eq!(p.in_window(now), 2);
        assert_eq!(p.in_window(now + Duration::from_secs(61)), 0);
        assert_eq!(p.reserve(now + Duration::from_secs(61)), Duration::ZERO);
    }

    #[test]
    fn the_delay_is_read_from_the_body_because_the_server_sends_no_header() {
        // Verbatim from `api_server.rs`, which formats the delay into the
        // body text and returns a bare `(StatusCode, String)`.
        let body = "LLM rate limit exceeded (10/min). Retry after 28 seconds.";
        assert_eq!(retry_after_secs(None, body), Some(28));
    }

    #[test]
    fn the_rate_in_the_message_is_not_mistaken_for_the_delay() {
        // "(10/min)" precedes the delay. Anchoring on "after" is what
        // keeps the 10 out of it.
        let body = "LLM rate limit exceeded (10/min). Retry after 3 seconds.";
        assert_eq!(retry_after_secs(None, body), Some(3));
    }

    #[test]
    fn a_standard_header_still_wins_if_one_ever_arrives() {
        let body = "LLM rate limit exceeded (10/min). Retry after 28 seconds.";
        assert_eq!(retry_after_secs(Some("9"), body), Some(9));
        // A date-form Retry-After is not an integer; fall through to the
        // body rather than guessing.
        assert_eq!(
            retry_after_secs(Some("Wed, 21 Oct 2026 07:28:00 GMT"), body),
            Some(28)
        );
    }

    #[test]
    fn an_unparseable_message_yields_nothing_rather_than_a_fabricated_number() {
        assert_eq!(retry_after_secs(None, ""), None);
        assert_eq!(retry_after_secs(None, "too many requests"), None);
    }

    #[test]
    fn the_flattened_sse_error_is_recognisable_as_a_rate_limit() {
        // This is the exact string the streaming path produces.
        assert!(is_rate_limited(
            "HTTP 429: LLM rate limit exceeded (10/min). Retry after 28 seconds."
        ));
        assert!(is_rate_limited("Rate limited — retry after 5s"));
        assert!(!is_rate_limited("stream ended without a complete event"));
        assert!(!is_rate_limited("HTTP 404: no such agent"));
    }
}
