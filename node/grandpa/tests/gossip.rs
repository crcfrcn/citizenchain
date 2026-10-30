use super::{super::NEIGHBOR_REBROADCAST_PERIOD, environment::SharedVoterSetState, *};
use crate::communication;
use sc_network::config::Role;
use sc_network_gossip::Validator as GossipValidatorT;
use sp_core::{crypto::UncheckedFrom, H256};
use std::time::Instant;
use substrate_test_runtime_client::runtime::{Block, Header};

// some random config (not really needed)
fn config() -> crate::Config {
    crate::Config {
        gossip_duration: Duration::from_millis(10),
        justification_generation_period: 256,
        keystore: None,
        name: None,
        local_role: Role::Authority,
        observer_enabled: true,
        telemetry: None,
        protocol_name: communication::grandpa_protocol_name::NAME.into(),
    }
}

// dummy voter set state
fn voter_set_state() -> SharedVoterSetState<Block> {
    use crate::{authorities::AuthoritySet, environment::VoterSetState};

    let base = (H256::zero(), 0);

    let voters = vec![(AuthorityId::unchecked_from([1; 32]), 1)];
    let voters = AuthoritySet::genesis(voters).unwrap();

    let set_state = VoterSetState::live(0, &voters, base);

    set_state.into()
}

#[test]
fn view_vote_rules() {
    let view = View {
        round: Round(100),
        set_id: SetId(1),
        last_commit: Some(1000u64),
        last_update: None,
    };

    assert_eq!(
        view.consider_vote(Round(98), SetId(1)),
        Consider::RejectPast
    );
    assert_eq!(view.consider_vote(Round(1), SetId(0)), Consider::RejectPast);
    assert_eq!(
        view.consider_vote(Round(1000), SetId(0)),
        Consider::RejectPast
    );

    assert_eq!(view.consider_vote(Round(99), SetId(1)), Consider::Accept);
    assert_eq!(view.consider_vote(Round(100), SetId(1)), Consider::Accept);
    assert_eq!(view.consider_vote(Round(101), SetId(1)), Consider::Accept);

    assert_eq!(
        view.consider_vote(Round(102), SetId(1)),
        Consider::RejectFuture
    );
    assert_eq!(
        view.consider_vote(Round(1), SetId(2)),
        Consider::RejectFuture
    );
    assert_eq!(
        view.consider_vote(Round(1000), SetId(2)),
        Consider::RejectFuture
    );
}

#[test]
fn view_global_message_rules() {
    let view = View {
        round: Round(100),
        set_id: SetId(2),
        last_commit: Some(1000u64),
        last_update: None,
    };

    assert_eq!(view.consider_global(SetId(3), 1), Consider::RejectFuture);
    assert_eq!(view.consider_global(SetId(3), 1000), Consider::RejectFuture);
    assert_eq!(
        view.consider_global(SetId(3), 10000),
        Consider::RejectFuture
    );

    assert_eq!(view.consider_global(SetId(1), 1), Consider::RejectPast);
    assert_eq!(view.consider_global(SetId(1), 1000), Consider::RejectPast);
    assert_eq!(view.consider_global(SetId(1), 10000), Consider::RejectPast);

    assert_eq!(view.consider_global(SetId(2), 1), Consider::RejectPast);
    assert_eq!(view.consider_global(SetId(2), 1000), Consider::RejectPast);
    assert_eq!(view.consider_global(SetId(2), 1001), Consider::Accept);
    assert_eq!(view.consider_global(SetId(2), 10000), Consider::Accept);
}

#[test]
fn unknown_peer_cannot_be_updated() {
    let mut peers = Peers::new(NEIGHBOR_REBROADCAST_PERIOD);
    let id = PeerId::random();

    let update = NeighborPacket {
        round: Round(5),
        set_id: SetId(10),
        commit_finalized_height: 50,
    };

    let res = peers.update_peer_state(&id, update.clone());
    assert!(res.unwrap().is_none());

    // connect & disconnect.
    peers.new_peer(id, ObservedRole::Authority);
    peers.peer_disconnected(&id);

    let res = peers.update_peer_state(&id, update.clone());
    assert!(res.unwrap().is_none());
}

