//! Per-guest rate limits: token buckets the host applies to every message of an admitted
//! guest before acting on it.
//!
//! Requests that cost the host work and can be refused without breaking anything have their
//! own bucket: project calls ([`CALL_RATE`]), document open / save / sync ([`DOC_RATE`]) and
//! terminal and run input ([`INPUT_RATE`]). A refused request gets its normal answer with
//! [`TOO_MANY`], and the session log gets one entry per burst (refusals less than [`QUIET`]
//! apart belong to the same burst).
//!
//! Edits, carets and presence (every other message) are never refused: dropping a Yjs update
//! would make the replicas diverge. Instead they count against [`FLOOD_RATE`], set far above
//! what a person can type or click; a guest that exceeds it is disconnected ([`FLOODED`]).
//! A refused request costs the host only its short answer, so requests never disconnect.
//!
//! Time is passed in, so the buckets are tested without sleeping.

use std::time::{Duration, Instant};

use crate::protocol::GuestMessage;

/// A bucket's size: `burst` messages at once, refilled with `per_second`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub burst: u32,
    pub per_second: u32,
}

/// Project calls (`GuestMessage::Call`): tree, search, git, language servers. Typing with
/// completion, hover and synchronization takes about 30 a second, opening a project or
/// expanding a large tree a few hundred at once; at most `MAX_PENDING_CALLS` run at once
/// anyway.
pub const CALL_RATE: Rate = Rate {
    burst: 1000,
    per_second: 200,
};
/// Opening, saving and resynchronizing documents (a reconnect resynchronizes every open tab
/// at once).
pub const DOC_RATE: Rate = Rate {
    burst: 100,
    per_second: 20,
};
/// Keys typed into shared terminals and runs (a held key repeats about 30 times a second).
pub const INPUT_RATE: Rate = Rate {
    burst: 500,
    per_second: 100,
};
/// The messages that are never refused (edits, carets, presence, closing documents): about
/// ten times what fast typing with caret and presence updates sends. Exceeding it
/// disconnects the guest.
pub const FLOOD_RATE: Rate = Rate {
    burst: 1000,
    per_second: 300,
};
/// Refusals closer together than this are one burst and logged once.
pub const QUIET: Duration = Duration::from_secs(5);

/// The answer to a request over its rate.
pub const TOO_MANY: &str = "Too many requests. Try again in a moment.";
/// Why a guest over [`FLOOD_RATE`] was disconnected (shown to the guest).
pub const FLOODED: &str =
    "The host disconnected you because your Forge sent far more messages than anyone can type.";
/// The session log's reason for the same.
pub const FLOODED_LOG: &str = "Sent far more messages than anyone can type; disconnected.";

/// A token bucket: holds up to `burst` tokens, gains `per_second`, each message takes one.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    refill: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// A full bucket.
    pub fn new(rate: Rate, now: Instant) -> Self {
        Self {
            capacity: f64::from(rate.burst),
            refill: f64::from(rate.per_second),
            tokens: f64::from(rate.burst),
            last: now,
        }
    }

    /// Take a token if there is one. A `now` before the last call counts as no time passed.
    pub fn try_take(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill).min(self.capacity);
        self.last = self.last.max(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// The refusable kinds of requests, each with its own bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Calls,
    Docs,
    Input,
}

impl Limit {
    /// The bucket a message is refused by, if any.
    pub fn of(msg: &GuestMessage) -> Option<Self> {
        match msg {
            GuestMessage::Call { .. } => Some(Self::Calls),
            GuestMessage::DocOpen { .. }
            | GuestMessage::DocSave { .. }
            | GuestMessage::DocSync { .. } => Some(Self::Docs),
            GuestMessage::TerminalInput { .. } | GuestMessage::RunInput { .. } => Some(Self::Input),
            GuestMessage::Knock { .. }
            | GuestMessage::Resume { .. }
            | GuestMessage::DocUpdate { .. }
            | GuestMessage::Awareness { .. }
            | GuestMessage::DocClose { .. }
            | GuestMessage::Presence { .. }
            | GuestMessage::Leave => None,
        }
    }

    fn rate(self) -> Rate {
        match self {
            Self::Calls => CALL_RATE,
            Self::Docs => DOC_RATE,
            Self::Input => INPUT_RATE,
        }
    }
}

/// What to do with a guest's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Answer it with [`TOO_MANY`]; `report`: the first refusal of a burst, to log (and,
    /// for input that has no answer of its own, to tell the guest).
    Refuse {
        report: bool,
    },
    /// Over [`FLOOD_RATE`]: disconnect the guest.
    Flood,
}

#[derive(Debug, Clone)]
struct Limited {
    bucket: TokenBucket,
    last_refused: Option<Instant>,
}

/// The buckets of one guest.
#[derive(Debug, Clone)]
pub struct GuestLimits {
    unrefused: TokenBucket,
    limited: [Limited; 3],
}

impl GuestLimits {
    pub fn new(now: Instant) -> Self {
        let limited = |limit: Limit| Limited {
            bucket: TokenBucket::new(limit.rate(), now),
            last_refused: None,
        };
        Self {
            unrefused: TokenBucket::new(FLOOD_RATE, now),
            limited: [
                limited(Limit::Calls),
                limited(Limit::Docs),
                limited(Limit::Input),
            ],
        }
    }

