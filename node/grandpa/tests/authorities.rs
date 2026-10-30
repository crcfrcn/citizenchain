use super::*;
use sp_core::crypto::{ByteArray, UncheckedFrom};

fn static_is_descendent_of<A>(value: bool) -> impl Fn(&A, &A) -> Result<bool, std::io::Error> {
    move |_, _| Ok(value)
}

fn is_descendent_of<A, F>(f: F) -> impl Fn(&A, &A) -> Result<bool, std::io::Error>
where
    F: Fn(&A, &A) -> bool,
{
    move |base, hash| Ok(f(base, hash))
}

#[test]
fn current_limit_filters_min() {
    let current_authorities = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 1)];

    let mut authorities = AuthoritySet {
        current_authorities: current_authorities.clone(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let change = |height| PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 0,
        canon_height: height,
        canon_hash: height.to_string(),
        delay_kind: DelayKind::Finalized,
    };

    let is_descendent_of = static_is_descendent_of(false);

    authorities
        .add_pending_change(change(1), &is_descendent_of)
        .unwrap();
    authorities
        .add_pending_change(change(2), &is_descendent_of)
        .unwrap();

    assert_eq!(authorities.current_limit(0), Some(1));

    assert_eq!(authorities.current_limit(1), Some(1));

    assert_eq!(authorities.current_limit(2), Some(2));

    assert_eq!(authorities.current_limit(3), None);
}

#[test]
fn changes_iterated_in_pre_order() {
    let current_authorities = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 1)];

    let mut authorities = AuthoritySet {
        current_authorities: current_authorities.clone(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let change_a = PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Finalized,
    };

    let change_b = PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 0,
        canon_height: 5,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Finalized,
    };

    let change_c = PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 5,
        canon_height: 10,
        canon_hash: "hash_c",
        delay_kind: DelayKind::Finalized,
    };

    authorities
        .add_pending_change(change_a.clone(), &static_is_descendent_of(false))
        .unwrap();
    authorities
        .add_pending_change(change_b.clone(), &static_is_descendent_of(false))
        .unwrap();
    authorities
        .add_pending_change(
            change_c.clone(),
            &is_descendent_of(|base, hash| match (*base, *hash) {
                ("hash_a", "hash_c") => true,
                ("hash_b", "hash_c") => false,
                _ => unreachable!(),
            }),
        )
        .unwrap();

    // forced changes are iterated last
    let change_d = PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 2,
        canon_height: 1,
        canon_hash: "hash_d",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    let change_e = PendingChange {
        next_authorities: current_authorities.clone(),
        delay: 2,
        canon_height: 0,
        canon_hash: "hash_e",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    authorities
        .add_pending_change(change_d.clone(), &static_is_descendent_of(false))
        .unwrap();
    authorities
        .add_pending_change(change_e.clone(), &static_is_descendent_of(false))
        .unwrap();

    // ordered by subtree depth
    assert_eq!(
        authorities.pending_changes().collect::<Vec<_>>(),
        vec![&change_a, &change_c, &change_b, &change_e, &change_d],
    );
}