#[test]
fn update_peer_state() {
    let update1 = NeighborPacket {
        round: Round(5),
        set_id: SetId(10),
        commit_finalized_height: 50u32,
    };

    let update2 = NeighborPacket {
        round: Round(6),
        set_id: SetId(10),
        commit_finalized_height: 60,
    };

    let update3 = NeighborPacket {
        round: Round(2),
        set_id: SetId(11),
        commit_finalized_height: 61,
    };

    let update4 = NeighborPacket {
        round: Round(3),
        set_id: SetId(11),
        commit_finalized_height: 80,
    };

    // Use shorter rebroadcast period to safely roll the clock back in the last test
    // and don't hit the system boot time on systems with unsigned time.
    const SHORT_NEIGHBOR_REBROADCAST_PERIOD: Duration = Duration::from_secs(1);
    let mut peers = Peers::new(SHORT_NEIGHBOR_REBROADCAST_PERIOD);
    let id = PeerId::random();

    peers.new_peer(id, ObservedRole::Authority);

    let check_update = |peers: &mut Peers<_>, update: NeighborPacket<_>| {
        let view = peers
            .update_peer_state(&id, update.clone())
            .unwrap()
            .unwrap();
        assert_eq!(view.round, update.round);
        assert_eq!(view.set_id, update.set_id);
        assert_eq!(view.last_commit, Some(update.commit_finalized_height));
    };

    check_update(&mut peers, update1);
    check_update(&mut peers, update2);
    check_update(&mut peers, update3);
    check_update(&mut peers, update4.clone());

    // Allow duplicate neighbor packets if enough time has passed.
    peers.inner.get_mut(&id).unwrap().view.last_update =
        Some(Instant::now() - SHORT_NEIGHBOR_REBROADCAST_PERIOD);
    check_update(&mut peers, update4);
}

#[test]
fn invalid_view_change() {
    let mut peers = Peers::new(NEIGHBOR_REBROADCAST_PERIOD);

    let id = PeerId::random();
    peers.new_peer(id, ObservedRole::Authority);

    peers
        .update_peer_state(
            &id,
            NeighborPacket {
                round: Round(10),
                set_id: SetId(10),
                commit_finalized_height: 10,
            },
        )
        .unwrap()
        .unwrap();

    let mut check_update = move |update: NeighborPacket<_>, misbehavior| {
        let err = peers.update_peer_state(&id, update.clone()).unwrap_err();
        assert_eq!(err, misbehavior);
    };

    // round moves backwards.
    check_update(
        NeighborPacket {
            round: Round(9),
            set_id: SetId(10),
            commit_finalized_height: 10,
        },
        Misbehavior::InvalidViewChange,
    );
    // set ID moves backwards.
    check_update(
        NeighborPacket {
            round: Round(10),
            set_id: SetId(9),
            commit_finalized_height: 10,
        },
        Misbehavior::InvalidViewChange,
    );
    // commit finalized height moves backwards.
    check_update(
        NeighborPacket {
            round: Round(10),
            set_id: SetId(10),
            commit_finalized_height: 9,
        },
        Misbehavior::InvalidViewChange,
    );
    // duplicate packet without grace period.
    check_update(
        NeighborPacket {
            round: Round(10),
            set_id: SetId(10),
            commit_finalized_height: 10,
        },
        Misbehavior::DuplicateNeighborMessage,
    );
    // commit finalized height moves backwards while round moves forward.
    check_update(
        NeighborPacket {
            round: Round(11),
            set_id: SetId(10),
            commit_finalized_height: 9,
        },
        Misbehavior::InvalidViewChange,
    );
    // commit finalized height moves backwards while set ID moves forward.
    check_update(
        NeighborPacket {
            round: Round(10),
            set_id: SetId(11),
            commit_finalized_height: 9,
        },
        Misbehavior::InvalidViewChange,
    );
}

