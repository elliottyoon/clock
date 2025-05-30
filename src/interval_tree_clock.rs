//! Traditional logical clocks require a fixed set of participants (e.g. vector clocks), each with
//! a unique, pre-defined identity. Interval tree clocks (ITCs) eliminate this restriction by
//! encoding identity as a function over the half-open interval [0, 1), allowing for dynamic growth
//! and shrinkage of participants and automatic partitioning of identity space.
//!
//! Each ITC stamp is an (id, event) pair, where
//! - id:    binary tree that describes which sub-intervals of [0,1) a process controls, and
//! - event: another binary tree mapping sub-intervals to non-negative integers that represent the
//!          logical time (i.e. how many events occurred).
//!
//! Full details in "Interval Tree Clocks: A Logical Clock for Dynamic Systems" by Almeida et al.
use crate::LamportClock;
use crate::interval_tree_clock::Event::N;
use std::cmp::{Ordering, PartialEq};
use std::rc::Rc;

macro_rules! rc {
    ($val:expr) => {
        Rc::new($val)
    };
}

#[repr(transparent)]
pub struct IntervalTreeClock {
    /// The underlying representation of an `IntervalTreeClock` is a `Stamp`, which encodes the
    /// state of the logical clock. While the two types are functionally equivalent, I'm separating
    /// them to maintain conceptual clarity (since I already know my brain will probably hurt when
    /// revisiting the code).
    ///
    /// Thanks to `#[repr(transparent)]`, this wrapper introduces no additional memory overhead,
    /// and the abstraction cost is limited to method dispatch.
    stamp: Stamp,
}

impl IntervalTreeClock {
    pub fn new() -> Self {
        Self {
            stamp: Stamp::seed(),
        }
    }

    /// A sync is the atomic composition of join followed by fork, e.g. in version vector systems
    /// and in bounded version vectors, it models the atomic synchronization of two replicas.
    pub fn sync(first: &Self, second: &Self) -> (Self, Self) {
        let (stamp1, stamp2) = first.stamp.join(&second.stamp).fork();
        (Self::from(stamp1), Self::from(stamp2))
    }

    /// Traditional descriptions of causal systems assume a starting number of participants, which
    /// can be simulated here by starting from an initial seed stamp and forking several times
    /// until the required number of participants is reached.
    pub fn fork(&self) -> (Self, Self) {
        let (stamp1, stamp2) = self.stamp.fork();
        (Self::from(stamp1), Self::from(stamp2))
    }

    fn bump(&mut self) {
        self.stamp = self.stamp.event();
    }

    fn send(&mut self) -> Self {
        IntervalTreeClock::bump(self);
        Self::from(self.stamp.peek())
    }

    fn receive(&mut self, incoming_clock: &Self) {
        self.stamp = self.stamp.join(&incoming_clock.stamp).event();
    }
}

impl LamportClock for IntervalTreeClock {
    fn bump(&mut self) {
        IntervalTreeClock::bump(self)
    }

    fn send(&mut self) -> Self {
        IntervalTreeClock::send(self)
    }

    fn receive(&mut self, incoming_clock: &Self) {
        IntervalTreeClock::receive(self, incoming_clock)
    }
}

impl From<Stamp> for IntervalTreeClock {
    fn from(stamp: Stamp) -> Self {
        IntervalTreeClock { stamp }
    }
}

impl PartialOrd for IntervalTreeClock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.stamp.leq(&other.stamp), other.stamp.leq(&self.stamp)) {
            (true, true) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Less),
            (false, true) => Some(Ordering::Greater),
            (false, false) => None,
        }
    }
}

impl PartialEq<Self> for IntervalTreeClock {
    fn eq(&self, other: &Self) -> bool {
        self.stamp.leq(&other.stamp) && other.stamp.leq(&self.stamp)
    }
}

/// A logical clock representation upon which a set of core operations (fork, event, join) models
/// a causality tracking mechanism.
struct Stamp {
    id: Id,
    event: Event,
}

impl Stamp {
    fn new(id: Id, event: Event) -> Self {
        Self { id, event }
    }

