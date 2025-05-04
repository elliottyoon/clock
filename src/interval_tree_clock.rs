//! Traditional logical clocks require a fixed set of participants (e.g. vector clocks), each with
//! a unique, pre-defined identity. Interval tree clocks (ITCs) eliminate this restriction by
//! encoding identity as a function over the half-open interval [0, 1), allowing for dynamic growth
//! and shrinkage of participants and automatic partitioning of identity space.
//!
//! Each ITC stamp is an (id, event) pair, where
//! - id:    binary tree that describes which sub-intervals of [0,1) a process controls, and
//! - event: another binary tree mapping sub-intervals to non-negative integers that represent the
//!          logical time (i.e. how many events occurred).

use std::rc::Rc;

/// Define "unit pulse function", 1 : |R -> {0, 1}:
/// 1'(x) := { 1 if 0 <= x < 1;
///            0 if x < 0 or x >= 1 }
///
/// Define an "id tree" with recursive form (where i, i1, i2 range over id trees):
/// i := 0 | 1 | (i1, i2)
///
/// Define a semantic function [] for the interpretation of id trees:
/// [0]        = 0' : |R -> {0, 1}
/// [1]        = 1' : |R -> {0, 1}
/// [(i1, i2)] = λ(x): [i1](2x) + [i2](2x-1)
/// These functions can be 1 for some sub-intervals of [0, 1) and 0 otherwise. For an id (i1, i2),
/// the functions corresponding to the two subtrees are transformed to be non-zero in two
/// non-overlapping sub-intervals: i1 in the interval [0, 1/2) and i2 in [1/2, 1). For example, the
/// id (1, (0,1)) represents the function 1'(2x) + 1'(2x-1)(2x-1).
///
/// The event component is a binary event tree with non-negative integers in nodes: using e, e1, e2
/// to range over event trees and n over non-negative integers:
/// e := n | (n, e1, e2)
///
/// Define a semantic function for the interpretation of these trees as functions:
/// [n]           = n * 1'
/// [(n, e1, e2)] = n * 1' + (λ(x): [e1](2x) [e2](2x-1))
/// This means the value for an element in some sub-interval is the sum of a base value, common for
/// the whole interval plus a relative value from the corresponding subtree.
#[repr(transparent)]
pub struct IntervalTreeClock {
    stamp: Stamp,
}

enum Id {
    /// No ownership over the id's interval domain.
    None,
    /// Full ownership over the id's interval domain.
    All,
    /// Partitioned ownership over the id's interval domain `[a,b)`, where the first `Rc<Id>`
    /// represents ownership over `[a,(a+b)/2)` and the second represents ownership of `[(a+b)/2,b)`.
    Split(Rc<Id>, Rc<Id>),
}

enum Event {
    /// Represents an element over its interval domain whose value is constant throughout.
    N(u32),
    /// Represents an element `(n, e1, e2)` over an interval `[a,b)` whose value is the sum of a
    /// base value `n`, common for the whole interval, plus a relative value from each corresponding
    /// subtree `e1` and `e2`, where `e1` represents an event over the sub-interval `[a,(a+b)/2)`
    /// `e2` represents an event over `[(a+b)/2, b)`.
    Split(u32, Rc<Event>, Rc<Event>),
}

/// A logical clock representation upon which a set of core operations (fork, event, join) models
/// a causality tracking mechanism.
struct Stamp {
    id: Id,
    event: Event,
}

// Classic operations can be described as a composition of these core operations:
//
// send:    This operation is the atomic composition of event followed by peek. E.g. in vector clock systems,
//          message sending is modeled by incrementing the local counter and then creating a new message.
// receive: A receive is the atomic composition of join followed by event. E.g. in vector clocks taking the
//          point-wise maximum is followed by an increment of the local counter.
// sync:    A sync is the atomic composition of join followed by fork. E.g. In version vector systems and in
//          bounded version vectors [1] it models the atomic synchronization of two replicas.
//          Traditional descriptions assume a starting number of participants. This can be simulated by starting
//          from an initial seed stamp and forking several times until the required number of participants is reached.
impl Stamp {
    /// Returns the *seed* stamp, (1,0), from which we can fork as desired to obtain an initial
    /// configuration. This represents full ownership over the entire domain [0,1).
    fn seed() -> Self {
        Self {
            id: Id::All,
            event: Event::N(0),
        }
    }

    // The fork operation allows the cloning of the causal past of a stamp, resulting in a pair of
    // stamps that have identical copies of the event component and distinct ids; fork(i,e) =
    // ((i1,e),(i2,e)) such that i2 ̸= i1. Typically, i= i1 and i2 is a new id. In some systems i2
    // is obtained from an external source of unique ids, e.g. MAC addresses. In contrast, in Bayou
    // i2 is a function of the original stamp f((i,e)); consecutive forks are assigned distinct ids
    // since an event is issued to increment a counter after each fork.
    fn fork() {}
    /// A special case of fork when it is enough to obtain an anonymous stamp (0,e), with “null”
    /// identity, than can be used to transmit causal information but cannot register events,
    /// peek((i,e)) = ((0,e),(i,e)). Anonymous stamps are typically used to create messages or as
    /// inactive copies for later debugging of distributed executions.
    fn peek() {}
    /// An event operation adds a new event to the event component, so that if (i,e′) results from
    /// event((i,e)) the causal ordering is such that e < e′. This action does a strict advance in
    /// the partial order such that e′is not dominated by any other entity and does not dominate
    /// more events than needed: for any other event component xin the system, e′̸≤xand when x<e′
    /// then x≤e. In version vectors the event operation increments a counter associated to the
    /// identity in the stamp: ∀k ̸= i. e′[k] = e[k] and e′[i] = e[i] + 1.
    fn event() {}
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
    fn join() {}
}