#[test]
fn messages_not_expired_immediately() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    let set_id = 1;

    val.note_set(SetId(set_id), Vec::new(), |_, _| {});

    for round_num in 1u64..10 {
        val.note_round(Round(round_num), |_, _| {});
    }

    {
        let mut is_expired = val.message_expired();
        let last_kept_round = 10u64 - KEEP_RECENT_ROUNDS as u64 - 1;

        // messages from old rounds are expired.
        for round_num in 1u64..last_kept_round {
            let topic = communication::round_topic::<Block>(round_num, 1);
            assert!(is_expired(topic, &[1, 2, 3]));
        }

        // messages from not-too-old rounds are not expired.
        for round_num in last_kept_round..10 {
            let topic = communication::round_topic::<Block>(round_num, 1);
            assert!(!is_expired(topic, &[1, 2, 3]));
        }
    }
}

#[test]
fn message_from_unknown_authority_discarded() {
    assert!(cost::UNKNOWN_VOTER != cost::BAD_SIGNATURE);

    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);
    let set_id = 1;
    let auth = AuthorityId::unchecked_from([1u8; 32]);
    let peer = PeerId::random();

    val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
    val.note_round(Round(1), |_, _| {});

    let inner = val.inner.read();
    let unknown_voter = inner.validate_round_message(
        &peer,
        &VoteMessage {
            round: Round(1),
            set_id: SetId(set_id),
            message: SignedMessage::<Header> {
                message: finality_grandpa::Message::Prevote(finality_grandpa::Prevote {
                    target_hash: Default::default(),
                    target_number: 10,
                }),
                signature: UncheckedFrom::unchecked_from([1; 64]),
                id: UncheckedFrom::unchecked_from([2u8; 32]),
            },
        },
    );

    let bad_sig = inner.validate_round_message(
        &peer,
        &VoteMessage {
            round: Round(1),
            set_id: SetId(set_id),
            message: SignedMessage::<Header> {
                message: finality_grandpa::Message::Prevote(finality_grandpa::Prevote {
                    target_hash: Default::default(),
                    target_number: 10,
                }),
                signature: UncheckedFrom::unchecked_from([1; 64]),
                id: auth.clone(),
            },
        },
    );

    assert_eq!(unknown_voter, Action::Discard(cost::UNKNOWN_VOTER));
    assert_eq!(bad_sig, Action::Discard(cost::BAD_SIGNATURE));
}

#[test]
fn unsolicited_catch_up_messages_discarded() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    let set_id = 1;
    let auth = AuthorityId::unchecked_from([1u8; 32]);
    let peer = PeerId::random();

    val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
    val.note_round(Round(1), |_, _| {});

    let validate_catch_up = || {
        let mut inner = val.inner.write();
        inner.validate_catch_up_message(
            &peer,
            &FullCatchUpMessage {
                set_id: SetId(set_id),
                message: finality_grandpa::CatchUp {
                    round_number: 10,
                    prevotes: Default::default(),
                    precommits: Default::default(),
                    base_hash: Default::default(),
                    base_number: Default::default(),
                },
            },
        )
    };

    // the catch up is discarded because we have no pending request
    assert_eq!(
        validate_catch_up(),
        Action::Discard(cost::OUT_OF_SCOPE_MESSAGE)
    );

    let noted = val.inner.write().note_catch_up_request(
        &peer,
        &CatchUpRequestMessage {
            set_id: SetId(set_id),
            round: Round(10),
        },
    );

    assert!(noted.0);

    // catch up is allowed because we have requested it, but it's rejected
    // because it's malformed (empty prevotes and precommits)
    assert_eq!(
        validate_catch_up(),
        Action::Discard(cost::MALFORMED_CATCH_UP)
    );
}

