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

/// The (Lamport) Clock Condition gives that if `a` happens before `b` (denoted `a -> b`), then
/// `TS(a) < TS(b)`. Vector clocks guarantee a stronger condition: `a -> b` <=> `TS(a) < TS(b)`.
pub mod vector_clock;

/// HybridTime (HT) exploits the clock synchronization assumption of PT clocks to trim entries from
/// VC and reduces the overhead of causality tracking. In practice the size of HT at a node would
/// only depend on the number of nodes that communicated with that node within the last ϵ time,
/// where ϵ denotes the clock synchronization uncertainty. Of importance to note is that hybrid
/// time clocks preserve the Clock Condition, i.e. `a -> b` => `TS(a) < TS(b)`; and is backwards
/// -compatible with NTC.
pub mod hybrid_logical_clock;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
