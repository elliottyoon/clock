pub trait LamportClock: PartialOrd {
    /// Updates this clock for when its respective process executes a local event.
    fn bump(&mut self);

    /// Signifies a process sending a message to another process, updating the clock's state and
    /// producing an owned copy of the clock, which can be used by the receiving process to update
    /// its own respective clock.
    fn send(&mut self) -> Self;

    /// Signifies a process receiving a message from another process, updating the clock's state
    /// with the sender's clock that is piggybacked onto the received message.
    fn receive(&mut self, incoming_clock: &Self);
}

/// TODO(elliottyoon): VersionVectors
mod version_vector;

/// The (Lamport) Clock Condition gives that if `a` happens before `b` (denoted `a -> b`), then
/// `TS(a) < TS(b)`. Vector clocks guarantee a stronger condition: `a -> b` <=> `TS(a) < TS(b)`.
pub mod vector_clock;

/// Hybrid logical time clocks preserve the Clock Condition, i.e. `a -> b` => `TS(a) < TS(b)`; and
/// are backwards-compatible with NTC. An HLC can be represented as a 64-bit float! Very cool.
pub mod hybrid_logical_clock;

/// Provides causality tracking in dynamic settings, e.g. peer-to-peer systems. Generalizes vector
/// clocks and version vectors to a clock whose space requirement scales reasonably with the
/// number of entities and grows modestly over time.
mod interval_tree_clock;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_logical_clock::HybridLogicalClock;
    use crate::vector_clock::VectorClock;

    #[test]
    fn they_are_lamport_clocks() {
        fn assert_impl<T: LamportClock>() {}
        // Will fail to compile if the given types don't implement the LamportClock trait.
        assert_impl::<VectorClock>();
        assert_impl::<HybridLogicalClock>();
    }
}