#[test]
fn apply_change() {
    let mut authorities = AuthoritySet {
        current_authorities: Vec::new(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 5)];
    let set_b = vec![(AuthorityId::from_slice(&[2; 32]).unwrap(), 5)];

    // two competing changes at the same height on different forks
    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Finalized,
    };

    let change_b = PendingChange {
        next_authorities: set_b.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Finalized,
    };

    authorities
        .add_pending_change(change_a.clone(), &static_is_descendent_of(true))
        .unwrap();
    authorities
        .add_pending_change(change_b.clone(), &static_is_descendent_of(true))
        .unwrap();

    assert_eq!(
        authorities.pending_changes().collect::<Vec<_>>(),
        vec![&change_a, &change_b]
    );

    // finalizing "hash_c" won't enact the change signaled at "hash_a" but it will prune out
    // "hash_b"
    let status = authorities
        .apply_standard_changes(
            "hash_c",
            11,
            &is_descendent_of(|base, hash| match (*base, *hash) {
                ("hash_a", "hash_c") => true,
                ("hash_b", "hash_c") => false,
                _ => unreachable!(),
            }),
            false,
            None,
        )
        .unwrap();

    assert!(status.changed);
    assert_eq!(status.new_set_block, None);
    assert_eq!(
        authorities.pending_changes().collect::<Vec<_>>(),
        vec![&change_a]
    );
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges::empty()
    );

    // finalizing "hash_d" will enact the change signaled at "hash_a"
    let status = authorities
        .apply_standard_changes(
            "hash_d",
            15,
            &is_descendent_of(|base, hash| match (*base, *hash) {
                ("hash_a", "hash_d") => true,
                _ => unreachable!(),
            }),
            false,
            None,
        )
        .unwrap();

    assert!(status.changed);
    assert_eq!(status.new_set_block, Some(("hash_d", 15)));

    assert_eq!(authorities.current_authorities, set_a);
    assert_eq!(authorities.set_id, 1);
    assert_eq!(authorities.pending_changes().count(), 0);
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15)])
    );
}

#[test]
fn disallow_multiple_changes_being_finalized_at_once() {
    let mut authorities = AuthoritySet {
        current_authorities: Vec::new(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 5)];
    let set_c = vec![(AuthorityId::from_slice(&[2; 32]).unwrap(), 5)];

    // two competing changes at the same height on different forks
    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Finalized,
    };

    let change_c = PendingChange {
        next_authorities: set_c.clone(),
        delay: 10,
        canon_height: 30,
        canon_hash: "hash_c",
        delay_kind: DelayKind::Finalized,
    };

    authorities
        .add_pending_change(change_a.clone(), &static_is_descendent_of(true))
        .unwrap();
    authorities
        .add_pending_change(change_c.clone(), &static_is_descendent_of(true))
        .unwrap();

    let is_descendent_of = is_descendent_of(|base, hash| match (*base, *hash) {
        ("hash_a", "hash_b") => true,
        ("hash_a", "hash_c") => true,
        ("hash_a", "hash_d") => true,

        ("hash_c", "hash_b") => false,
        ("hash_c", "hash_d") => true,

        ("hash_b", "hash_c") => true,
        _ => unreachable!(),
    });

    // trying to finalize past `change_c` without finalizing `change_a` first
    assert!(matches!(
        authorities.apply_standard_changes("hash_d", 40, &is_descendent_of, false, None),
        Err(Error::ForkTree(fork_tree::Error::UnfinalizedAncestor))
    ));
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges::empty()
    );

    let status = authorities
        .apply_standard_changes("hash_b", 15, &is_descendent_of, false, None)
        .unwrap();

    assert!(status.changed);
    assert_eq!(status.new_set_block, Some(("hash_b", 15)));

    assert_eq!(authorities.current_authorities, set_a);
    assert_eq!(authorities.set_id, 1);
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15)])
    );

    // after finalizing `change_a` it should be possible to finalize `change_c`
    let status = authorities
        .apply_standard_changes("hash_d", 40, &is_descendent_of, false, None)
        .unwrap();

    assert!(status.changed);
    assert_eq!(status.new_set_block, Some(("hash_d", 40)));

    assert_eq!(authorities.current_authorities, set_c);
    assert_eq!(authorities.set_id, 2);
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15), (1, 40)])
    );
}