    fn new_from_rcs(id: &Rc<Id>, event: &Rc<Event>) -> Self {
        Self {
            id: id.as_ref().clone(),
            event: event.as_ref().clone(),
        }
    }

    /// Returns the *seed* stamp, (1,0), from which we can fork as desired to obtain an initial
    /// configuration. This represents full ownership over the entire domain [0,1).
    fn seed() -> Self {
        Self {
            id: Id::Full,
            event: N(0),
        }
    }

    fn fork(&self) -> (Self, Self) {
        let (left_id, right_id) = self.id.split();
        (
            Self::new(left_id, self.event.clone()),
            Self::new(right_id, self.event.clone()),
        )
    }

    /// A special case of fork when it is enough to obtain an anonymous stamp (0,e), with “null”
    /// identity, than can be used to transmit causal information but cannot register events,
    /// peek((i,e)) = ((0,e),(i,e)). Anonymous stamps are typically used to create messages or as
    /// inactive copies for later debugging of distributed executions.
    fn peek(&self) -> Self {
        Self::new(Id::Empty, self.event.clone())
    }

    /// An event operation adds a new event to the event component, so that if (i,e′) results from
    /// event((i,e)) the causal ordering is such that e < e′. This action does a strict advance in
    /// the partial order such that e′is not dominated by any other entity and does not dominate
    /// more events than needed: for any other event component xin the system, e′̸≤xand when x<e′
    /// then x≤e. In version vectors the event operation increments a counter associated to the
    /// identity in the stamp: ∀k ̸= i. e′[k] = e[k] and e′[i] = e[i] + 1.
    fn event(&self) -> Self {
        // Fill either succeeds in doing one or more simplifications, or returns an unmodified tree.
        fn fill(stamp: &Stamp) -> Event {
            match (&stamp.id, &stamp.event) {
                (Id::Empty, e) => e.clone(),
                (Id::Full, e) => N(e.max()),
                (_, n @ N(_)) => n.clone(),
                (Id::Split(i_left, i_right), Event::Split(n, e_left, e_right)) => {
                    if **i_left == Id::Full {
                        let e_right = rc!(fill(&Stamp::new_from_rcs(i_right, e_right)));
                        let maximus_prime = rc!(N(u32::max(e_left.max(), e_right.min())));

                        Event::Split(*n, maximus_prime, e_right).norm()
                    } else if **i_right == Id::Full {
                        let e_left = rc!(fill(&Stamp::new_from_rcs(i_left, e_left,)));
                        let maximus_prime = rc!(N(u32::max(e_right.max(), e_left.min())));

                        Event::Split(*n, e_left, maximus_prime).norm()
                    } else {
                        let stamp_left = Stamp::new_from_rcs(i_left, e_left);
                        let stamp_right = Stamp::new_from_rcs(i_right, e_right);
                        Event::Split(*n, rc!(fill(&stamp_left)), rc!(fill(&stamp_right))).norm()
                    }
                }
            }
        }
        // Performs a dynamic programming optimization to choose the inflation that, given the
        // available id tree, can be performed to minimize the cost of the event tree growth. It is
        // defined recursively, returning the new event tree and cost, so that
        // - incrementing an integer is preferable over expanding an integer to a tuple,
        // - to disambiguate, an operation near the root is preferable to one farther away.
        fn grow(stamp: &Stamp) -> (Event, u64) {
            // Surely, most realistic Interval Tree Clock trees stay well under depth 1000...
            const GREATER_THAN_MAX_TREE_DEPTH: u64 = 1000;

            match (&stamp.id, &stamp.event) {
                // Incrementing an integer has cost zero, as it's preferable to all other options.
                (Id::Full, N(n)) => (N(n + 1), 0),
                (i, N(n)) => {
                    let (e, cost) = grow(&Stamp::new(i.clone(), Event::split_from(n)));
                    (e, cost + GREATER_THAN_MAX_TREE_DEPTH)
                }
                // In the case of split events, choose whatever option has the lower cost.
                (Id::Split(i_left, i_right), Event::Split(n, e_left, e_right)) => {
                    if **i_left == Id::Empty {
                        let (e_right, cost) = grow(&Stamp::new_from_rcs(i_right, e_right));
                        (Event::Split(*n, Rc::clone(e_left), rc!(e_right)), cost + 1)
                    } else if **i_right == Id::Empty {
                        let (e_left, cost) = grow(&Stamp::new_from_rcs(i_left, e_left));
                        (Event::Split(*n, rc!(e_left), Rc::clone(e_right)), cost + 1)
                    } else {
                        let (e_left, e_right, cost) = {
                            let (new_e_left, cost_left) =
                                grow(&Stamp::new_from_rcs(i_left, e_left));
                            let (new_e_right, cost_right) =
                                grow(&Stamp::new_from_rcs(i_right, e_right));
                            // Choose the cheaper plan.
                            if cost_left < cost_right {
                                (rc!(new_e_left), Rc::clone(e_right), cost_left)
                            } else {
                                (Rc::clone(e_left), rc!(new_e_right), cost_right)
                            }
                        };
                        (Event::Split(*n, e_left, e_right), cost + 1)
                    }
                }
                (Id::Full, _) => unreachable!(
                    "Invariant violation: if we truly have full ownership \
                                                you should never end up with a Split event."
                ),
                (Id::Empty, _) => unreachable!("Cannot apply event to anonymous stamps!"),
            }
        }

        // Event cannot be applied to anonymous stamps; it has the precondition that the id is
        // non-null, i.e. i != 0.
        assert_ne!(self.id, Id::Empty);

        let new_event = {
            let filled_event = fill(self);
            if filled_event == self.event {
                filled_event
            } else {
                let (grown_event, _cost) = grow(self);
                grown_event
            }
        };
        Self::new(self.id.clone(), new_event)
    }

