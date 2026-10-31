//! 原所属模块回归，断言与输入保持。
use super::*;
use codec::Encode;

#[test]
fn active_binding_requires_positive_revision() {
    let snapshot = FinalizedCitizenIdentity {
        genesis_hash: [0; 32],
        identity_version: 0,
        finalized_block_hash: [1; 32],
        finalized_block_number: 7,
        chain_now_seconds: 100,
        cid_status: FinalizedCidStatus::Active,
        registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
        commitment: Some([2; 32]),
        residence_province_code: b"GD".to_vec(),
        residence_city_code: b"001".to_vec(),
        account_id: Some([3; 32]),
        binding_revision: Some(0),
        voting: None,
        candidate: None,
    };
    assert_eq!(snapshot.active_binding(), None);
}

#[test]
fn revoked_cid_has_no_active_binding_even_when_audit_mapping_remains() {
    let snapshot = FinalizedCitizenIdentity {
        genesis_hash: [0; 32],
        identity_version: 0,
        finalized_block_hash: [1; 32],
        finalized_block_number: 7,
        chain_now_seconds: 100,
        cid_status: FinalizedCidStatus::Revoked,
        registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
        commitment: Some([2; 32]),
        residence_province_code: b"GD".to_vec(),
        residence_city_code: b"001".to_vec(),
        account_id: Some([3; 32]),
        binding_revision: Some(3),
        voting: None,
        candidate: None,
    };
    assert_eq!(snapshot.active_binding(), None);
}

#[test]
fn voting_or_candidate_requires_registry_rebind() {
    let snapshot = FinalizedCitizenIdentity {
        genesis_hash: [0; 32],
        identity_version: 0,
        finalized_block_hash: [1; 32],
        finalized_block_number: 7,
        chain_now_seconds: 100,
        cid_status: FinalizedCidStatus::Active,
        registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
        commitment: Some([2; 32]),
        residence_province_code: b"GD".to_vec(),
        residence_city_code: b"001".to_vec(),
        account_id: Some([3; 32]),
        binding_revision: Some(2),
        voting: Some(FinalizedVotingIdentity {
            passport_valid_from: 20260101,
            passport_valid_until: 20360101,
            citizen_status: 0,
            residence_province_code: b"GD".to_vec(),
            residence_city_code: b"001".to_vec(),
            residence_town_code: b"001001".to_vec(),
        }),
        candidate: None,
    };
    assert!(snapshot.registry_rebind_required());
}

#[test]
fn finalized_storage_decode_rejects_trailing_layout_bytes() {
    let mut encoded_record = (
        b"CN001-CREG-0001".to_vec(),
        [0x44u8; 32],
        Vec::<u8>::new(),
        Vec::<u8>::new(),
        0u8,
        42u32,
        Option::<u32>::None,
    )
        .encode();
    let decoded: RawCidRecord = match decode_all(&encoded_record, "CidRegistry fixture") {
        Ok(value) => value,
        Err(error) => panic!("exact record should decode: {error}"),
    };
    assert_eq!(decoded.status, 0);
    encoded_record.push(0xff);
    assert!(decode_all::<RawCidRecord>(&encoded_record, "CidRegistry fixture").is_err());
}

#[test]
fn timestamp_may_only_be_missing_at_genesis() {
    assert_eq!(finalized_chain_now_millis(0, None), Ok(0));
    assert_eq!(finalized_chain_now_millis(7, Some(12_000)), Ok(12_000));
    assert!(finalized_chain_now_millis(7, None).is_err());
}