    /// Count `msg`, received at `now`.
    pub fn check(&mut self, msg: &GuestMessage, now: Instant) -> Verdict {
        let Some(limit) = Limit::of(msg) else {
            return if self.unrefused.try_take(now) {
                Verdict::Allow
            } else {
                Verdict::Flood
            };
        };
        let l = &mut self.limited[limit as usize];
        if l.bucket.try_take(now) {
            return Verdict::Allow;
        }
        let report = l
            .last_refused
            .is_none_or(|t| now.saturating_duration_since(t) >= QUIET);
        l.last_refused = Some(now);
        Verdict::Refuse { report }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DocId;
    use crate::share::TerminalId;
    use serde_bytes::ByteBuf;

    fn call() -> GuestMessage {
        GuestMessage::Call {
            id: 1,
            payload: ByteBuf::new(),
        }
    }

    fn edit() -> GuestMessage {
        GuestMessage::DocUpdate {
            doc: DocId(1),
            update: ByteBuf::new(),
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_bucket_allows_its_burst_then_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(
            Rate {
                burst: 3,
                per_second: 10,
            },
            t0,
        );
        assert!((0..3).all(|_| b.try_take(t0)));
        assert!(!b.try_take(t0));
        // 10 a second: one token every 100 ms.
        assert!(!b.try_take(t0 + ms(50)));
        assert!(b.try_take(t0 + ms(100)));
        assert!(!b.try_take(t0 + ms(100)));
        // A long pause refills to the burst, not beyond.
        let later = t0 + Duration::from_secs(60);
        assert!((0..3).all(|_| b.try_take(later)));
        assert!(!b.try_take(later));
    }

    #[test]
    fn a_clock_going_backwards_adds_nothing() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let mut b = TokenBucket::new(
            Rate {
                burst: 1,
                per_second: 1,
            },
            t0,
        );
        assert!(b.try_take(t0));
        assert!(!b.try_take(t0 - Duration::from_secs(1)));
        assert!(!b.try_take(t0 + ms(500)));
        assert!(b.try_take(t0 + ms(1000)));
    }

    #[test]
    fn requests_over_their_rate_are_refused_and_reported_once_per_burst() {
        let t0 = Instant::now();
        let mut g = GuestLimits::new(t0);
        for _ in 0..CALL_RATE.burst {
            assert_eq!(g.check(&call(), t0), Verdict::Allow);
        }
        assert_eq!(g.check(&call(), t0), Verdict::Refuse { report: true });
        assert_eq!(g.check(&call(), t0), Verdict::Refuse { report: false });
        // Other kinds have their own buckets.
        let open = GuestMessage::DocOpen {
            request: 1,
            path: "a.php".into(),
        };
        assert_eq!(g.check(&open, t0), Verdict::Allow);
        let key = GuestMessage::TerminalInput {
            terminal: TerminalId(1),
            data: "x".into(),
        };
        assert_eq!(g.check(&key, t0), Verdict::Allow);
        // Refusals keep coming: still the same burst.
        let mut at = t0;
        for _ in 0..20 {
            at += ms(400);
            // Exactly what refilled is allowed, the rest refused without a new report.
            while g.check(&call(), at) == Verdict::Allow {}
        }
        assert_eq!(g.check(&call(), at), Verdict::Refuse { report: false });
    }

    #[test]
    fn a_new_burst_after_a_quiet_pause_is_reported_again() {
        let t0 = Instant::now();
        let mut g = GuestLimits::new(t0);
        while g.check(&call(), t0) == Verdict::Allow {}
        let later = t0 + QUIET + Duration::from_secs(5);
        let mut verdict = Verdict::Allow;
        while verdict == Verdict::Allow {
            verdict = g.check(&call(), later);
        }
        assert_eq!(verdict, Verdict::Refuse { report: true });
    }

    #[test]
    fn edits_are_never_refused_but_a_flood_disconnects() {
        let t0 = Instant::now();
        let mut g = GuestLimits::new(t0);
        // Fast typing for a minute (an edit, a caret and a presence update per key, 15 keys
        // a second) stays far below the limit.
        for i in 0..60 * 15 * 3 {
            let at = t0 + Duration::from_micros(i * 1_000_000 / 45);
            assert_eq!(g.check(&edit(), at), Verdict::Allow);
        }
        let t1 = t0 + Duration::from_secs(61);
        for _ in 0..FLOOD_RATE.burst {
            assert_eq!(g.check(&edit(), t1), Verdict::Allow);
        }
        assert_eq!(g.check(&edit(), t1), Verdict::Flood);
    }

    #[test]
    fn refused_requests_never_disconnect_and_leave_edits_alone() {
        let t0 = Instant::now();
        let mut g = GuestLimits::new(t0);
        for _ in 0..(CALL_RATE.burst + FLOOD_RATE.burst) * 2 {
            assert_ne!(g.check(&call(), t0), Verdict::Flood);
        }
        assert_eq!(g.check(&edit(), t0), Verdict::Allow);
    }
}