#[test]
fn unanswerable_catch_up_requests_discarded() {
    // create voter set state with round 2 completed
    let set_state: SharedVoterSetState<Block> = {
        let mut completed_rounds = voter_set_state().read().completed_rounds();

        completed_rounds.push(environment::CompletedRound {
            number: 2,
            state: finality_grandpa::round::State::genesis(Default::default()),
            base: Default::default(),
            votes: Default::default(),
        });

        let mut current_rounds = environment::CurrentRounds::<Block>::new();
        current_rounds.insert(3, environment::HasVoted::No);

        let set_state = environment::VoterSetState::<Block>::Live {
            completed_rounds,
            current_rounds,
        };

        set_state.into()
    };

    let (val, _) = GossipValidator::<Block>::new(config(), set_state.clone(), None, None);

    let set_id = 1;
    let auth = AuthorityId::unchecked_from([1u8; 32]);
    let peer = PeerId::random();

    val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
    val.note_round(Round(3), |_, _| {});

    // add the peer making the request to the validator,
    // otherwise it is discarded
    let mut inner = val.inner.write();
    inner.peers.new_peer(peer, ObservedRole::Authority);

    let res = inner.handle_catch_up_request(
        &peer,
        CatchUpRequestMessage {
            set_id: SetId(set_id),
            round: Round(10),
        },
        &set_state,
    );

    // we're at round 3, a catch up request for round 10 is out of scope
    assert!(res.0.is_none());
    assert_eq!(res.1, Action::Discard(cost::OUT_OF_SCOPE_MESSAGE));

    let res = inner.handle_catch_up_request(
        &peer,
        CatchUpRequestMessage {
            set_id: SetId(set_id),
            round: Round(2),
        },
        &set_state,
    );

    // a catch up request for round 2 should be answered successfully
    match res.0.unwrap() {
        GossipMessage::CatchUp(catch_up) => {
            assert_eq!(catch_up.set_id, SetId(set_id));
            assert_eq!(catch_up.message.round_number, 2);

            assert_eq!(res.1, Action::Discard(cost::CATCH_UP_REPLY));
        }
        _ => panic!("expected catch up message"),
    };
}

#[test]
fn detects_honest_out_of_scope_catch_requests() {
    let set_state = voter_set_state();
    let (val, _) = GossipValidator::<Block>::new(config(), set_state.clone(), None, None);

    // the validator starts at set id 2
    val.note_set(SetId(2), Vec::new(), |_, _| {});

    // add the peer making the request to the validator,
    // otherwise it is discarded
    let peer = PeerId::random();
    val.inner
        .write()
        .peers
        .new_peer(peer, ObservedRole::Authority);

    let send_request = |set_id, round| {
        let mut inner = val.inner.write();
        inner.handle_catch_up_request(
            &peer,
            CatchUpRequestMessage {
                set_id: SetId(set_id),
                round: Round(round),
            },
            &set_state,
        )
    };

    let assert_res = |res: (Option<_>, Action<_>), honest| {
        assert!(res.0.is_none());
        assert_eq!(
            res.1,
            if honest {
                Action::Discard(cost::HONEST_OUT_OF_SCOPE_CATCH_UP)
            } else {
                Action::Discard(Misbehavior::OutOfScopeMessage.cost())
            },
        );
    };

    // the validator is at set id 2 and round 0. requests for set id 1
    // should not be answered but they should be considered an honest
    // mistake
    assert_res(send_request(1, 1), true);

    assert_res(send_request(1, 10), true);

    // requests for set id 0 should be considered out of scope
    assert_res(send_request(0, 1), false);

    assert_res(send_request(0, 10), false);

    // after the validator progresses further than CATCH_UP_THRESHOLD in set
    // id 2, any request for set id 1 should no longer be considered an
    // honest mistake.
    val.note_round(Round(3), |_, _| {});

    assert_res(send_request(1, 1), false);

    assert_res(send_request(1, 2), false);
}

