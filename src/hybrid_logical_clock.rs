//! The goal of HLC is to provide one-way causality detection similar to that provided by lamport
//! clocks, while maintaining the clock value to be always close to the physical/NTP clock.
//!
//! Formally, given a distributed system, we need to assign each event `e` a timestamp, `L(e)`,
//! such that:
//!   1. `e -> f => L(e) < L(f),`
//!   2. Space requirement for `L(e)` is O(1) integers (unlike vector clocks, for example),
//!   3. `L(e)` is represented with bounded space, and
//!   4. `L(e)` is *close* to the physical time of `e`, i.e. `| L(e) - PT(e) |` is bounded.
//! In practice, we want the size of `L(e)` to be the same as `PT(e)` (64 bits in NTP protocol).
//! Note that goal (4) enables us to utilize HLC in place of physical time.
//!
//! ---
//!
//! A naive algorithm to satisfy requirements (1) - (4) might be intuitively proposed as follows:
//! - Initially, `L(j) := 0`
//! - **Send or local event**
//!     1. `L(j) := max(1 + L(j), PT(j))`
//!     2. Timestamp with `L(j)`
//! - **Receive event of message `m`**
//!     1. `L(j) := max(1 + L(j), 1 + L(m), PT(j))`
//!     2. Timestamp with `L(j)`
//!
//! However, this algorithm fails to satisfy (4) due to edge cases of unbounded drift between
//! physical and logical time. Since L is used by the algorithm to maintain both the maximum of PT
//! values seen so far and also the logical clock increments from new events (local/send/receive),
//! clocks lose information when it becomes unclear if the new L value came from PT or causality.
//! Thus, there's no suitable place to reset the L value to bound the L-PT difference, as such a
//! reset might fuck up the -> relation, violating (1) and defeating the whole purpose of having a
//! logical clock for causality detection in the first place.
//!
//! Instead, we'll use a level of indirection (yay!!) to develop the correct algorithm, expanding
//! the L(j) timestamp from the naive algorithm into two parts: L(j) and C(j), where
//! - L(j) is introduced as a level of indirection to maintain the maximum of PT information
//!   learned so far, and
//! - C(j) is used to capture causality updates only when L values are equal. In contrast to the
//!   naive algorithm where we couldn't reset L without violating ->, we can reset C when the
//!   information heard about maximum PT catches up or goes ahead of L. Since L denotes the
//!   *maximum* PT heard among nodes and is not necessarily continuously incremented with each
//!   event, within a bounded time either
//!     1. a node receives a message with a larger L, its own L value is updated and C is reset to
//!        reflect this, or
//!     2. its L stays the same, and its PT will catch up and update its L, followed by a reset to
//!        C to reflect the reset.
//!
//! ---
//!
//! ## The Hybrid Lamport Clock algorithm:
//! - `L(j) := 0, C(j) := 0`
//! - **Send or local event**
//!     1. `L'(j) := L(j)`
//!     2. `L(j) := max(L'(j), PT(j))`
//!     3. Set `C(j)` to be
//!         - `C(j) + 1`   if `(L(j) == L'(j))`
//!         - `0`          otherwise
//! - **Receive event of message `m`**
//!     1. `L'(j) := L(j)`
//!     2. `L(j) := max(L'(j), L(m), PT(j))`
//!     3. Set `C(j)` to be
//!         - `1 + max(C(j), C(m))`   if      `L(j) == L'(j)) == L(m)`
//!         - `1 + C(j)`              else if `L(j) == L'(j)`
//!         - `1 + C(m)`              else if `L(j) == L(m)`
//!         - 0                       otherwise.
//!     4. Timestamp with `L(j), C(j)`
//!
//! Note that we compare `(L(e), C(e))` timestamp pairs lexicographically, i.e. ordered by `L`
//! first, then by `C` if necessary.
//! ---
//!
//! The paper "Logical Physical Clocks and Consistent Snapshots in Globally Distributed Databases"
//! by Kulkarni et al. proves the following assertions given the HLC algorithm above:
//! - **Theorem 1** For any two events `e` and `f`, `e -> f` implies `(L(e), C(e)) >= (L(e), C(f))`
//! (Requirement 1: ✅)
//! - **Theorem 2** For any event `f`, `L(f) >= PT(f)` (Requirement 4: ✅)
//! - **Theorem 3** `L(f)` denotes the maximum clock value that `f` is aware of. In other words,
//!   `L(f) > PT(f)` implies that there exists some `g` such that `g -> f` and `PT(g) = L(f)`.
//! - **Corollary 1** For any event `f`, `|L(f) - PT(f)| <= ϵ`.
//! - **Theorem 4** For any event `f`, if `C(f) == k` and `k > 0`, then there exist `g1, ..., gk`
//!   such that
//!     1. `gi -> g(i+1)` for all `1 <= j < k`,
//!     2. `L(gi) == L(f)` for all `1 <= j <= k`, and
//!     3. `gk -> f`.
//! - **Corollary 2** For any event `f`, `C(f) <= |{g : g -> f and L(g) == L(f)}|`
//! - **Corollary 3** For any event `f`, `C(f) <= N * (ϵ + 1)`.
//! - **Corollary 4** Under the assumption that the time for message transmission is long enough so
//!   that the physical clock of every node is incremented by at least `d`, a given parameter, then
//!   `C(f) <= 1 + ϵ/d`
use crate::LamportClock;
use rsntp::SntpClient;

