use crate::LamportClock;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Add;

#[derive(Debug, Clone)]
pub struct VectorClock<K = usize, V = usize>
where
    K: Eq + std::hash::Hash + Clone,
    V: Add<V, Output = V> + From<u8> + Ord + Default + Clone,
{
    /// Assume there are N processes in the system, all of whom have their own respective vector
    /// clock (say `VC_i` for each process i in {1, ..., N}). Then each clock, `VC_i`, will have
    /// an underlying list of size N, `V_i` such that:
    /// - `V_i[i]` is the number of events that have taken place at process `i`,
    /// - `V_i[j]` is the number of events that process `i` **knows** to have taken place at
    ///    process `j`, (i.e. that have potentially affected process `i`).
    ///
    /// Comparing vector timestamps `U` and `V`, we say
    /// - `U == V` if, and only if, `U[i] == V[i]` for each `i` in {1, ..., N},
    /// - `U < V` if, and only if, `U[i] <= V[i]` for each `i` in {1, ..., N} _and_ there exists
    ///           some `j` such that `U[j] < V[j]`, and
    /// - `U || V` (are **concurrent**) if neither `U < V` nor `V < U`, i.e. with respect to the
    ///   notion of partial ordering, we'd say `U` and `V` are **not comparable**.
    ///
    /// _(Note that we're conflating a process `p_i` with its index `i` in {1, ..., N}. In practice,
    /// we could have a mapping between the process's index `i` in the vector and its process id.)_
    clock: HashMap<K, V>,
    /// The key (in the list `clock`) of the process who owns this vector clock struct, i.e.
    /// process `i` would have vector clock `VC_i` from the above description.
    i: K,
}

impl LamportClock for VectorClock {
    fn bump(&mut self) {
        VectorClock::bump(self);
    }

    fn send(&mut self) -> Self {
        VectorClock::send(self)
    }

    fn receive(&mut self, incoming_clock: &Self) {
        VectorClock::receive(self, incoming_clock);
    }
}

impl<K, V> VectorClock<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Add<V, Output = V> + From<u8> + Ord + Default + Clone,
{
    /// Constructs a new vector clock for the given process identifier.
    pub fn new(i: K) -> VectorClock<K, V> {
        Self {
            clock: HashMap::new(),
            i,
        }
    }

    /// Increments the owning process's corresponding value in the vector clock.
    pub fn bump(&mut self) {
        let value = self.clock.get(&self.i).unwrap_or(&V::default()).clone() + V::from(1);
        self.clock.insert(self.i.clone(), value);
    }

    /// Returns whether this vector clock represents a state that is causal to the state that is
    /// represented by the incoming vector clock.
    ///
    /// If events `x` and `y` occurred at respective processes `i` and `j` who have corresponding
    /// vector clocks `V_i` and `V_j`, then `x -> y` if, and only if, `V_i[i] < V_j[i]`; otherwise,
    /// `x || y` (see [`VectorClock::is_concurrent_with`]).
    #[inline]
    pub fn happens_before(&self, other: &Self) -> bool {
        self < other
    }

    /// Returns whether this vector clock represents a state that is concurrent with the incoming
    /// vector clock (see documentation for [`VectorClock::happens_before`] for more detail).
    #[inline]
    pub fn is_concurrent_with(&self, other: &Self) -> bool {
        !self.happens_before(other)
    }

    /// When a process intends on sending a message to another process, prepares the sending
    /// process's vector clock to be piggybacked along with the message.
    ///
    /// Upholds the vector clock invariant that the value corresponding to the sending process in
    /// the sending process's vector clock must be incremented before being sent.
    #[inline]
    pub fn send(&mut self) -> Self {
        self.bump();
        self.clone()
    }

    /// When a process receives a message from another process, maintains the vector clock
    /// invariant that both:
    /// - each entry of the receiving process's vector clock must be updated to be the max value
    ///   between it and the corresponding value of the incoming message's vector clock, and
    /// - after the merge described above, the value corresponding to the receiving process in the
    ///   receiving process's vector clock must be incremented.
    #[inline]
    pub fn receive(&mut self, incoming_clock: &Self) {
        self.merge(incoming_clock);
        self.bump();
    }

    /// Fetches the clock's value for a given key, if such an entry exists. Otherwise, returns the
    /// default value.
    fn get(&self, key: &K) -> V {
        match self.clock.get(key) {
            Some(value) => value.clone(),
            None => V::default(),
        }
    }

    /// Merges this vector clock, in place, with the incoming one, taking each merged entry to be
    /// the maximum between the two entries.
    fn merge(&mut self, other: &VectorClock<K, V>) {
        for (k, other_v) in other.clock.iter() {
            // Only overwrite/insert a `key`/`value` pair from other into self if `value` is
            // greater than what we currently have in self corresponding to `key`.
            if match self.clock.get(k) {
                Some(self_v) => self_v < other_v,
                None => true,
            } {
                self.clock.insert(k.clone(), other_v.clone());
            }
        }
    }
}

impl<K, V> PartialEq<Self> for VectorClock<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Add<V, Output = V> + From<u8> + Ord + Default + Clone,
{
    fn eq(&self, other: &Self) -> bool {
        // Returns if for every value in the left clock, the corresponding key's value in the right
        // clock (or the default value, if the key doesn't exist) is equal to it. You can think of
        // this as subset equality, returning if `left` is a subset of `right`.
        #[inline]
        fn subset_eq<K, V>(left: &VectorClock<K, V>, right: &VectorClock<K, V>) -> bool
        where
            K: Eq + std::hash::Hash + Clone,
            V: Add<Output = V> + From<u8> + Ord + Default + Clone + PartialEq,
        {
            for (k, v) in left.clock.iter() {
                match right.clock.get(k) {
                    Some(v2) => {
                        if v2.ne(v) {
                            return false;
                        }
                    }
                    // We can think of non-existent values as being equal to the default value.
                    None => {
                        if V::default().ne(v) {
                            return false;
                        }
                    }
                }
            }
            true
        }

        if self.clock == other.clock {
            return true;
        }
        // A == B if, and only if, both (1) A is a subset of B, and (2) B is a subset of A.
        subset_eq(self, &other) && subset_eq(&other, self)
    }
}