#[test]
fn issues_catch_up_request_on_neighbor_packet_import() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // the validator starts at set id 1.
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // add the peer making the request to the validator,
    // otherwise it is discarded.
    let peer = PeerId::random();
    val.inner
        .write()
        .peers
        .new_peer(peer, ObservedRole::Authority);

    let import_neighbor_message = |set_id, round| {
        let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
            &peer,
            NeighborPacket {
                round: Round(round),
                set_id: SetId(set_id),
                commit_finalized_height: 42,
            },
        );

        catch_up_request
    };

    // importing a neighbor message from a peer in the same set in a later
    // round should lead to a catch up request for the previous round.
    match import_neighbor_message(1, 42) {
        Some(GossipMessage::CatchUpRequest(request)) => {
            assert_eq!(request.set_id, SetId(1));
            assert_eq!(request.round, Round(41));
        }
        _ => panic!("expected catch up message"),
    }

    // we note that we're at round 41.
    val.note_round(Round(41), |_, _| {});

    // if we import a neighbor message within CATCH_UP_THRESHOLD then we
    // won't request a catch up.
    match import_neighbor_message(1, 42) {
        None => {}
        _ => panic!("expected no catch up message"),
    }

    // or if the peer is on a lower round.
    match import_neighbor_message(1, 40) {
        None => {}
        _ => panic!("expected no catch up message"),
    }

    // we also don't request a catch up if the peer is in a different set.
    match import_neighbor_message(2, 42) {
        None => {}
        _ => panic!("expected no catch up message"),
    }
}

#[test]
fn doesnt_send_catch_up_requests_when_disabled() {
    // we create a gossip validator with catch up requests disabled.
    let config = {
        let mut c = config();

        // if the observer protocol is enabled and we are not an authority,
        // then we don't issue any catch-up requests.
        c.local_role = Role::Full;
        c.observer_enabled = true;

        c
    };

    let (val, _) = GossipValidator::<Block>::new(config, voter_set_state(), None, None);

    // the validator starts at set id 1.
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // add the peer making the request to the validator,
    // otherwise it is discarded.
    let peer = PeerId::random();
    val.inner
        .write()
        .peers
        .new_peer(peer, ObservedRole::Authority);

    // importing a neighbor message from a peer in the same set in a later
    // round should lead to a catch up request but since they're disabled
    // we should get `None`.
    let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
        &peer,
        NeighborPacket {
            round: Round(42),
            set_id: SetId(1),
            commit_finalized_height: 50,
        },
    );

    match catch_up_request {
        None => {}
        _ => panic!("expected no catch up message"),
    }
}

#[test]
fn doesnt_send_catch_up_requests_to_non_authorities_when_observer_enabled() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // the validator starts at set id 1.
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // add the peers making the requests to the validator,
    // otherwise it is discarded.
    let peer_authority = PeerId::random();
    let peer_full = PeerId::random();

    val.inner
        .write()
        .peers
        .new_peer(peer_authority, ObservedRole::Authority);
    val.inner
        .write()
        .peers
        .new_peer(peer_full, ObservedRole::Full);

    let import_neighbor_message = |peer| {
        let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
            &peer,
            NeighborPacket {
                round: Round(42),
                set_id: SetId(1),
                commit_finalized_height: 50,
            },
        );

        catch_up_request
    };

    // importing a neighbor message from a peer in the same set in a later
    // round should lead to a catch up request but since the node is not an
    // authority we should get `None`.
    if import_neighbor_message(peer_full).is_some() {
        panic!("expected no catch up message");
    }

    // importing the same neighbor message from a peer who is an authority
    // should lead to a catch up request.
    match import_neighbor_message(peer_authority) {
        Some(GossipMessage::CatchUpRequest(request)) => {
            assert_eq!(request.set_id, SetId(1));
            assert_eq!(request.round, Round(41));
        }
        _ => panic!("expected catch up message"),
    }
}

#[test]
fn sends_catch_up_requests_to_non_authorities_when_observer_disabled() {
    let config = {
        let mut c = config();

        // if the observer protocol is disable any full-node should be able
        // to answer catch-up requests.
        c.observer_enabled = false;

        c
    };

    let (val, _) = GossipValidator::<Block>::new(config, voter_set_state(), None, None);

    // the validator starts at set id 1.
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // add the peer making the requests to the validator, otherwise it is
    // discarded.
    let peer_full = PeerId::random();
    val.inner
        .write()
        .peers
        .new_peer(peer_full, ObservedRole::Full);

    let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
        &peer_full,
        NeighborPacket {
            round: Round(42),
            set_id: SetId(1),
            commit_finalized_height: 50,
        },
    );

    // importing a neighbor message from a peer in the same set in a later
    // round should lead to a catch up request, the node is not an
    // authority, but since the observer protocol is disabled we should
    // issue a catch-up request to it anyway.
    match catch_up_request {
        Some(GossipMessage::CatchUpRequest(request)) => {
            assert_eq!(request.set_id, SetId(1));
            assert_eq!(request.round, Round(41));
        }
        _ => panic!("expected catch up message"),
    }
}

