//! 验证提案和分离元数据解码及链上状态投影。

use super::*;
use codec::Encode;

fn body(cid: &[u8]) -> RepresentativeBody {
    entitybase::RoleSubject {
        cid_number: cid.to_vec(),
        role_code: b"MEMBER".to_vec(),
    }
}

/// Proposal 镜像从链端字段序编码的 golden 逐字节解码一致。
#[test]
fn proposal_decodes_from_field_ordered_golden() {
    let mut golden = Vec::new();
    golden.extend(2u8.encode()); // kind = 立法
    golden.extend(10u8.encode()); // stage = 院内
    golden.extend(0u8.encode()); // status = 投票中
    golden.extend(Some(*b"NRP\0").encode()); // internal_code
    golden.extend(Some(b"LN001-NRP0G-000000001-2026".to_vec()).encode()); // actor_cid_number
    golden.extend(Option::<[u8; 32]>::None.encode()); // execution_account_id
    golden.extend(vec![b"LN001-NRP0G-000000001-2026".to_vec()].encode()); // subject_cid_numbers
    golden.extend(100u32.encode()); // start
    golden.extend(200u32.encode()); // end

    let proposal = OnChainProposal::decode(&mut &golden[..]).expect("decode Proposal");
    assert_eq!(proposal.kind, 2);
    assert_eq!(proposal.stage, 10);
    assert_eq!(proposal.internal_code, Some(*b"NRP\0"));
    assert_eq!(
        proposal.actor_cid_number.as_deref(),
        Some(b"LN001-NRP0G-000000001-2026".as_slice())
    );
    assert_eq!(proposal.subject_cid_numbers.len(), 1);
    assert_eq!(proposal.start, 100);
}

/// 代表元数据与法律元数据按两个独立存储布局解码。
#[test]
fn split_meta_mirrors_decode_independently() {
    let bodies: Vec<RepresentativeBody> = vec![
        body(b"LN001-NRP0G-000000001-2026"),
        body(b"LN001-NSN0G-000000001-2026"),
    ];
    let mut representative_golden = Vec::new();
    representative_golden.extend(1u8.encode()); // Sequential
    representative_golden.extend(bodies.encode());
    representative_golden.extend(0u32.encode()); // current_body
    representative_golden.extend(1u8.encode()); // Major
    representative_golden.extend(1u8.encode()); // Legislation
    let representative = OnChainRepresentativeMeta::decode(&mut &representative_golden[..])
        .expect("decode RepresentativeMeta");
    assert_eq!(representative.current_body, 0);
    assert_eq!(representative.rule.index(), 1);
    assert_eq!(representative.route.bodies().len(), 2);

    let executive = body(b"LN001-PRS0G-000000001-2026");
    let guard = body(b"LN001-NJD0G-000000001-2026");
    let golden = (
        Some(executive.clone()),
        vec![body(b"LN001-NLG0G-000000001-2026")],
        true,
        Some(guard.clone()),
    )
        .encode();
    let legislation = OnChainLegislationMeta::decode_all(&mut golden.as_slice()).unwrap();
    assert_eq!(legislation.executive, Some(executive));
    assert_eq!(legislation.override_signers.len(), 1);
    assert_eq!(legislation.guard, Some(guard));
    assert!(legislation.needs_guard);
    let mut broken = golden.clone();
    broken.push(0);
    assert!(OnChainLegislationMeta::decode_all(&mut broken.as_slice()).is_err());
    assert!(OnChainLegislationMeta::decode_all(&mut &golden[..golden.len() - 1]).is_err());
}

/// VoteCount 镜像解码 + 组装 LegProposalState(计票加宽、houses→HouseRef)。
#[test]
fn build_state_projects_tally_and_houses() {
    let proposal = OnChainProposal {
        kind: 2,
        stage: 10,
        status: 0,
        internal_code: None,
        actor_cid_number: None,
        execution_account_id: None,
        subject_cid_numbers: Vec::new(),
        start: 100,
        end: 200,
    };
    let representative_meta = OnChainRepresentativeMeta {
        route: OnChainRepresentativeRoute::Sequential(vec![
            body(b"LN001-NRP0G-000000001-2026"),
            body(b"LN001-NSN0G-000000001-2026"),
        ]),
        current_body: 1,
        rule: OnChainRepresentativeRule::Major,
        procedure: OnChainVoteProcedure::Legislation,
    };
    let legislation_meta = OnChainLegislationMeta {
        executive: Some(body(b"LN001-PRS0G-000000001-2026")),
        override_signers: vec![body(b"LN001-NLG0G-000000001-2026")],
        guard: None,
        needs_guard: false,
    };
    let representative_tally = OnChainVoteCount32 { yes: 220, no: 30 };
    let referendum_tally = OnChainVoteCount64::default();

    let state = build_leg_proposal_state(
        7,
        &proposal,
        &representative_meta,
        &legislation_meta,
        &representative_tally,
        &referendum_tally,
    );
    assert_eq!(state.proposal_id, 7);
    assert_eq!(state.stage, 10);
    assert_eq!(state.current_body, 1);
    assert_eq!(state.representative_bodies.len(), 2);
    assert_eq!(
        state.representative_bodies[1].cid_number,
        "LN001-NSN0G-000000001-2026"
    );
    assert_eq!(state.representative_tally.yes, 220);
    assert_eq!(state.representative_tally.no, 30);
    assert_eq!(state.referendum_tally.yes, 0);
}
