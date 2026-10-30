use crate::display::format_fee_rate_summary;
use crate::query::encode_compact_u32;
use crate::query::split_action_into_details;
use crate::types::FeeRateProposalDetail;
use codec::Encode;
use nodeclient::codec::truncate_chars;
use nodedestroy::summary::format_destroy_summary;
use nodedestroy::types::ResolutionDestroyDetail;
use noderesolutionissue::decode::decode_resolution_issuance_action;
use noderesolutionissue::decode::TAG_RESOLUTION_ISSUANCE;
use noderesolutionissue::summary::format_issuance_summary;
use noderesolutionissue::types::IssuanceAllocationItem;
use noderesolutionissue::types::ResolutionIssuanceDetail;
use nodeupgrade::decode::decode_runtime_upgrade_action;
use nodeupgrade::decode::TAG_RUNTIME_UPGRADE;
use nodeupgrade::summary::format_runtime_upgrade_summary;
use nodeupgrade::types::RuntimeUpgradeDetail;

fn compact_bytes_for_test(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() < 64, "test helper only supports short vectors");
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push((bytes.len() as u8) << 2);
    out.extend_from_slice(bytes);
    out
}

#[test]
fn format_runtime_upgrade_summary_truncates_long_reason() {
    let long = "a".repeat(80);
    let d = RuntimeUpgradeDetail {
        proposal_id: 2,
        proposer_account_id: String::new(),
        reason: long,
        code_hash_hex: String::new(),
        expected_pow_hash: String::new(),
        pow_params: difficulty::PowDifficultyParams::genesis_default(),
    };
    let summary = format_runtime_upgrade_summary(&d);
    assert!(summary.starts_with("协议升级："));
    assert!(summary.contains("…"));
}

#[test]
fn decode_runtime_upgrade_action_uses_current_summary_layout() {
    let mut data = Vec::from(TAG_RUNTIME_UPGRADE);
    data.extend_from_slice(&compact_bytes_for_test(b"LN001-NRC0G-944805165-2026"));
    data.extend_from_slice(&[7u8; 32]);
    data.extend_from_slice(&compact_bytes_for_test("升级".as_bytes()));
    data.extend_from_slice(&[9u8; 32]);
    data.extend_from_slice(&[8u8; 32]);
    data.extend_from_slice(&difficulty::PowDifficultyParams::genesis_default().encode());

    let detail = decode_runtime_upgrade_action(10, &data).expect("current layout should decode");
    assert_eq!(detail.proposal_id, 10);
    assert_eq!(detail.reason, "升级");
    assert_eq!(detail.code_hash_hex, "09".repeat(32));

    data.push(0);
    assert!(
        decode_runtime_upgrade_action(10, &data).is_none(),
        "old upgrade summary status field must not be accepted"
    );
}

#[test]
fn format_issuance_summary_shows_count_and_amount() {
    let d = ResolutionIssuanceDetail {
        proposal_id: 3,
        actor_cid_number: String::new(),
        proposer_role_code: String::new(),
        proposer_account_id: String::new(),
        reason: "春节福利".to_string(),
        total_amount_fen: "100000".to_string(), // 1000 元
        allocations: vec![
            IssuanceAllocationItem {
                recipient_account_id: "a".repeat(64),
                amount_fen: "50000".to_string(),
            },
            IssuanceAllocationItem {
                recipient_account_id: "b".repeat(64),
                amount_fen: "50000".to_string(),
            },
        ],
    };
    assert_eq!(
        format_issuance_summary(&d),
        "决议发行 1000.00 元（2条分配）：春节福利"
    );
}

#[test]
fn decode_resolution_issuance_action_requires_explicit_role_code() {
    let mut data = Vec::from(TAG_RESOLUTION_ISSUANCE);
    data.extend_from_slice(&compact_bytes_for_test(b"LN001-NRC0G-944805165-2026"));
    data.extend_from_slice(&compact_bytes_for_test(b"COMMITTEE_MEMBER"));
    data.extend_from_slice(&[7u8; 32]);
    data.extend_from_slice(&compact_bytes_for_test("救灾发行".as_bytes()));
    data.extend_from_slice(&100_000u128.to_le_bytes());
    data.extend_from_slice(&encode_compact_u32(1));
    data.extend_from_slice(&[8u8; 32]);
    data.extend_from_slice(&100_000u128.to_le_bytes());

    let detail = decode_resolution_issuance_action(11, &data)
        .expect("explicit institution role layout should decode");
    assert_eq!(detail.actor_cid_number, "LN001-NRC0G-944805165-2026");
    assert_eq!(detail.proposer_role_code, "COMMITTEE_MEMBER");
    assert_eq!(detail.reason, "救灾发行");
    assert_eq!(detail.allocations.len(), 1);

    data.push(0);
    assert!(
        decode_resolution_issuance_action(11, &data).is_none(),
        "trailing legacy fields must be rejected"
    );
}

#[test]
fn format_destroy_summary_falls_back_to_unknown_institution() {
    let d = ResolutionDestroyDetail {
        proposal_id: 4,
        actor_cid_number: String::new(),
        institution_account_id: format!("0x{}", "00".repeat(32)),
        amount_fen: "50000".to_string(),
    };
    assert_eq!(format_destroy_summary(&d), "决议销毁 500.00 元：未知机构");
}

#[test]
fn format_fee_rate_summary_shows_percent() {
    let d = FeeRateProposalDetail {
        proposal_id: 5,
        actor_cid_number: String::new(),
        institution_account_id: format!("0x{}", "00".repeat(32)),
        new_rate_bp: 150, // 1.50%
    };
    assert_eq!(format_fee_rate_summary(&d), "费率设置 1.50%：未知机构");
}

#[test]
fn split_action_into_details_maps_each_variant() {
    let detail = RuntimeUpgradeDetail {
        proposal_id: 1,
        proposer_account_id: String::new(),
        reason: "升级".to_string(),
        code_hash_hex: String::new(),
        expected_pow_hash: String::new(),
        pow_params: difficulty::PowDifficultyParams::genesis_default(),
    };
    let (dq, ru, ri, rd, fr) = split_action_into_details(
        crate::types::ProposalAction::RuntimeUpgrade(Box::new(detail)),
    );
    let _ = dq;
    assert!(ru.is_some());
    assert!(ri.is_none() && rd.is_none() && fr.is_none());

    let (_, ru2, _, _, _) = split_action_into_details(crate::types::ProposalAction::Unknown);
    assert!(ru2.is_none());
}