#[test]
fn doesnt_expire_next_round_messages() {
    // NOTE: this is a regression test
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // the validator starts at set id 1.
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // we are at round 10
    val.note_round(Round(9), |_, _| {});
    val.note_round(Round(10), |_, _| {});

    let mut is_expired = val.message_expired();

    // we accept messages from rounds 9, 10 and 11
    // therefore neither of those should be considered expired
    for round in &[9, 10, 11] {
        assert!(!is_expired(
            communication::round_topic::<Block>(*round, 1),
            &[]
        ))
    }
}

#[test]
fn progressively_gossips_to_more_peers_as_round_duration_increases() {
    let mut config = config();
    config.gossip_duration = Duration::from_secs(300); // Set to high value to prevent test race
    let round_duration = config.gossip_duration * ROUND_DURATION;

    let (val, _) = GossipValidator::<Block>::new(config, voter_set_state(), None, None);

    // the validator start at set id 0
    val.note_set(SetId(0), Vec::new(), |_, _| {});

    // add 60 peers, 30 authorities and 30 full nodes
    let mut authorities = Vec::new();
    authorities.resize_with(30, || PeerId::random());

    let mut full_nodes = Vec::new();
    full_nodes.resize_with(30, || PeerId::random());

    for i in 0..30 {
        val.inner
            .write()
            .peers
            .new_peer(authorities[i], ObservedRole::Authority);

        val.inner
            .write()
            .peers
            .new_peer(full_nodes[i], ObservedRole::Full);
    }

    let test = |rounds_elapsed, peers| {
        // rewind n round durations
        val.inner.write().local_view.as_mut().unwrap().round_start = Instant::now()
            - Duration::from_millis((round_duration.as_millis() as f32 * rounds_elapsed) as u64);

        val.inner.write().peers.reshuffle();

        let mut message_allowed = val.message_allowed();

        move || {
            let mut allowed = 0;
            for peer in peers {
                if message_allowed(
                    peer,
                    MessageIntent::Broadcast,
                    &communication::round_topic::<Block>(1, 0),
                    &[],
                ) {
                    allowed += 1;
                }
            }
            allowed
        }
    };

    fn trial<F: FnMut() -> usize>(mut test: F) -> usize {
        let mut results = Vec::new();
        let n = 1000;

        for _ in 0..n {
            results.push(test());
        }

        let n = results.len();
        let sum: usize = results.iter().sum();

        sum / n
    }

    let all_peers = authorities
        .iter()
        .chain(full_nodes.iter())
        .cloned()
        .collect();

    // on the first attempt we will only gossip to 4 peers, either
    // authorities or full nodes, but we'll guarantee that half of those
    // are authorities
    assert!(trial(test(1.0, &authorities)) >= LUCKY_PEERS / 2);
    assert_eq!(trial(test(1.0, &all_peers)), LUCKY_PEERS);

    // after more than 1.5 round durations have elapsed we should gossip to
    // `sqrt(peers)` we're connected to, but we guarantee that at least 4 of
    // those peers are authorities (plus the `LUCKY_PEERS` from the previous
    // stage)
    assert!(trial(test(PROPAGATION_SOME * 1.1, &authorities)) >= LUCKY_PEERS);
    assert_eq!(
        trial(test(2.0, &all_peers)),
        LUCKY_PEERS + (all_peers.len() as f64).sqrt() as usize,
    );

    // after 3 rounds durations we should gossip to all peers we are
    // connected to
    assert_eq!(
        trial(test(PROPAGATION_ALL * 1.1, &all_peers)),
        all_peers.len()
    );
}

