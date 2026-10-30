use super::*;

use crate::guard::runtime_policy::check_block;
use crate::guard::runtime_policy::check_max_rate;
use crate::guard::runtime_policy::check_rate_value;
use crate::guard::runtime_policy::decode_events;
use crate::guard::runtime_policy::offchain_fee;
use crate::guard::runtime_policy::variant;
use crate::guard::runtime_policy::OFFCHAIN_MAX_RATE_UNITS;
use frame_system::EventRecord;
use runtime::RuntimeEvent;
use sp_runtime::traits::Header as _;
use sp_runtime::AccountId32;

fn local_metadata() -> Vec<u8> {
    runtime::Runtime::metadata().into()
}

fn unit_code_updated_metadata() -> Vec<u8> {
    let metadata = local_metadata();
    let mut input = metadata.as_slice();
    let mut prefixed = RuntimeMetadataPrefixed::decode(&mut input).expect("metadata decodes");
    assert!(input.is_empty());
    let (types, event_type) = match &mut prefixed.1 {
        RuntimeMetadata::V14(meta) => (
            &mut meta.types,
            meta.pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .unwrap()
                .event
                .as_ref()
                .unwrap()
                .ty
                .id,
        ),
        RuntimeMetadata::V15(meta) => (
            &mut meta.types,
            meta.pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .unwrap()
                .event
                .as_ref()
                .unwrap()
                .ty
                .id,
        ),
        RuntimeMetadata::V16(meta) => (
            &mut meta.types,
            meta.pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .unwrap()
                .event
                .as_ref()
                .unwrap()
                .ty
                .id,
        ),
        _ => panic!("unsupported metadata version"),
    };
    let event = types
        .types
        .iter_mut()
        .find(|entry| entry.id == event_type)
        .expect("System event type exists");
    let scale_info::TypeDef::Variant(variants) = &mut event.ty.type_def else {
        panic!("System events are variants");
    };
    let code_updated = variants
        .variants
        .iter_mut()
        .find(|variant| variant.name == "CodeUpdated")
        .expect("CodeUpdated exists");
    assert_eq!(code_updated.fields.len(), 1);
    code_updated.fields.clear();
    prefixed.encode()
}

fn signed_transfer(amount: u128) -> (Vec<sp_runtime::OpaqueExtrinsic>, AccountId32) {
    let pair = sr25519::Pair::from_seed(&[7u8; 32]);
    let payer_account_id = chain_signing::account_id_from_public(pair.public());
    let call = RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
        beneficiary_account_id: AccountId32::new([8u8; 32]),
        amount,
        remark: b"fee-test".to_vec().try_into().expect("remark fits"),
    });
    let xt =
        chain_signing::build_signed_extrinsic_local(call, sp_core::H256::repeat_byte(1), 0, &pair);
    (vec![xt.into()], payer_account_id)
}

#[test]
fn node_fixed_fee_values_are_exact() {
    assert_eq!(primitives::fee_policy::calculate_onchain_fee(1), 10);
    assert_eq!(primitives::fee_policy::calculate_onchain_fee(50_000), 50);
    assert_eq!(primitives::fee_policy::VOTE_FLAT_FEE, 100);
    assert_eq!(primitives::fee_policy::OFFCHAIN_MIN_FEE, 1);
    assert_eq!(offchain_fee(50_000, OFFCHAIN_MAX_RATE_UNITS), 50);
}

#[test]
fn rates_above_zero_point_one_percent_are_rejected() {
    assert!(check_rate_value(0).is_ok());
    assert!(check_rate_value(10).is_ok());
    assert!(check_rate_value(11).is_err());
    assert!(check_max_rate(Some(11u32.encode())).is_err());
}

#[test]
fn actual_onchain_fee_event_must_match_fixed_formula() {
    let (body, payer_account_id) = signed_transfer(50_000);
    let metadata = local_metadata();
    let good_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
        phase: Phase::ApplyExtrinsic(0),
        event: RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
            account_id: payer_account_id.clone(),
            fee: 50,
        }),
        topics: Vec::new(),
    }];
    let events_key = crate::guard::runtime_policy::storage_key::events();
    assert!(check_block(
        &body,
        |key| { (key == events_key.as_slice()).then(|| good_events.encode()) },
        &metadata
    )
    .is_ok());

    let bad_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
        phase: Phase::ApplyExtrinsic(0),
        event: RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
            account_id: payer_account_id,
            fee: 49,
        }),
        topics: Vec::new(),
    }];
    assert!(check_block(
        &body,
        |key| { (key == events_key.as_slice()).then(|| bad_events.encode()) },
        &metadata
    )
    .is_err());
}