    /// This operation merges two stamps, producing a new one. If join((i1,e1),(i2,e2)) = (i3,e3),
    /// the resulting event component e3 should be such that e1 ≤e3 and e2 ≤e3. Also, e3 should not
    /// dominate 2 more that either e1 and e2 did. This is obtained by the order theoretical join,
    /// e3 = e1 ⊔ e2, that must be defined for all pairs; i.e. the order must form a join
    /// semi-lattice. In causal histories the join is defined by set union, and in version vectors
    /// it is obtained by the pointwise maximum of the two vectors. The identity should be based on
    /// the provided ones, i3 = f(i1,i2) and kept globally unique (except anonymous ids). In most
    /// systems this is obtained by keeping only one of the ids, but if ids are to be reused it
    /// should depend upon and incorporate both. When one stamp is anonymous, join can model message
    /// reception, where join((i,e1),(0,e2)) = (i,e1 ⊔e2). When both ids are defined, the join can
    /// be used to terminate an entity and collect its causal past. Also notice that joins can be
    /// applied when both stamps are anonymous, modeling in-transit aggregation of messages.
    fn join(&self, other: &Self) -> Self {
        Self::new(self.id.sum(&other.id), self.event.join(&other.event))
    }

    /// There can be several equivalent representations for a given function; in ITC we wish to
    /// keep stamps in *normal form* for the representations of both id and event functions, not
    /// only to have compact representations but also to allow simple definitions on stamps.
    fn norm(&self) -> Self {
        Self::new(self.id.norm(), self.event.norm())
    }

    /// Comparison of ITC can be derived from the point-wise comparison, which can be computed
    /// through a recursive function over normalized event trees; i.e. (i1, e1) <= (i2, e2) if, and
    /// only if, e1 <= e2.
    fn leq(&self, other: &Self) -> bool {
        self.event.leq(&other.event)
    }
}

#[derive(Clone, Debug)]
enum Id {
    /// No ownership over the id's interval domain.
    Empty,
    /// Full ownership over the id's interval domain.
    Full,
    /// Partitioned ownership over the id's interval domain `[a,b)`, where the first `Rc<Id>`
    /// represents ownership over `[a,(a+b)/2)` and the second represents ownership of `[(a+b)/2,b)`.
    Split(Rc<Id>, Rc<Id>),
}