const MASK_48_MSB: u64 = 0xFFFFFFFFFFFF0000;

pub struct HybridLogicalClock {
    /// The maximum physical timestamp (PT) observed so far, either from local events or received
    /// messages. This tracks the highest PT known to the node and is monotonically non-decreasing.
    l: f64,
    /// The logical counter used to distinguish causally related events that happen at the same
    /// physical time `l`. This counter increments when multiple events occur with the same `l`.
    ///
    /// We choose to represent this as a 16-bit integer for compaction, as rounding the physical
    /// timestamp to the 48 most significant bits allows for microsecond-level granularity and
    /// 16 bits for `c` gives it room to grow up to 65536, which is more than enough (probably).
    c: u16,
    /// The NTP client used to fetch physical timestamps. This is wrapped in an Option so that
    /// when we send an HLC to another process with a message, we don't have to actually send a
    /// client with it, just the `l` and `c` timestamps.
    ntp: Option<SntpClient>,
}

impl HybridLogicalClock {
    /// Compacts the `l` and `c` timestamps of the clock into a single 64-bit value.
    fn compact_timestamps(&self) -> u64 {
        let mask = 0xFFFFFFFFFFFF0000_u64;
        let rounded_l = (self.l as u64) & mask;
        rounded_l + self.c as u64
    }

    /// Unpacks a 64 bit representation of the HLC into the struct representation.
    fn decompose_into_timestamps(value: u64) -> Self {
        let l = (value & MASK_48_MSB) as f64;
        let c = (value & !MASK_48_MSB) as u16;
        HybridLogicalClock { l, c, ntp: None }
    }

    /// Gets the current NTP timestamp as duration of seconds represented by a 64-bit float.
    fn get_current_timestamp(&self) -> f64 {
        self.ntp
            .as_ref()
            .unwrap()
            .synchronize("pool.ntp.org")
            .unwrap()
            .datetime()
            .unix_timestamp()
            .unwrap()
            .as_secs_f64()
    }
}

impl LamportClock for HybridLogicalClock {
    fn bump(&mut self) {
        let pt = self.get_current_timestamp();
        if pt > self.l {
            // If we advance to a new max physical timestamp (`l`) in the system, reset the counter
            // of causally related events, since we're the first event to occur at this timestamp!
            self.l = pt;
            self.c = 0;
        } else {
            // Otherwise, this is yet another event at the current `l` value, and we should update
            // our event counter accordingly.
            self.c += 1;
        }
    }

    fn send(&mut self) -> Self {
        self.bump();

        // The receiving clock doesn't care about the ntp client, just the timestamps.
        Self {
            c: self.c,
            l: self.l,
            ntp: None,
        }
    }

    fn receive(&mut self, incoming_clock: &Self) {
        let prev_l = self.l;
        let pt = self.get_current_timestamp();

        self.l = pt.max(prev_l.max(incoming_clock.l));
        self.c = match (self.l == prev_l, self.l == incoming_clock.l) {
            // The incoming clock and us both are at the same `l` value, so we need to ensure the
            // counter of events occurring at timestamp `l` is greater than both what our clock and
            // the incoming clock had.
            (true, true) => 1 + u16::max(self.c, incoming_clock.c),
            // Our clock's max timestamp is ahead of the incoming one, so we just need to ensure
            // our new `c` value is greater than what the previous version of this clock had.
            (true, false) => 1 + self.c,
            // The incoming clock's max timestamp is ahead of ours, so we need to ensure that
            // our new `c` value is greater than what they had.
            (false, true) => 1 + incoming_clock.c,
            // We're at a new max timestamp, so we're the first event and can reset the counter!
            (false, false) => 0,
        }
    }
}

impl From<u64> for HybridLogicalClock {
    fn from(value: u64) -> Self {
        Self::decompose_into_timestamps(value)
    }
}

impl From<HybridLogicalClock> for u64 {
    fn from(value: HybridLogicalClock) -> u64 {
        value.compact_timestamps()
    }
}

impl PartialEq<Self> for HybridLogicalClock {
    fn eq(&self, other: &Self) -> bool {
        self.l == other.l && self.c == other.c
    }
}

impl PartialOrd for HybridLogicalClock {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.l == other.l {
            return self.c.partial_cmp(&other.c);
        }
        Some(self.l.cmp(&other.l))
    }
}