#[test]
fn never_gossips_round_messages_to_light_clients() {
    let config = config();
    let round_duration = config.gossip_duration * ROUND_DURATION;
    let (val, _) = GossipValidator::<Block>::new(config, voter_set_state(), None, None);

    // the validator starts at set id 0
    val.note_set(SetId(0), Vec::new(), |_, _| {});

    // add a new light client as peer
    let light_peer = PeerId::random();

    val.inner
        .write()
        .peers
        .new_peer(light_peer, ObservedRole::Light);

    assert!(!val.message_allowed()(
        &light_peer,
        MessageIntent::Broadcast,
        &communication::round_topic::<Block>(1, 0),
        &[],
    ));

    // we reverse the round start time so that the elapsed time is higher
    // (which should lead to more peers getting the message)
    val.inner.write().local_view.as_mut().unwrap().round_start =
        Instant::now() - round_duration * 10;

    // even after the round has been going for 10 round durations we will never
    // gossip to light clients
    assert!(!val.message_allowed()(
        &light_peer,
        MessageIntent::Broadcast,
        &communication::round_topic::<Block>(1, 0),
        &[],
    ));

    // update the peer state and local state wrt commits
    val.inner
        .write()
        .peers
        .update_peer_state(
            &light_peer,
            NeighborPacket {
                round: Round(1),
                set_id: SetId(0),
                commit_finalized_height: 1,
            },
        )
        .unwrap();

    val.note_commit_finalized(Round(1), SetId(0), 2, |_, _| {});

    let commit = {
        let commit = finality_grandpa::CompactCommit {
            target_hash: H256::random(),
            target_number: 2,
            precommits: Vec::new(),
            auth_data: Vec::new(),
        };

        communication::gossip::GossipMessage::<Block>::Commit(
            communication::gossip::FullCommitMessage {
                round: Round(2),
                set_id: SetId(0),
                message: commit,
            },
        )
        .encode()
    };

    // global messages are gossiped to light clients though
    assert!(val.message_allowed()(
        &light_peer,
        MessageIntent::Broadcast,
        &communication::global_topic::<Block>(0),
        &commit,
    ));
}

#[test]
fn only_gossip_commits_to_peers_on_same_set() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // the validator starts at set id 1
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // add a new peer at set id 1
    let peer1 = PeerId::random();

    val.inner
        .write()
        .peers
        .new_peer(peer1, ObservedRole::Authority);

    val.inner
        .write()
        .peers
        .update_peer_state(
            &peer1,
            NeighborPacket {
                round: Round(1),
                set_id: SetId(1),
                commit_finalized_height: 1,
            },
        )
        .unwrap();

    // peer2 will default to set id 0
    let peer2 = PeerId::random();
    val.inner
        .write()
        .peers
        .new_peer(peer2, ObservedRole::Authority);

    // create a commit for round 1 of set id 1
    // targeting a block at height 2
    let commit = {
        let commit = finality_grandpa::CompactCommit {
            target_hash: H256::random(),
            target_number: 2,
            precommits: Vec::new(),
            auth_data: Vec::new(),
        };

        communication::gossip::GossipMessage::<Block>::Commit(
            communication::gossip::FullCommitMessage {
                round: Round(1),
                set_id: SetId(1),
                message: commit,
            },
        )
        .encode()
    };

    // note the commit in the validator
    val.note_commit_finalized(Round(1), SetId(1), 2, |_, _| {});

    let mut message_allowed = val.message_allowed();

    // the commit should be allowed to peer 1
    assert!(message_allowed(
        &peer1,
        MessageIntent::Broadcast,
        &communication::global_topic::<Block>(1),
        &commit,
    ));

    // but disallowed to peer 2 since the peer is on set id 0
    // the commit should be allowed to peer 1
    assert!(!message_allowed(
        &peer2,
        MessageIntent::Broadcast,
        &communication::global_topic::<Block>(1),
        &commit,
    ));
}