impl Id {
    fn split(&self) -> (Self, Self) {
        use Id::{Empty, Full, Split};
        match self {
            // No identity: nothing to split.
            Empty => (Empty, Empty),
            // Base case: split full interval into halves.
            Full => (
                // [0, 0.5)
                Split(rc!(Full), rc!(Empty)),
                // [0.5, 1)
                Split(rc!(Empty), rc!(Full)),
            ),
            Split(l, r) => {
                // split((0,i)) = ((0, i1), (0, i2)), where (i1, i2) = split(i)
                if let Empty = *l.as_ref() {
                    let (i1, i2) = r.split();
                    (Split(rc!(Empty), rc!(i1)), Split(rc!(Empty), rc!(i2)))
                }
                // split((i, 0)) = ((i1, 0), (i2, 0)), where (i1, i2) = split(i)
                else if let Empty = *r.as_ref() {
                    let (i1, i2) = l.split();
                    (Split(rc!(i1), rc!(Empty)), Split(rc!(i2), rc!(Empty)))
                }
                // split((i1, i2)) = ((i1, 0), (0, i2))
                else {
                    (
                        Split(Rc::clone(l), rc!(Empty)),
                        Split(rc!(Empty), Rc::clone(r)),
                    )
                }
            }
        }
    }

    /// Respects the condition that [[sum(i1, i2)]] = [[i1]] + [[i2]] and produces a normalized id.
    fn sum(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, i) | (i, Self::Empty) => i.clone(),
            (Self::Split(l1, r1), Self::Split(l2, r2)) => {
                let (l1, l2, r1, r2) = (l1.as_ref(), l2.as_ref(), r1.as_ref(), r2.as_ref());
                Self::Split(rc!(l1.sum(l2)), rc!(r1.sum(r2))).norm()
            }
            _ => {
                // In cases of `(&Id::Full, &Id::Full)` and `(&Id::Full, &Id::Split(_, _))`,
                // will we ever see get to this point? Who knows...
                unreachable!()
            }
        }
    }

    /// Normalization of the id component can be obtained by recursively applying this function
    /// when building the id tree.
    fn norm(&self) -> Self {
        use Id::{Empty, Full, Split};

        if let Split(l, r) = &*self {
            if let (Empty, Empty) = (&**l, &**r) {
                return Empty;
            }
            if let (Full, Full) = (&**l, &**r) {
                return Full;
            }
        }
        self.clone()
    }
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Id::Empty, Id::Empty) => true,
            (Id::Empty, _) => false,
            (_, Id::Empty) => false,
            (Id::Full, Id::Full) => true,
            (Id::Full, _) => false,
            (_, Id::Full) => false,
            (Id::Split(l1, l2), Id::Split(r1, r2)) => l1 == r1 && l2 == r2,
        }
    }
}

#[derive(Clone)]
enum Event {
    /// Represents an element over its interval domain whose value is constant throughout.
    N(u32),
    /// Represents an element `(n, e1, e2)` over an interval `[a,b)` whose value is the sum of a
    /// base value `n`, common for the whole interval, plus a relative value from each corresponding
    /// subtree `e1` and `e2`, where `e1` represents an event over the sub-interval `[a,(a+b)/2)`
    /// `e2` represents an event over `[(a+b)/2, b)`.
    Split(u32, Rc<Event>, Rc<Event>),
}

impl PartialEq<Event> for Event {
    fn eq(&self, other: &Event) -> bool {
        match (self, other) {
            (N(n1), N(n2)) => n1 == n2,
            (N(_), _) => false,
            (_, N(_)) => false,
            (Self::Split(n1, e1_left, e1_right), Self::Split(n2, e2_left, e2_right)) => {
                n1 == n2
                    && e1_left.as_ref() == e2_left.as_ref()
                    && e1_right.as_ref() == e2_right.as_ref()
            }
        }
    }
}

