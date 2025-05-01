// fork:  The fork operation allows the cloning of the causal past of a stamp, resulting in a pair of stamps that
//        have identical copies of the event component and distinct ids; fork(i,e) = ((i1,e),(i2,e)) such that
//        i2 ̸= i1. Typically, i= i1 and i2 is a new id. In some systems i2 is obtained from an external source
//        of unique ids, e.g. MAC addresses. In contrast, in Bayou [18] i2 is a function of the original stamp
//        f((i,e)); consecutive forks are assigned distinct ids since an event is issued to increment a counter
//        after each fork.
// peek:  A special case of fork when it is enough to obtain an anonymous stamp (0,e), with “null” identity,
//        than can be used to transmit causal information but cannot register events, peek((i,e)) =
//        ((0,e),(i,e)). Anonymous stamps are typically used to create messages or as inactive copies
//        for later debugging of distributed executions.
// event: An event operation adds a new event to the event component, so that if (i,e′) results from event((i,e))
//        the causal ordering is such that e < e′. This action does a strict advance in the partial order such
//        that e′is not dominated by any other entity and does not dominate more events than needed: for any
//        other event component xin the system, e′̸≤xand when x<e′then x≤e. In version vectors the
//        event operation increments a counter associated to the identity in the stamp: ∀k ̸= i. e′[k] = e[k]
//        and e′[i] = e[i] + 1.
// join:  This operation merges two stamps, producing a new one. If join((i1,e1),(i2,e2)) = (i3,e3), the
//        resulting event component e3 should be such that e1 ≤e3 and e2 ≤e3. Also, e3 should not dominate
//        2 more that either e1 and e2 did. This is obtained by the order theoretical join, e3 = e1 ⊔e2, that
//        must be defined for all pairs; i.e. the order must form a join semilattice. In causal histories the join
//        is defined by set union, and in version vectors it is obtained by the pointwise maximum of the two
//        vectors.
//        The identity should be based on the provided ones, i3 = f(i1,i2) and kept globally unique (with the
//        exception of anonymous ids). In most systems this is obtained by keeping only one of the ids, but if
//        ids are to be reused it should depend upon and incorporate both [2].
//        When one stamp is anonymous, join can model message reception, where join((i,e1),(0,e2)) =
//        (i,e1 ⊔e2). When both ids are defined, the join can be used to terminate an entity and collect
//        its causal past. Also notice that joins can be applied when both stamps are anonymous, modeling
//        in-transit aggregation of messages.
//
// Classic operations can be described as a composition of these core operations:
//
// send:    This operation is the atomic composition of event followed by peek. E.g. in vector clock systems,
//          message sending is modeled by incrementing the local counter and then creating a new message.
// receive: A receive is the atomic composition of join followed by event. E.g. in vector clocks taking the
//          pointwise maximum is followed by an increment of the local counter.
// sync:    A sync is the atomic composition of join followed by fork. E.g. In version vector systems and in
//          bounded version vectors [1] it models the atomic synchronization of two replicas.
//          Traditional descriptions assume a starting number of participants. This can be simulated by starting
//          from an initial seed stamp and forking several times until the required number of participants is reached.