impl<K, V> PartialOrd for VectorClock<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Add<V, Output = V> + From<u8> + Ord + Default + Clone,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        struct HasCmp {
            has_greater: bool,
            has_less: bool,
        }
        #[inline]
        fn subset_cmp<K, V>(left: &VectorClock<K, V>, right: &VectorClock<K, V>) -> HasCmp
        where
            K: Eq + std::hash::Hash + Clone,
            V: Add<Output = V> + From<u8> + Ord + Default + Clone + PartialEq,
        {
            let mut has_greater = false;
            let mut has_less = false;

            for (k, left_v) in left.clock.iter() {
                match right.clock.get(k) {
                    Some(right_v) => match left_v.partial_cmp(right_v) {
                        Some(Ordering::Greater) => {
                            has_greater = true;
                        }
                        Some(Ordering::Less) => {
                            has_less = true;
                        }
                        Some(Ordering::Equal) => {
                            // no-op
                        }
                        None => unreachable!(),
                    },
                    None => match V::default().cmp(left_v) {
                        Ordering::Less => has_greater = true,
                        Ordering::Equal => {
                            // no-op
                        }
                        Ordering::Greater => {
                            unreachable!("Default value should be the minimum possible value.")
                        }
                    },
                }
            }

            HasCmp {
                has_greater,
                has_less,
            }
        }

        let (has_greater, has_less) = {
            let HasCmp {
                has_greater: self_has_greater,
                has_less: _self_has_less,
                ..
            } = subset_cmp(self, other);
            let HasCmp {
                has_greater: other_has_greater,
                has_less: _other_has_less,
                ..
            } = subset_cmp(other, self);

            // Note that even if self_has_less is false, there might be non-default values in
            // `other` that are greater than the corresponding value in `self`. Thus, we must
            // necessarily check if `other_has_greater` is true to determine if `self` has a
            // value that is less than `other`'s corresponding one.
            //
            // However, observe that if clock A doesn't have any non-default values that are
            // greater than the corresponding values of B, then any non-default values of B
            // should definitely not be less than any values of A (at the most, they'll be
            // equal). Thus, we have that `other_has_greater => self_has_less` so it's actually
            // sufficient to check only that `other_has_greater` is true to determine that there
            // is a value in `self` that is less than the corresponding one in `other`.
            (self_has_greater, other_has_greater)
        };

        match (has_greater, has_less) {
            // V[i] >= V'[i] for all i, and there exists some j such that V[j] > V'[j] => V > V'
            (true, false) => Some(Ordering::Greater),
            // V[i] <= V'[i] for all i, and there exists some j such that V[j] < V'[j] => V < V'
            (false, true) => Some(Ordering::Less),
            // V[i] = V'[i] for all i => V = V'
            (false, false) => Some(Ordering::Equal),
            // Non-comparable, i.e. concurrent clocks!
            (true, true) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::vector_clock::VectorClock;

    #[test]
    fn test_causality() {
        let (p1, p2, p3) = (1, 2, 3);
        let [mut vc1, mut vc2, mut vc3] = [p1, p2, p3].map(VectorClock::<usize, usize>::new);

        // - Process 1 will (1) bump, (2) send a message to p2, (3), bump, (4) receive a message
        //   from p3, and (5) send a message to p2.
        // - Process 2 will (1) bump, (2) receive a message from p1, (3) send a message to p3,
        //   (4) bump, (5) receive a message from p1, and (6) bump.
        // - Process 3 will (1) bump, (2) receive a message from p2, (3) bump, and (4) send a
        //   message to p1.
        //
        // We should have 4 causal events: (1.2/2.2), (2.3/3.2), (3.4/1.4), and (1.5/2.5).

        // (2.1)
        vc2.bump();
        // (1.1)
        vc1.bump();
        // (1.2 / 2.2)
        let sending_clock = vc1.send();
        vc2.receive(&sending_clock);
        // (3.1)
        vc3.bump();

        assert!(vc1.happens_before(&vc2));
        assert!(vc3.is_concurrent_with(&vc1));
        assert!(vc3.is_concurrent_with(&vc2));

        // (1.3)
        vc1.bump();
        // (2.3 / 3.2)
        let sending_clock = vc2.send();
        vc3.receive(&sending_clock);

        assert!(vc2.happens_before(&vc3));
        assert!(vc1.is_concurrent_with(&vc2));
        assert!(vc1.is_concurrent_with(&vc3));

        // (2.4)
        vc2.bump();
        // (3.3)
        vc3.bump();
        // (3.4 / 1.4)
        let sending_clock = vc3.send();
        vc1.receive(&sending_clock);

        assert!(vc3.happens_before(&vc1));
        assert!(vc2.is_concurrent_with(&vc1));
        assert!(vc1.is_concurrent_with(&vc3));

        // (1.5 / 2.5)
        let sending_clock = vc1.send();
        vc2.receive(&sending_clock);

        assert!(vc1.happens_before(&vc2));
        // p3 is "before" p1 because of (3.4/1.4).
        assert!(!vc3.is_concurrent_with(&vc1));
        // p3 is "after" p2 because of (2.3/3.2).
        assert!(!vc3.is_concurrent_with(&vc2));

        // (2.6)
        vc2.bump();
    }
}
