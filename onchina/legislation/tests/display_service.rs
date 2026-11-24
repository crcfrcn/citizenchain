//! 验证名册与提案投票装配及同账户多岗位的独立席位。

use super::*;
use crate::chain_read_proposal::VoteTally;

fn profile(seed: &str, name: &str) -> FrozenSeat {
    FrozenSeat {
        body_index: 0,
        person_cid: onchina::testing::person_for("CTZN", seed),
        account_id: Some(format!("0x{}", seed.repeat(64))),
        role_code: "MEMBER".into(),
        role_name: "委员".into(),
        name: name.into(),
    }
}

fn sample_state() -> LegProposalState {
    LegProposalState {
        proposal_id: 7,
        kind: 2,
        stage: 10,
        status: 0,
        representative_rule: 1,
        current_body: 0,
        vote_procedure: 1,
        needs_guard: false,
        representative_bodies: vec![],
        start_block: 100,
        end_block: 200,
        representative_tally: VoteTally { yes: 1, no: 1 },
        referendum_tally: VoteTally { yes: 0, no: 0 },
    }
}

fn ballot_key(digit: &str, role: &str) -> BallotKey {
    BallotKey {
        proposal_id: 7,
        body_index: 0,
        institution_cid_number: "institution-a".into(),
        role_code: role.into(),
        person_cid: onchina::testing::person_for("CTZN", digit),
    }
}
#[test]
fn one_wallet_multiple_roles_and_foreign_tickets_remain_independent() {
    let first = profile("a", "甲");
    let mut second = first.clone();
    second.role_code = "OTHER_ROLE".into();
    let ballots = HashMap::from([
        (ballot_key("a", "MEMBER"), true),
        (
            BallotKey {
                institution_cid_number: "institution-b".into(),
                ..ballot_key("a", "OTHER_ROLE")
            },
            false,
        ),
        (
            BallotKey {
                body_index: 1,
                ..ballot_key("a", "OTHER_ROLE")
            },
            false,
        ),
        (
            BallotKey {
                proposal_id: 8,
                ..ballot_key("a", "OTHER_ROLE")
            },
            false,
        ),
    ]);
    let view = build_active_proposal_view(
        sample_state(),
        "institution-a",
        &[first.clone(), second, first],
        &ballots,
    );
    assert_eq!(view.seats.len(), 2);
    assert_eq!(view.seats[0].vote, Some(true));
    assert_eq!(view.seats[1].vote, None);
    assert_ne!(view.seats[0].ticket_id, view.seats[1].ticket_id);
    assert_eq!(view.approved_count, 1);
    assert_eq!(view.pending_count, 1);
}

#[test]
fn seats_left_join_ballots_and_count_by_vote() {
    let roster = vec![profile("a", "甲"), profile("b", "乙"), profile("c", "丙")];
    let mut ballots = HashMap::new();
    ballots.insert(ballot_key("a", "MEMBER"), true);
    ballots.insert(ballot_key("b", "MEMBER"), false);
    // 丙(0xcc)未投。

    let view = build_active_proposal_view(sample_state(), "institution-a", &roster, &ballots);

    assert_eq!(view.seats.len(), 3);
    assert_eq!(view.seats[0].vote, Some(true));
    assert_eq!(view.seats[1].vote, Some(false));
    assert_eq!(view.seats[2].vote, None);
    assert_eq!(view.approved_count, 1);
    assert_eq!(view.rejected_count, 1);
    assert_eq!(view.pending_count, 1);
    assert_eq!(view.state.proposal_id, 7);
}

#[test]
fn empty_roster_yields_no_seats() {
    let view = build_active_proposal_view(sample_state(), "institution-a", &[], &HashMap::new());
    assert!(view.seats.is_empty());
    assert_eq!(view.approved_count, 0);
    assert_eq!(view.pending_count, 0);
}

#[test]
fn current_wallet_changes_or_unbinding_do_not_change_frozen_vote_identity() {
    let mut seat = profile("a", "甲");
    let ballots = HashMap::from([(ballot_key("a", "MEMBER"), true)]);
    let original =
        build_active_proposal_view(sample_state(), "institution-a", &[seat.clone()], &ballots);
    seat.account_id = None;
    let unbound = build_active_proposal_view(sample_state(), "institution-a", &[seat], &ballots);
    assert_eq!(original.seats[0].ticket_id, unbound.seats[0].ticket_id);
    assert_eq!(unbound.seats[0].vote, Some(true));
    assert!(unbound.seats[0].account_id.is_none());
}