impl Event {
    #[inline]
    fn split_from(n: &u32) -> Event {
        Event::Split(*n, Rc::new(N(0)), Rc::new(N(0)))
    }

    fn join(&self, other: &Self) -> Self {
        use Event::*;

        match (self, other) {
            (N(n1), N(n2)) => N(if n1 > n2 { *n1 } else { *n2 }),
            (N(n1), Split(_, _, _)) => Event::split_from(n1).join(other),
            (Split(_, _, _), N(n2)) => self.join(&Event::split_from(n2)),
            (Split(n1, l1, r1), Split(n2, l2, r2)) => {
                if n1 > n2 {
                    other.join(self)
                } else {
                    let n = n2 - n1;
                    let (left, right) = (l1.join(&l2.lift(n)), r1.join(&r2.lift(n)));
                    Split(*n1, rc!(left), rc!(right)).norm()
                }
            }
        }
    }

    fn norm(&self) -> Self {
        match self {
            N(n) => N(*n),
            Event::Split(n, e1, e2) => {
                let (e1, e2) = (e1.as_ref(), e2.as_ref());

                // norm((n,m,m)) = n + m
                if let N(m1) = e1 {
                    if let N(m2) = e2 {
                        if m1 == m2 {
                            return N(*n + m1);
                        }
                    }
                }

                // norm((n, e1, e2)) = (n+m, e1.sink(m), e2.sink(m)), where m = min(min(e1), min(e2)).
                let m = u32::min(e1.min(), e2.min());
                Self::Split(n + m, rc!(e1.sink(m)), rc!(e2.sink(m)))
            }
        }
    }

    fn lift(&self, m: u32) -> Self {
        match self {
            N(n) => N(n + m),
            Self::Split(n, e1, e2) => Self::Split(n + m, Rc::clone(e1), Rc::clone(e2)),
        }
    }

    fn sink(&self, m: u32) -> Self {
        match self {
            N(n) => N(n - m),
            Self::Split(n, e1, e2) => Self::Split(n - m, Rc::clone(e1), Rc::clone(e2)),
        }
    }

    /// Returns the minimum value of the function, corresponding to the given tree, in the range
    /// `[0,1): min(e) = min { [[e]](x) | x in [0,1) }`.
    fn min(&self) -> u32 {
        match self {
            N(n) => *n,
            Event::Split(n, e1, e2) => n + u32::min(e1.min(), e2.min()),
        }
    }

    /// See Event::min() above.
    fn max(&self) -> u32 {
        match self {
            N(n) => *n,
            Event::Split(n, e1, e2) => n + u32::max(e1.max(), e2.max()),
        }
    }

    /// We define leq(e1, e2) as follows:
    /// - leq(n1, n2)                      = n1 <= n2
    /// - leq(n1, (n2, l2, r2))            = n1 <= n2
    /// - leq((n1, l1, r1), n2)            = n1 <= n2 AND leq(l1.lift(n1), n2)
    ///                                               AND leq(r1.lift(n1), n2)
    /// - leq((n1, l1, r1), (n2, l2, r2))  = n1 <= n2 AND leq(l1.lift(n1), l2.lift(n2))
    ///                                               AND leq(r1.lift(n1), r2.lift(n2))
    fn leq(&self, other: &Self) -> bool {
        use Event::{N, Split};

        match (self, other) {
            (N(n1), N(n2)) => n1 <= n2,
            (N(n1), Split(n2, _, _)) => n1 <= n2,
            (Split(n1, l1, r1), N(n2)) => {
                let (l1, r1) = (l1.as_ref(), r1.as_ref());
                n1 <= n2 && l1.lift(*n1).leq(other) && r1.lift(*n1).leq(other)
            }
            (Split(n1, l1, r1), Split(n2, l2, r2)) => {
                let (l1, l2, r1, r2) = (l1.as_ref(), l2.as_ref(), r1.as_ref(), r2.as_ref());
                n1 <= n2 && l1.lift(*n1).leq(&l2.lift(*n2)) && r1.lift(*n1).leq(&r2.lift(*n2))
            }
        }
    }
}
