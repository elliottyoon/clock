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
//! See "Logical Physical Clocks and Consistent Snapshots in Globally Distributed Databases" by
//! Kulkarni et al. for more detail in motivation, proof of correctness, properties, stress testing
//! + performance results, and discussion.

use crate::LamportClock;
use rsntp::{SntpClient, SntpDuration};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MASK_48_MSB: u64 = 0xFFFFFFFFFFFF0000;
const NTP_SYNC_INTERVAL: Duration = Duration::from_secs(60);

struct ClockSync {
    ntp: SntpClient,
    time_offset: f64,
    last_ntp_sync: SystemTime,
}

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
    /// A bundle of all NTP-related state.
    ///
    /// When a clock is sent to another process, the only relevant fields are its timestamps
    /// (`self.l` and `self.c`) so we can disregard all the NTP-related stuff. Wrapping it as an
    /// `Option` allows it to have essentially no memory footprint unless we'll use it.
    sync: Option<ClockSync>,
}

impl HybridLogicalClock {
    /// A boring old constructor.
    pub fn new() -> Self {
        Self {
            l: 0.0,
            c: 0,
            sync: Some(ClockSync {
                ntp: SntpClient::new(),
                time_offset: 0.0,
                last_ntp_sync: SystemTime::UNIX_EPOCH,
            }),
        }
    }

    /// Compacts the `l` and `c` timestamps of the clock into a single 64-bit value.
    fn compact_timestamps(&self) -> u64 {
        let rounded_l = (self.l.to_bits()) & MASK_48_MSB;
        rounded_l + self.c as u64
    }

    /// Unpacks a 64 bit representation of the HLC into the struct representation.
    fn decompose_into_timestamps(value: u64) -> Self {
        let l = f64::from_bits(value & MASK_48_MSB);
        let c = (value & !MASK_48_MSB) as u16;
        HybridLogicalClock { l, c, sync: None }
    }

    /// Gets the current hybrid timestamp as duration of seconds represented by a 64-bit float.
    fn get_current_timestamp(&mut self) -> f64 {
        #[inline]
        fn system_time_now_as_secs() -> f64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64()
        }

        let sync = self.sync.as_mut().unwrap();

        // If we're out of the sync-free window, we need to resynchronize.
        if SystemTime::now()
            .duration_since(sync.last_ntp_sync)
            .unwrap_or(Duration::ZERO)
            >= NTP_SYNC_INTERVAL
        {
            let ntp_now = sync
                .ntp
                .synchronize("pool.ntp.org")
                .unwrap()
                .datetime()
                .unix_timestamp()
                .unwrap()
                .as_secs_f64();
            let system_now = system_time_now_as_secs();

            sync.time_offset = ntp_now - system_now;
            sync.last_ntp_sync = SystemTime::now();
        }

        // Now recompute current time using the freshest system clock + offset.
        system_time_now_as_secs() + sync.time_offset
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
            sync: None,
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

impl Into<Duration> for HybridLogicalClock {
    fn into(self) -> Duration {
        Duration::from_secs_f64(self.l)
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
        self.l.partial_cmp(&other.l)
    }
}