#[test]
fn enacts_standard_change_works() {
    let mut authorities = AuthoritySet {
        current_authorities: Vec::new(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 5)];

    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Finalized,
    };

    let change_b = PendingChange {
        next_authorities: set_a.clone(),
        delay: 10,
        canon_height: 20,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Finalized,
    };

    authorities
        .add_pending_change(change_a.clone(), &static_is_descendent_of(false))
        .unwrap();
    authorities
        .add_pending_change(change_b.clone(), &static_is_descendent_of(true))
        .unwrap();

    let is_descendent_of = is_descendent_of(|base, hash| match (*base, *hash) {
        ("hash_a", "hash_d") => true,
        ("hash_a", "hash_e") => true,
        ("hash_b", "hash_d") => true,
        ("hash_b", "hash_e") => true,
        ("hash_a", "hash_c") => false,
        ("hash_b", "hash_c") => false,
        _ => unreachable!(),
    });

    // "hash_c" won't finalize the existing change since it isn't a descendent
    assert_eq!(
        authorities
            .enacts_standard_change("hash_c", 15, &is_descendent_of)
            .unwrap(),
        None,
    );

    // "hash_d" at depth 14 won't work either
    assert_eq!(
        authorities
            .enacts_standard_change("hash_d", 14, &is_descendent_of)
            .unwrap(),
        None,
    );

    // but it should work at depth 15 (change height + depth)
    assert_eq!(
        authorities
            .enacts_standard_change("hash_d", 15, &is_descendent_of)
            .unwrap(),
        Some(true),
    );

    // finalizing "hash_e" at depth 20 will trigger change at "hash_b", but
    // it can't be applied yet since "hash_a" must be applied first
    assert_eq!(
        authorities
            .enacts_standard_change("hash_e", 30, &is_descendent_of)
            .unwrap(),
        Some(false),
    );
}

