//! 原所属模块回归，断言与输入保持。
use super::*;
use codec::{Decode, Encode};

/// 索引器必须解码当前 Runtime 的真实元数据与 EventRecord，禁止仅伪造字段名测试。
fn runtime_event(
    pallet: &str,
    variant: &str,
    fields: impl Encode,
) -> subxt::events::Events<PolkadotConfig> {
    let bytes = runtime::Runtime::metadata().encode();
    let metadata = subxt::Metadata::decode(&mut bytes.as_slice()).unwrap();
    let module = metadata.pallet_by_name(pallet).unwrap();
    let event = module
        .event_variants()
        .unwrap()
        .iter()
        .find(|event| event.name == variant)
        .unwrap();
    let mut encoded = vec![module.index(), event.index];
    encoded.extend(fields.encode());
    let event = runtime::RuntimeEvent::decode(&mut encoded.as_slice()).unwrap();
    let record = (
        Phase::ApplyExtrinsic(3),
        event,
        Vec::<subxt::utils::H256>::new(),
    );
    subxt::events::Events::decode_from(vec![record].encode(), metadata)
}

#[test]
fn runtime_issuance_and_personal_events_project_real_fields() {
    let account = [0x11u8; 32];
    let other = [0x22u8; 32];
    for (events, kind, recipient) in [
        (
            runtime_event(
                "FullnodeIssuance",
                "FullnodeIssuanceIssued",
                (7u32, other, account, 123u128),
            ),
            "block_reward",
            account,
        ),
        (
            runtime_event(
                "ProvincialBankInterest",
                "ProvincialBankInterestMinted",
                (2026u32, account, 123u128),
            ),
            "bank_interest",
            account,
        ),
        (
            runtime_event(
                "CitizenIssuance",
                "CertificationRewardIssued",
                (
                    account,
                    format!(
                        "CN000-CTZN{}-100000000-2026",
                        primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
                    )
                    .into_bytes(),
                    123u128,
                ),
            ),
            "lightnode_reward",
            account,
        ),
        (
            runtime_event(
                "MultisigTransfer",
                "TransferExecuted",
                (1u64, other, other, account, 123u128, 2u128),
            ),
            "proposal_transfer",
            account,
        ),
        (
            runtime_event(
                "PersonalManage",
                "PersonalCreated",
                (1u64, account, other, 3u32, 2u32, 123u128, 2u128),
            ),
            "institution_multisig_create",
            account,
        ),
        (
            runtime_event(
                "PersonalManage",
                "PersonalClosed",
                (1u64, other, account, 3u32, 2u32, 123u128, 2u128),
            ),
            "institution_multisig_close",
            account,
        ),
    ] {
        let records = parse_block_events(&events, 7, Some(1_700_000_000_000)).unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.tx_type, kind);
        assert_eq!(
            record.recipient_account_id.as_deref(),
            Some(account_id_text(&recipient).as_str())
        );
        assert_eq!(record.amount_fen, 123);
        assert_eq!(record.extrinsic_index, Some(3));
        assert_eq!(record.event_index, 0);
    }
}

#[test]
fn empty_event_vectors_and_invalid_count_prefixes_follow_scale_decode() {
    let metadata =
        subxt::Metadata::decode(&mut runtime::Runtime::metadata().encode().as_slice()).unwrap();
    for bytes in [Vec::new(), vec![0u8]] {
        let events = subxt::events::Events::decode_from(bytes, metadata.clone());
        assert!(parse_block_events(&events, 7, None).unwrap().is_empty());
        assert_eq!(
            collect_entity_projection_cids(&events).unwrap(),
            (Vec::<String>::new(), Vec::<String>::new())
        );
    }
    for bytes in [vec![1u8], vec![2u8], vec![3u8], vec![0xffu8, 0xffu8]] {
        let events = subxt::events::Events::decode_from(bytes, metadata.clone());
        assert!(
            matches!(parse_block_events(&events, 7, None), Err(error) if error.starts_with("decode event count failed:"))
        );
        assert!(
            matches!(collect_entity_projection_cids(&events), Err(error) if error.starts_with("decode event count failed:"))
        );
    }
}

#[test]
fn overflow_corrupt_and_truncated_events_stop_the_batch() {
    let events = runtime_event(
        "ProvincialBankInterest",
        "ProvincialBankInterestMinted",
        (2026u32, [0x11u8; 32], i64::MAX as u128 + 1),
    );
    assert!(parse_block_events(&events, 7, None).is_err());
    let metadata =
        subxt::Metadata::decode(&mut runtime::Runtime::metadata().encode().as_slice()).unwrap();
    for bytes in [vec![4u8], vec![4u8, 0u8, 0u8], vec![0xffu8], vec![0u8, 1u8]] {
        let corrupt = subxt::events::Events::decode_from(bytes, metadata.clone());
        assert!(parse_block_events(&corrupt, 7, None).is_err());
        assert!(collect_entity_projection_cids(&corrupt).is_err());
    }
    let unknown = runtime_event(
        "ProvincialBankInterest",
        "ProvincialBankYearSettled",
        (2026u32, 43u32, 123u128),
    );
    assert!(parse_block_events(&unknown, 7, None).unwrap().is_empty());
    assert!(parse_block_events(&unknown, 7, Some(u64::MAX)).is_err());
}