#[test]
fn expire_commits_from_older_rounds() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    let commit = |round, set_id, target_number| {
        let commit = finality_grandpa::CompactCommit {
            target_hash: H256::random(),
            target_number,
            precommits: Vec::new(),
            auth_data: Vec::new(),
        };

        communication::gossip::GossipMessage::<Block>::Commit(
            communication::gossip::FullCommitMessage {
                round: Round(round),
                set_id: SetId(set_id),
                message: commit,
            },
        )
        .encode()
    };

    // note the beginning of a new set with id 1
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    // note a commit for round 1 in the validator
    // finalizing a block at height 2
    val.note_commit_finalized(Round(1), SetId(1), 2, |_, _| {});

    let mut message_expired = val.message_expired();

    // a commit message for round 1 that finalizes the same height as we
    // have observed previously should not be expired
    assert!(!message_expired(
        communication::global_topic::<Block>(1),
        &commit(1, 1, 2),
    ));

    // it should be expired if it is for a lower block
    assert!(message_expired(
        communication::global_topic::<Block>(1),
        &commit(1, 1, 1)
    ));

    // or the same block height but from the previous round
    assert!(message_expired(
        communication::global_topic::<Block>(1),
        &commit(0, 1, 2)
    ));
}

#[test]
fn allow_noting_different_authorities_for_same_set() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    let a1 = vec![UncheckedFrom::unchecked_from([0; 32])];
    val.note_set(SetId(1), a1.clone(), |_, _| {});

    assert_eq!(val.inner().read().authorities, a1);

    let a2 = vec![
        UncheckedFrom::unchecked_from([1; 32]),
        UncheckedFrom::unchecked_from([2; 32]),
    ];
    val.note_set(SetId(1), a2.clone(), |_, _| {});

    assert_eq!(val.inner().read().authorities, a2);
}

#[test]
fn sends_neighbor_packets_to_all_peers_when_starting_a_new_round() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // initialize the validator to a stable set id
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    let authority_peer = PeerId::random();
    let full_peer = PeerId::random();
    let light_peer = PeerId::random();

    val.inner
        .write()
        .peers
        .new_peer(authority_peer, ObservedRole::Authority);
    val.inner
        .write()
        .peers
        .new_peer(full_peer, ObservedRole::Full);
    val.inner
        .write()
        .peers
        .new_peer(light_peer, ObservedRole::Light);

    val.note_round(Round(2), |peers, message| {
        assert_eq!(peers.len(), 3);
        assert!(peers.contains(&authority_peer));
        assert!(peers.contains(&full_peer));
        assert!(peers.contains(&light_peer));
        assert!(matches!(
            message,
            NeighborPacket {
                set_id: SetId(1),
                round: Round(2),
                ..
            }
        ));
    });
}

#[test]
fn sends_neighbor_packets_to_all_peers_when_starting_a_new_set() {
    let (val, _) = GossipValidator::<Block>::new(config(), voter_set_state(), None, None);

    // initialize the validator to a stable set id
    val.note_set(SetId(1), Vec::new(), |_, _| {});

    let authority_peer = PeerId::random();
    let full_peer = PeerId::random();
    let light_peer = PeerId::random();

    val.inner
        .write()
        .peers
        .new_peer(authority_peer, ObservedRole::Authority);
    val.inner
        .write()
        .peers
        .new_peer(full_peer, ObservedRole::Full);
    val.inner
        .write()
        .peers
        .new_peer(light_peer, ObservedRole::Light);

    val.note_set(SetId(2), Vec::new(), |peers, message| {
        assert_eq!(peers.len(), 3);
        assert!(peers.contains(&authority_peer));
        assert!(peers.contains(&full_peer));
        assert!(peers.contains(&light_peer));
        assert!(matches!(
            message,
            NeighborPacket {
                set_id: SetId(2),
                round: Round(1),
                ..
            }
        ));
    });
}

impl<Block: BlockT> GossipValidator<Block> {
    fn inner(&self) -> &parking_lot::RwLock<Inner<Block>> {
        &self.inner
    }
}
