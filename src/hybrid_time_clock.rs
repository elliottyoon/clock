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