#[test]
fn actual_offchain_settlement_uses_recipient_rate_and_one_fen_minimum() {
    let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    let metadata = local_metadata();
    let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
        phase: Phase::ApplyExtrinsic(0),
        event: RuntimeEvent::OffchainTransaction(offchain::pallet::Event::PaymentSettled {
            tx_id: sp_core::H256::repeat_byte(4),
            payer_account_id: AccountId32::new([5u8; 32]),
            payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
            recipient_account_id: AccountId32::new([7u8; 32]),
            recipient_bank_cid: bank.clone().try_into().unwrap(),
            transfer_amount: 1,
            fee_amount: 1,
        }),
        topics: Vec::new(),
    }];
    let events_key = crate::guard::runtime_policy::storage_key::events();
    let rate_key = crate::guard::runtime_policy::storage_key::rate(&bank);
    assert!(check_block(
        &[],
        |key| {
            if key == events_key.as_slice() {
                Some(events.encode())
            } else if key == rate_key.as_slice() {
                Some(10u32.encode())
            } else {
                None
            }
        },
        &metadata
    )
    .is_ok());

    let wrong_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
        phase: Phase::ApplyExtrinsic(0),
        event: RuntimeEvent::OffchainTransaction(offchain::pallet::Event::PaymentSettled {
            tx_id: sp_core::H256::repeat_byte(4),
            payer_account_id: AccountId32::new([5u8; 32]),
            payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
            recipient_account_id: AccountId32::new([7u8; 32]),
            recipient_bank_cid: bank.try_into().unwrap(),
            transfer_amount: 1,
            fee_amount: 2,
        }),
        topics: Vec::new(),
    }];
    assert!(check_block(
        &[],
        |key| {
            if key == events_key.as_slice() {
                Some(wrong_events.encode())
            } else if key == rate_key.as_slice() {
                Some(10u32.encode())
            } else {
                None
            }
        },
        &metadata
    )
    .is_err());
}

#[test]
fn upgrade_block_events_use_parent_runtime_schema_without_skipping_fee_checks() {
    let metadata = local_metadata();
    let old_metadata = unit_code_updated_metadata();
    let (body, payer) = signed_transfer(50_000);
    let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![
        EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                account_id: payer,
                fee: 50,
            }),
            topics: Vec::new(),
        },
        EventRecord {
            phase: Phase::ApplyExtrinsic(1),
            event: RuntimeEvent::System(frame_system::Event::CodeUpdated {
                hash: sp_core::H256::repeat_byte(0xA5),
            }),
            topics: Vec::new(),
        },
    ];
    let mut old_encoding = events.encode();
    let hash = [0xA5; 32];
    let offset = old_encoding
        .windows(hash.len())
        .position(|window| window == hash)
        .expect("CodeUpdated hash appears exactly once");
    old_encoding.drain(offset..offset + hash.len());
    let events_key = crate::guard::runtime_policy::storage_key::events();
    let old_read = |key: &[u8]| (key == events_key.as_slice()).then(|| old_encoding.clone());
    assert_eq!(decode_events(&old_read, &old_metadata).unwrap().len(), 1);
    assert!(decode_events(&old_read, &metadata).is_err());
    assert!(check_block(&body, old_read, &old_metadata).is_ok());

    let mut wrong = events.clone();
    wrong[0].event = RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
        account_id: AccountId32::new([0; 32]),
        fee: 49,
    });
    let mut wrong_old_encoding = wrong.encode();
    let offset = wrong_old_encoding
        .windows(hash.len())
        .position(|window| window == hash)
        .expect("CodeUpdated hash appears exactly once");
    wrong_old_encoding.drain(offset..offset + hash.len());
    assert!(check_block(
        &body,
        |key| (key == events_key.as_slice()).then(|| wrong_old_encoding.clone()),
        &old_metadata,
    )
    .is_err());

    let new_encoding = events.encode();
    assert_eq!(
        decode_events(
            &|key| (key == events_key.as_slice()).then(|| new_encoding.clone()),
            &metadata,
        )
        .unwrap()
        .len(),
        1
    );
    let truncated = &old_encoding[..old_encoding.len() - 1];
    assert!(decode_events(
        &|key| (key == events_key.as_slice()).then(|| truncated.to_vec()),
        &old_metadata,
    )
    .is_err());
    let mut trailing = old_encoding;
    trailing.push(0);
    assert!(decode_events(
        &|key| (key == events_key.as_slice()).then(|| trailing.clone()),
        &old_metadata,
    )
    .is_err());
}

/// 字节锁:清算体系 CID 存储键必须 = twox_128(pallet) || twox_128(item)
/// || blake2_128(SCALE(cid)) || SCALE(cid),其中 SCALE(cid) = Compact(len)||bytes
/// (与 runtime `StorageMap<_, Blake2_128Concat, InstitutionCidNumber, _>` 逐字节一致)。
/// 若回退成旧的定长 32B 账户编码(节点会静默读空),本测试必红。
#[test]
fn cid_storage_keys_use_compact_len_prefix() {
    let cid: Vec<u8> = b"AH001-SCB05-000000002-2026".to_vec();
    let encoded = cid.encode(); // Compact(26)=0x68 || 26 bytes
    assert_eq!(encoded[0], 0x68, "26<<2 = 0x68 单字节 compact 前缀");
    assert_eq!(&encoded[1..], &cid[..]);

    let mut want = Vec::new();
    want.extend_from_slice(&sp_io::hashing::twox_128(b"OffchainTransaction"));
    want.extend_from_slice(&sp_io::hashing::twox_128(b"LastClearingBatchSeq"));
    want.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    want.extend_from_slice(&encoded);
    assert_eq!(
        crate::guard::runtime_policy::storage_key::last_batch(&cid),
        want
    );

    // BankTotalDeposits 键尾同为 Compact(len)||bytes CID。
    assert!(crate::guard::runtime_policy::storage_key::bank_total(&cid).ends_with(&encoded));

    // DepositBalance 双 map:一级 CID 段(变长)必现于键内,证明非定长 32B 账户编码。
    let acc = AccountId32::new([0x09; 32]);
    assert!(
        crate::guard::runtime_policy::storage_key::deposit(&cid, &acc)
            .windows(encoded.len())
            .any(|w| w == encoded.as_slice())
    );
}