#[test]
fn forced_changes() {
    let mut authorities = AuthoritySet {
        current_authorities: Vec::new(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 5)];
    let set_b = vec![(AuthorityId::from_slice(&[2; 32]).unwrap(), 5)];

    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Best {
            median_last_finalized: 42,
        },
    };

    let change_b = PendingChange {
        next_authorities: set_b.clone(),
        delay: 10,
        canon_height: 5,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    authorities
        .add_pending_change(change_a, &static_is_descendent_of(false))
        .unwrap();
    authorities
        .add_pending_change(change_b.clone(), &static_is_descendent_of(false))
        .unwrap();

    // no duplicates are allowed
    assert!(matches!(
        authorities.add_pending_change(change_b, &static_is_descendent_of(false)),
        Err(Error::DuplicateAuthoritySetChange)
    ));

    // there's an effective change triggered at block 15 but not a standard one.
    // so this should do nothing.
    assert_eq!(
        authorities
            .enacts_standard_change("hash_c", 15, &static_is_descendent_of(true))
            .unwrap(),
        None,
    );

    // there can only be one pending forced change per fork
    let change_c = PendingChange {
        next_authorities: set_b.clone(),
        delay: 3,
        canon_height: 8,
        canon_hash: "hash_a8",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    let is_descendent_of_a = is_descendent_of(|base: &&str, _| base.starts_with("hash_a"));

    assert!(matches!(
        authorities.add_pending_change(change_c, &is_descendent_of_a),
        Err(Error::MultiplePendingForcedAuthoritySetChanges)
    ));

    // let's try and apply the forced changes.
    // too early and there's no forced changes to apply.
    assert!(authorities
        .apply_forced_changes("hash_a10", 10, &static_is_descendent_of(true), false, None)
        .unwrap()
        .is_none());

    // too late.
    assert!(authorities
        .apply_forced_changes("hash_a16", 16, &is_descendent_of_a, false, None)
        .unwrap()
        .is_none());

    // on time -- chooses the right change for this fork.
    assert_eq!(
        authorities
            .apply_forced_changes("hash_a15", 15, &is_descendent_of_a, false, None)
            .unwrap()
            .unwrap(),
        (
            42,
            AuthoritySet {
                current_authorities: set_a,
                set_id: 1,
                pending_standard_changes: ForkTree::new(),
                pending_forced_changes: Vec::new(),
                authority_set_changes: AuthoritySetChanges(vec![(0, 42)]),
            },
        )
    );
}

#[test]
fn forced_changes_with_no_delay() {
    // NOTE: this is a regression test
    let mut authorities = AuthoritySet {
        current_authorities: Vec::new(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 5)];

    // we create a forced change with no delay
    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 0,
        canon_height: 5,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    // and import it
    authorities
        .add_pending_change(change_a, &static_is_descendent_of(false))
        .unwrap();

    // it should be enacted at the same block that signaled it
    assert!(authorities
        .apply_forced_changes("hash_a", 5, &static_is_descendent_of(false), false, None)
        .unwrap()
        .is_some());
}

#[test]
fn forced_changes_blocked_by_standard_changes() {
    let set_a = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 1)];

    let mut authorities = AuthoritySet {
        current_authorities: set_a.clone(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    // effective at #15
    let change_a = PendingChange {
        next_authorities: set_a.clone(),
        delay: 5,
        canon_height: 10,
        canon_hash: "hash_a",
        delay_kind: DelayKind::Finalized,
    };

    // effective #20
    let change_b = PendingChange {
        next_authorities: set_a.clone(),
        delay: 0,
        canon_height: 20,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Finalized,
    };

    // effective at #35
    let change_c = PendingChange {
        next_authorities: set_a.clone(),
        delay: 5,
        canon_height: 30,
        canon_hash: "hash_c",
        delay_kind: DelayKind::Finalized,
    };

    // add some pending standard changes all on the same fork
    authorities
        .add_pending_change(change_a, &static_is_descendent_of(true))
        .unwrap();
    authorities
        .add_pending_change(change_b, &static_is_descendent_of(true))
        .unwrap();
    authorities
        .add_pending_change(change_c, &static_is_descendent_of(true))
        .unwrap();

    // effective at #45
    let change_d = PendingChange {
        next_authorities: set_a.clone(),
        delay: 5,
        canon_height: 40,
        canon_hash: "hash_d",
        delay_kind: DelayKind::Best {
            median_last_finalized: 31,
        },
    };

    // now add a forced change on the same fork
    authorities
        .add_pending_change(change_d, &static_is_descendent_of(true))
        .unwrap();

    // the forced change cannot be applied since the pending changes it depends on
    // have not been applied yet.
    assert!(matches!(
        authorities.apply_forced_changes(
            "hash_d45",
            45,
            &static_is_descendent_of(true),
            false,
            None
        ),
        Err(Error::ForcedAuthoritySetChangeDependencyUnsatisfied(15))
    ));
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges::empty()
    );

    // we apply the first pending standard change at #15
    authorities
        .apply_standard_changes("hash_a15", 15, &static_is_descendent_of(true), false, None)
        .unwrap();
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15)])
    );

    // but the forced change still depends on the next standard change
    assert!(matches!(
        authorities.apply_forced_changes("hash_d", 45, &static_is_descendent_of(true), false, None),
        Err(Error::ForcedAuthoritySetChangeDependencyUnsatisfied(20))
    ));
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15)])
    );

    // we apply the pending standard change at #20
    authorities
        .apply_standard_changes("hash_b", 20, &static_is_descendent_of(true), false, None)
        .unwrap();
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15), (1, 20)])
    );

    // afterwards the forced change at #45 can already be applied since it signals
    // that finality stalled at #31, and the next pending standard change is effective
    // at #35. subsequent forced changes on the same branch must be kept
    assert_eq!(
        authorities
            .apply_forced_changes("hash_d", 45, &static_is_descendent_of(true), false, None)
            .unwrap()
            .unwrap(),
        (
            31,
            AuthoritySet {
                current_authorities: set_a.clone(),
                set_id: 3,
                pending_standard_changes: ForkTree::new(),
                pending_forced_changes: Vec::new(),
                authority_set_changes: AuthoritySetChanges(vec![(0, 15), (1, 20), (2, 31)]),
            }
        ),
    );
    assert_eq!(
        authorities.authority_set_changes,
        AuthoritySetChanges(vec![(0, 15), (1, 20)])
    );
}

