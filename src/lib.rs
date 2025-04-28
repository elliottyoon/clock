/// The (Lamport) Clock Condition gives that if `a` happens before `b` (denoted `a -> b`), then
/// `TS(a) < TS(b)`. Vector clocks guarantee a stronger condition: `a -> b` <=> `TS(a) < TS(b)`.
pub mod vector_clock;

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
