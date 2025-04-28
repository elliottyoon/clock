/// The (Lamport) Clock Condition gives that if `a` happens before `b` (denoted `a -> b`), then
/// `TS(a) < TS(b)`. Vector clocks guarantee a stronger condition: `a -> b` <=> `TS(a) < TS(b)`.
pub mod vector_clock;

/// HybridTime (HT) exploits the clock synchronization assumption of PT clocks to trim entries from
/// VC and reduces the overhead of causality tracking. In practice the size of HT at a node would
/// only depend on the number of nodes that communicated with that node within the last ϵ time,
/// where ϵ denotes the clock synchronization uncertainty. Of importance to note is that hybrid
/// time clocks preserve the Clock Condition, i.e. `a -> b` => `TS(a) < TS(b)`; and is backwards
/// -compatible with NTC.
pub mod hybrid_time_clock;

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