#[test]
fn next_change_works() {
    let current_authorities = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 1)];

    let mut authorities = AuthoritySet {
        current_authorities: current_authorities.clone(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let new_set = current_authorities.clone();

    // We have three pending changes with 2 possible roots that are enacted
    // immediately on finality (i.e. standard changes).
    let change_a0 = PendingChange {
        next_authorities: new_set.clone(),
        delay: 0,
        canon_height: 5,
        canon_hash: "hash_a0",
        delay_kind: DelayKind::Finalized,
    };

    let change_a1 = PendingChange {
        next_authorities: new_set.clone(),
        delay: 0,
        canon_height: 10,
        canon_hash: "hash_a1",
        delay_kind: DelayKind::Finalized,
    };

    let change_b = PendingChange {
        next_authorities: new_set.clone(),
        delay: 0,
        canon_height: 4,
        canon_hash: "hash_b",
        delay_kind: DelayKind::Finalized,
    };

    // A0 (#5) <- A10 (#8) <- A1 (#10) <- best_a
    // B (#4) <- best_b
    let is_descendent_of = is_descendent_of(|base, hash| match (*base, *hash) {
        ("hash_a0", "hash_a1") => true,
        ("hash_a0", "best_a") => true,
        ("hash_a1", "best_a") => true,
        ("hash_a10", "best_a") => true,
        ("hash_b", "best_b") => true,
        _ => false,
    });

    // add the three pending changes
    authorities
        .add_pending_change(change_b, &is_descendent_of)
        .unwrap();
    authorities
        .add_pending_change(change_a0, &is_descendent_of)
        .unwrap();
    authorities
        .add_pending_change(change_a1, &is_descendent_of)
        .unwrap();

    // the earliest change at block `best_a` should be the change at A0 (#5)
    assert_eq!(
        authorities
            .next_change(&"best_a", &is_descendent_of)
            .unwrap(),
        Some(("hash_a0", 5)),
    );

    // the earliest change at block `best_b` should be the change at B (#4)
    assert_eq!(
        authorities
            .next_change(&"best_b", &is_descendent_of)
            .unwrap(),
        Some(("hash_b", 4)),
    );

    // we apply the change at A0 which should prune it and the fork at B
    authorities
        .apply_standard_changes("hash_a0", 5, &is_descendent_of, false, None)
        .unwrap();

    // the next change is now at A1 (#10)
    assert_eq!(
        authorities
            .next_change(&"best_a", &is_descendent_of)
            .unwrap(),
        Some(("hash_a1", 10)),
    );

    // there's no longer any pending change at `best_b` fork
    assert_eq!(
        authorities
            .next_change(&"best_b", &is_descendent_of)
            .unwrap(),
        None
    );

    // we a forced change at A10 (#8)
    let change_a10 = PendingChange {
        next_authorities: new_set.clone(),
        delay: 0,
        canon_height: 8,
        canon_hash: "hash_a10",
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    authorities
        .add_pending_change(change_a10, &static_is_descendent_of(false))
        .unwrap();

    // it should take precedence over the change at A1 (#10)
    assert_eq!(
        authorities
            .next_change(&"best_a", &is_descendent_of)
            .unwrap(),
        Some(("hash_a10", 8)),
    );
}

#[test]
fn maintains_authority_list_invariants() {
    // empty authority lists are invalid
    assert_eq!(AuthoritySet::<(), ()>::genesis(vec![]), None);
    assert_eq!(
        AuthoritySet::<(), ()>::new(
            vec![],
            0,
            ForkTree::new(),
            Vec::new(),
            AuthoritySetChanges::empty(),
        ),
        None,
    );

    let invalid_authorities_weight = vec![
        (AuthorityId::from_slice(&[1; 32]).unwrap(), 5),
        (AuthorityId::from_slice(&[2; 32]).unwrap(), 0),
    ];

    // authority weight of zero is invalid
    assert_eq!(
        AuthoritySet::<(), ()>::genesis(invalid_authorities_weight.clone()),
        None
    );
    assert_eq!(
        AuthoritySet::<(), ()>::new(
            invalid_authorities_weight.clone(),
            0,
            ForkTree::new(),
            Vec::new(),
            AuthoritySetChanges::empty(),
        ),
        None,
    );

    let mut authority_set =
        AuthoritySet::<(), u64>::genesis(vec![(AuthorityId::unchecked_from([1; 32]), 5)]).unwrap();

    let invalid_change_empty_authorities = PendingChange {
        next_authorities: vec![],
        delay: 10,
        canon_height: 5,
        canon_hash: (),
        delay_kind: DelayKind::Finalized,
    };

    // pending change contains an empty authority set
    assert!(matches!(
        authority_set.add_pending_change(
            invalid_change_empty_authorities.clone(),
            &static_is_descendent_of(false)
        ),
        Err(Error::InvalidAuthoritySet)
    ));

    let invalid_change_authorities_weight = PendingChange {
        next_authorities: invalid_authorities_weight,
        delay: 10,
        canon_height: 5,
        canon_hash: (),
        delay_kind: DelayKind::Best {
            median_last_finalized: 0,
        },
    };

    // pending change contains an an authority set
    // where one authority has weight of 0
    assert!(matches!(
        authority_set.add_pending_change(
            invalid_change_authorities_weight,
            &static_is_descendent_of(false)
        ),
        Err(Error::InvalidAuthoritySet)
    ));
}

#[test]
fn cleans_up_stale_forced_changes_when_applying_standard_change() {
    let current_authorities = vec![(AuthorityId::from_slice(&[1; 32]).unwrap(), 1)];

    let mut authorities = AuthoritySet {
        current_authorities: current_authorities.clone(),
        set_id: 0,
        pending_standard_changes: ForkTree::new(),
        pending_forced_changes: Vec::new(),
        authority_set_changes: AuthoritySetChanges::empty(),
    };

    let new_set = current_authorities.clone();

    // Create the following pending changes tree:
    //
    //               [#C3]
    //              /
    //             /- (#C2)
    //            /
    // (#A) - (#B) - [#C1]
    //            \
    //             (#C0) - [#D]
    //
    // () - Standard change
    // [] - Forced change

    let is_descendent_of = {
        let hashes = vec!["B", "C0", "C1", "C2", "C3", "D"];
        is_descendent_of(move |base, hash| match (*base, *hash) {
            ("B", "B") => false, // required to have the simpler case below
            ("A", b) | ("B", b) => hashes.iter().any(|h| *h == b),
            ("C0", "D") => true,
            _ => false,
        })
    };

    let mut add_pending_change = |canon_height, canon_hash, forced| {
        let change = PendingChange {
            next_authorities: new_set.clone(),
            delay: 0,
            canon_height,
            canon_hash,
            delay_kind: if forced {
                DelayKind::Best {
                    median_last_finalized: 0,
                }
            } else {
                DelayKind::Finalized
            },
        };

        authorities
            .add_pending_change(change, &is_descendent_of)
            .unwrap();
    };

    add_pending_change(5, "A", false);
    add_pending_change(10, "B", false);
    add_pending_change(15, "C0", false);
    add_pending_change(15, "C1", true);
    add_pending_change(15, "C2", false);
    add_pending_change(15, "C3", true);
    add_pending_change(20, "D", true);

    // applying the standard change at A should not prune anything
    // other then the change that was applied
    authorities
        .apply_standard_changes("A", 5, &is_descendent_of, false, None)
        .unwrap();

    assert_eq!(authorities.pending_changes().count(), 6);

    // same for B
    authorities
        .apply_standard_changes("B", 10, &is_descendent_of, false, None)
        .unwrap();

    assert_eq!(authorities.pending_changes().count(), 5);

    let authorities2 = authorities.clone();

    // finalizing C2 should clear all forced changes
    authorities
        .apply_standard_changes("C2", 15, &is_descendent_of, false, None)
        .unwrap();

    assert_eq!(authorities.pending_forced_changes.len(), 0);

    // finalizing C0 should clear all forced changes but D
    let mut authorities = authorities2;
    authorities
        .apply_standard_changes("C0", 15, &is_descendent_of, false, None)
        .unwrap();

    assert_eq!(authorities.pending_forced_changes.len(), 1);
    assert_eq!(
        authorities
            .pending_forced_changes
            .first()
            .unwrap()
            .canon_hash,
        "D"
    );
}

#[test]
fn authority_set_changes_insert() {
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 41);
    authority_set_changes.append(1, 81);
    authority_set_changes.append(4, 121);

    authority_set_changes.insert(101);
    assert_eq!(
        authority_set_changes.get_set_id(100),
        AuthoritySetChangeId::Set(2, 101)
    );
    assert_eq!(
        authority_set_changes.get_set_id(101),
        AuthoritySetChangeId::Set(2, 101)
    );
}

#[test]
fn authority_set_changes_for_complete_data() {
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 41);
    authority_set_changes.append(1, 81);
    authority_set_changes.append(2, 121);

    assert_eq!(
        authority_set_changes.get_set_id(20),
        AuthoritySetChangeId::Set(0, 41)
    );
    assert_eq!(
        authority_set_changes.get_set_id(40),
        AuthoritySetChangeId::Set(0, 41)
    );
    assert_eq!(
        authority_set_changes.get_set_id(41),
        AuthoritySetChangeId::Set(0, 41)
    );
    assert_eq!(
        authority_set_changes.get_set_id(42),
        AuthoritySetChangeId::Set(1, 81)
    );
    assert_eq!(
        authority_set_changes.get_set_id(141),
        AuthoritySetChangeId::Latest
    );
}

#[test]
fn authority_set_changes_for_incomplete_data() {
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(2, 41);
    authority_set_changes.append(3, 81);
    authority_set_changes.append(4, 121);

    assert_eq!(
        authority_set_changes.get_set_id(20),
        AuthoritySetChangeId::Unknown
    );
    assert_eq!(
        authority_set_changes.get_set_id(40),
        AuthoritySetChangeId::Unknown
    );
    assert_eq!(
        authority_set_changes.get_set_id(41),
        AuthoritySetChangeId::Unknown
    );
    assert_eq!(
        authority_set_changes.get_set_id(42),
        AuthoritySetChangeId::Set(3, 81)
    );
    assert_eq!(
        authority_set_changes.get_set_id(141),
        AuthoritySetChangeId::Latest
    );
}

#[test]
fn iter_from_works() {
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(1, 41);
    authority_set_changes.append(2, 81);

    // we are missing the data for the first set, therefore we should return `None`
    assert_eq!(
        None,
        authority_set_changes
            .iter_from(40)
            .map(|it| it.collect::<Vec<_>>())
    );

    // after adding the data for the first set the same query should work
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 21);
    authority_set_changes.append(1, 41);
    authority_set_changes.append(2, 81);
    authority_set_changes.append(3, 121);

    assert_eq!(
        Some(vec![(1, 41), (2, 81), (3, 121)]),
        authority_set_changes
            .iter_from(40)
            .map(|it| it.cloned().collect::<Vec<_>>()),
    );

    assert_eq!(
        Some(vec![(2, 81), (3, 121)]),
        authority_set_changes
            .iter_from(41)
            .map(|it| it.cloned().collect::<Vec<_>>()),
    );

    assert_eq!(0, authority_set_changes.iter_from(121).unwrap().count());

    assert_eq!(0, authority_set_changes.iter_from(200).unwrap().count());
}
