//! 以真实metadata和交易事件验证固定费用及受保护账户政策。

use super::*;

use crate::guard::runtime_policy::check_block;
use crate::guard::runtime_policy::check_max_rate;
use crate::guard::runtime_policy::check_rate_value;
use crate::guard::runtime_policy::decode_events;
use crate::guard::runtime_policy::offchain_fee;
use crate::guard::runtime_policy::OFFCHAIN_MAX_RATE_UNITS;
use frame_system::EventRecord;
use runtime::RuntimeEvent;
use sp_core::{sr25519, Pair as _};
use sp_runtime::AccountId32;

fn local_metadata() -> Vec<u8> {
    let raw=runtime::Runtime::metadata_at_version(16).expect("V16 metadata exists").to_vec();
    let mut metadata=nodeclient::codec::decode_runtime_metadata(&raw).unwrap();
    let RuntimeMetadata::V16(meta)=&mut metadata else {panic!("V16");};
    meta.pallets.retain(|p|p.name!="Utility");
    for pallet in &mut meta.pallets {pallet.constants.retain(|c|c.name!="TransferFeeTiming");}
    RuntimeMetadataPrefixed(frame_metadata::META_RESERVED,metadata).encode()
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

fn signed_transfer(amount: u128, remark: &[u8]) -> (Vec<sp_runtime::OpaqueExtrinsic>, AccountId32) {
    let pair = sr25519::Pair::from_seed(&[7u8; 32]);
    let payer_account_id = chain_signing::account_id_from_public(pair.public());
    let call = RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
        beneficiary_account_id: AccountId32::new([8u8; 32]),
        amount,
        remark: remark.to_vec().try_into().expect("remark fits"),
    });
    let xt =
        chain_signing::build_signed_extrinsic_local(call, sp_core::H256::repeat_byte(1), 0, &pair);
    (vec![xt.into()], payer_account_id)
}

#[test]
fn node_fixed_fee_values_are_exact() {
    assert_eq!(primitives::fee_policy::calculate_onchain_fee(1), 10);
    assert_eq!(primitives::fee_policy::calculate_onchain_fee(50_000), 50);
    assert_eq!(primitives::fee_policy::FLAT_FEE, 100);
    assert_eq!(primitives::fee_policy::OFFCHAIN_MIN_FEE, 1);
    assert_eq!(offchain_fee(50_000, OFFCHAIN_MAX_RATE_UNITS), 50);
}

#[test]
fn ordinary_remark_fee_and_historical_blocks_use_the_parent_metadata() {
    for legacy in [false, true] {
        let mut metadata = nodeclient::codec::decode_runtime_metadata(&local_metadata()).unwrap();
        let RuntimeMetadata::V16(meta) = &mut metadata else {
            panic!("V16 required")
        };
        if legacy {
            meta.pallets
                .iter_mut()
                .find(|p| p.name == "Onchain")
                .unwrap()
                .constants
                .iter_mut()
                .find(|c| c.name == "FLAT_FEE")
                .unwrap()
                .name = "VoteFlatFee".into();
        }
        let metadata = RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, metadata).encode();
        for remark in [
            vec![],
            b"a".to_vec(),
            "中".as_bytes().to_vec(),
            b" ".to_vec(),
            b"\n".to_vec(),
            vec![b'a'; 99],
        ] {
            let (body, payer) = signed_transfer(50_000, &remark);
            let expected = 50
                + if !legacy && !remark.is_empty() {
                    primitives::fee_policy::FLAT_FEE
                } else {
                    0
                };
            for actual in [expected - 1, expected, expected + 1] {
                let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
                    phase: Phase::ApplyExtrinsic(0),
                    topics: vec![],
                    event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
                        account_id: payer.clone(),
                        fee: actual,
                    }),
                }];
                let key = storage_key::events();
                assert_eq!(
                    check_block(&body, |at| (at == key).then(|| events.encode()), &metadata)
                        .is_ok(),
                    actual == expected
                );
                if actual == expected {
                    for (account_id, fee) in [
                        (payer.clone(), expected),
                        (payer.clone(), expected + 1),
                        (AccountId32::new([0x99; 32]), expected),
                    ] {
                        let extra = EventRecord {
                            phase: Phase::ApplyExtrinsic(0),
                            topics: vec![],
                            event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
                                account_id,
                                fee,
                            }),
                        };
                        let mut duplicated = vec![events[0].clone(), extra];
                        for _ in 0..2 {
                            assert!(check_block(
                                &body,
                                |at| (at == key).then(|| duplicated.encode()),
                                &metadata,
                            )
                            .is_err());
                            duplicated.reverse();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn voting_keeps_legal_business_execution_fee_events() {
    for execution_fee in [50, 150] {
        let pair = sr25519::Pair::from_seed(&[7; 32]);
        let payer = chain_signing::account_id_from_public(pair.public());
        let call = RuntimeCall::InternalVote(internalvote::pallet::Call::cast {
            proposal_id: 7,
            ticket_claim: internalvote::InternalVoteTicketClaim::Personal,
            approve: true,
        });
        let body = vec![chain_signing::build_signed_extrinsic_local(
            call,
            sp_core::H256::repeat_byte(1),
            0,
            &pair,
        )
        .into()];
        let mut events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = [
            (payer, primitives::fee_policy::FLAT_FEE),
            (AccountId32::new([0x99; 32]), execution_fee),
        ]
        .into_iter()
        .map(|(account_id, fee)| EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            topics: vec![],
            event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid { account_id, fee }),
        })
        .collect();
        let key = storage_key::events();
        let metadata = local_metadata();
        for _ in 0..2 {
            assert!(
                check_block(&body, |at| (at == key).then(|| events.encode()), &metadata).is_ok()
            );
            events.reverse();
        }
    }
}

#[test]
fn shared_fee_metadata_rejects_wrong_values_missing_duplicate_and_policy_rollback() {
    let mut metadata = nodeclient::codec::decode_runtime_metadata(&local_metadata()).unwrap();
    assert_eq!(remark_fee_enabled(&metadata, None).unwrap(), true);
    assert_eq!(remark_fee_enabled(&metadata, Some(true)).unwrap(), true);
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        panic!("V16 required")
    };
    let constants = &mut meta
        .pallets
        .iter_mut()
        .find(|p| p.name == "Onchain")
        .unwrap()
        .constants;
    let index = constants.iter().position(|c| c.name == "FLAT_FEE").unwrap();
    constants[index].name = "VoteFlatFee".into();
    assert_eq!(remark_fee_enabled(&metadata, Some(false)).unwrap(), false);
    assert!(remark_fee_enabled(&metadata, Some(true)).is_err());
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        unreachable!()
    };
    let constants = &mut meta
        .pallets
        .iter_mut()
        .find(|p| p.name == "Onchain")
        .unwrap()
        .constants;
    constants[index].name = "FLAT_FEE".into();
    constants[index].value = 99u128.encode();
    assert!(remark_fee_enabled(&metadata, None).is_err());
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        unreachable!()
    };
    let constants = &mut meta
        .pallets
        .iter_mut()
        .find(|p| p.name == "Onchain")
        .unwrap()
        .constants;
    constants[index].value = primitives::fee_policy::FLAT_FEE.encode();
    let mut duplicate = constants[index].clone();
    duplicate.name = "VoteFlatFee".into();
    constants.push(duplicate);
    assert!(remark_fee_enabled(&metadata, None).is_err());
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        unreachable!()
    };
    let constants = &mut meta
        .pallets
        .iter_mut()
        .find(|p| p.name == "Onchain")
        .unwrap()
        .constants;
    constants.pop();
    constants.remove(index);
    assert!(remark_fee_enabled(&metadata, None).is_err());
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
    let (body, payer_account_id) = signed_transfer(50_000, b"fee-test");
    let metadata = local_metadata();
    let good_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
        phase: Phase::ApplyExtrinsic(0),
        event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
            account_id: payer_account_id.clone(),
            fee: 150,
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
        event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
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
        event: RuntimeEvent::Offchain(offchain::pallet::Event::PaymentSettled {
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
        event: RuntimeEvent::Offchain(offchain::pallet::Event::PaymentSettled {
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
    let (body, payer) = signed_transfer(50_000, b"fee-test");
    let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![
        EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
                account_id: payer,
                fee: 150,
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
    wrong[0].event = RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
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

#[test]
fn both_ml_proofs_keep_the_actual_fixed_transfer_fee_guard() {
    let child = [9; 32];
    let payer = AccountId32::new(chain_signing::mldsa::account_id(
        &chain_signing::mldsa::public_key(&child),
    ));
    let metadata = local_metadata();
    for first in [true, false] {
        let call = RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
            beneficiary_account_id: AccountId32::new([8; 32]),
            amount: 50_000,
            remark: Vec::new().try_into().unwrap(),
        });
        let xt = chain_signing::build_pq_signed_extrinsic(
            call,
            sp_core::H256::repeat_byte(1),
            0,
            runtime::VERSION.spec_version,
            runtime::VERSION.transaction_version,
            &child,
            first,
        );
        let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            topics: vec![],
            event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
                account_id: payer.clone(),
                fee: 50,
            }),
        }];
        let key = storage_key::events();
        let body = vec![xt.into()];
        let result = check_block(&body, |at| (at == key).then(|| events.encode()), &metadata);
        assert!(result.is_ok(), "{result:?}");
        assert!(check_block(
            &body,
            |at| (at == key)
                .then(|| Vec::<EventRecord<RuntimeEvent, sp_core::H256>>::new().encode()),
            &metadata
        )
        .is_err());
    }
}

fn legacy_metadata() -> [Vec<u8>; 2] {
    use frame_metadata::{v14, v15};
    use scale_info::meta_type;
    type Legacy = sp_runtime::generic::UncheckedExtrinsic<
        runtime::TransactionAddress,
        RuntimeCall,
        runtime::Signature,
        runtime::TxExtension,
    >;
    // 旧Runtime的地址/签名合同与现行V5构造器的()/Proof不同，必须独立声明。
    let pallets = vec![
        v14::PalletMetadata {
            name: "System",
            storage: Some(v14::PalletStorageMetadata {
                prefix: "System",
                entries: vec![v14::StorageEntryMetadata {
                    name: "Events",
                    modifier: v14::StorageEntryModifier::Default,
                    ty: v14::StorageEntryType::Plain(meta_type::<
                        Vec<EventRecord<RuntimeEvent, sp_core::H256>>,
                    >()),
                    default: vec![0],
                    docs: vec![],
                }],
            }),
            calls: None,
            event: None,
            constants: vec![],
            error: None,
            index: 0,
        },
        v14::PalletMetadata {
            name: "OnchainTransaction",
            storage: None,
            calls: Some(v14::PalletCallMetadata {
                ty: meta_type::<onchain::pallet::Call<runtime::Runtime>>(),
            }),
            event: Some(v14::PalletEventMetadata {
                ty: meta_type::<onchain::pallet::Event<runtime::Runtime>>(),
            }),
            constants: vec![v14::PalletConstantMetadata {
                name: "VoteFlatFee",
                ty: meta_type::<u128>(),
                value: primitives::fee_policy::FLAT_FEE.encode(),
                docs: vec![],
            }],
            error: None,
            index: 4,
        },
    ];
    let extensions: Vec<_> =
        <runtime::TxExtension as sp_runtime::traits::TransactionExtension<RuntimeCall>>::metadata()
            .into_iter()
            .map(|ext| v14::SignedExtensionMetadata {
                identifier: ext.identifier,
                ty: ext.ty,
                additional_signed: ext.implicit,
            })
            .collect();
    let mut old14 = v14::RuntimeMetadataV14::new(
        pallets.clone(),
        v14::ExtrinsicMetadata {
            ty: meta_type::<Legacy>(),
            version: 4,
            signed_extensions: extensions.clone(),
        },
        meta_type::<runtime::Runtime>(),
    );
    let mut old15 = v15::RuntimeMetadataV15::new(
        pallets
            .into_iter()
            .map(|p| v15::PalletMetadata {
                name: p.name,
                storage: p.storage,
                calls: p.calls,
                event: p.event,
                constants: p.constants,
                error: p.error,
                index: p.index,
                docs: vec![],
            })
            .collect(),
        v15::ExtrinsicMetadata {
            version: 4,
            address_ty: meta_type::<runtime::TransactionAddress>(),
            call_ty: meta_type::<RuntimeCall>(),
            signature_ty: meta_type::<runtime::Signature>(),
            extra_ty: meta_type::<runtime::TxExtension>(),
            signed_extensions: extensions
                .into_iter()
                .map(|ext| v15::SignedExtensionMetadata {
                    identifier: ext.identifier,
                    ty: ext.ty,
                    additional_signed: ext.additional_signed,
                })
                .collect(),
        },
        meta_type::<runtime::Runtime>(),
        vec![],
        v15::OuterEnums {
            call_enum_ty: meta_type::<RuntimeCall>(),
            event_enum_ty: meta_type::<RuntimeEvent>(),
            error_enum_ty: meta_type::<runtime::RuntimeError>(),
        },
        v15::CustomMetadata {
            map: BTreeMap::new(),
        },
    );
    for types in [&mut old14.types, &mut old15.types] {
        for ty in &mut types.types {
            if matches!(
                ty.ty.path.segments.last().map(String::as_str),
                Some("RuntimeCall" | "RuntimeEvent")
            ) {
                if let scale_info::TypeDef::Variant(variants) = &mut ty.ty.type_def {
                    for variant in &mut variants.variants {
                        if variant.name == "Onchain" {
                            variant.name = "OnchainTransaction".into();
                        }
                        if variant.name == "Offchain" {
                            variant.name = "OffchainTransaction".into();
                        }
                    }
                }
            }
        }
    }
    [
        RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, RuntimeMetadata::V14(old14))
            .encode(),
        RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, RuntimeMetadata::V15(old15))
            .encode(),
    ]
}

#[test]
fn historical_v4_uses_parent_metadata_and_keeps_every_fee_constraint() {
    let pair = sr25519::Pair::from_seed(&[7; 32]);
    let payer = chain_signing::account_id_from_public(pair.public());
    let call = RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
        beneficiary_account_id: AccountId32::new([8; 32]),
        amount: 50_000,
        remark: b"historical-v4".to_vec().try_into().unwrap(),
    });
    let material = chain_signing::build_signing_material_from_call(
        call,
        sp_core::H256::repeat_byte(1),
        0,
        3,
        2,
    );
    let signature = chain_signing::sign_material_with_pair(&material, &pair);
    let call_bytes = material.call.encode();
    let legacy = sp_runtime::generic::UncheckedExtrinsic::<
        runtime::TransactionAddress,
        RuntimeCall,
        runtime::Signature,
        runtime::TxExtension,
    >::new_signed(
        material.call,
        sp_runtime::MultiAddress::Id(payer.clone()),
        signature.into(),
        material.tx_ext,
    );
    let opaque = sp_runtime::OpaqueExtrinsic::try_from_encoded_extrinsic(&legacy.encode()).unwrap();
    assert_eq!(opaque.inner()[0], 0x84);
    // 合成旧Runtime的不同pallet索引，证明不能借用当前Rust枚举解释历史字节。
    let mut raw = opaque.inner().to_vec();
    let call_offset = raw.len() - call_bytes.len();
    assert_eq!(&raw[call_offset..], call_bytes);
    assert_eq!(raw[call_offset], 4);
    raw[call_offset] = 254;
    let opaque = sp_runtime::OpaqueExtrinsic::from_blob(raw);
    assert!(UncheckedExtrinsic::decode_unprefixed(opaque.inner()).is_err());
    let timestamp: UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::new_bare(
        RuntimeCall::Timestamp(pallet_timestamp::Call::set { now: 1 }),
    )
    .into();
    let body = vec![timestamp.into(), opaque];
    let key = storage_key::events();
    for metadata in legacy_metadata() {
        let mut prefixed = RuntimeMetadataPrefixed::decode(&mut metadata.as_slice()).unwrap();
        let types = match &mut prefixed.1 {
            RuntimeMetadata::V14(meta) => {
                meta.extrinsic.version = 4;
                meta.pallets
                    .iter_mut()
                    .find(|pallet| pallet.name == "OnchainTransaction")
                    .unwrap()
                    .index = 254;
                &mut meta.types
            }
            RuntimeMetadata::V15(meta) => {
                meta.extrinsic.version = 4;
                meta.pallets
                    .iter_mut()
                    .find(|pallet| pallet.name == "OnchainTransaction")
                    .unwrap()
                    .index = 254;
                &mut meta.types
            }
            _ => panic!("legacy metadata required"),
        };
        for ty in &mut types.types {
            if matches!(
                ty.ty.path.segments.last().map(String::as_str),
                Some("RuntimeCall" | "RuntimeEvent")
            ) {
                let scale_info::TypeDef::Variant(variants) = &mut ty.ty.type_def else {
                    panic!("outer enum required")
                };
                variants
                    .variants
                    .iter_mut()
                    .find(|variant| variant.name == "OnchainTransaction")
                    .unwrap()
                    .index = 254;
            }
        }
        let metadata = prefixed.encode();
        let event = |account_id, fee, index| {
            let mut encoded = vec![EventRecord {
                phase: Phase::ApplyExtrinsic(index),
                topics: Vec::<sp_core::H256>::new(),
                event: RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid { account_id, fee }),
            }]
            .encode();
            assert_eq!(encoded[6], 4);
            encoded[6] = 254;
            encoded
        };
        let good = event(payer.clone(), 50, 1);
        check_block(&body, |at| (at == key).then(|| good.clone()), &metadata)
            .expect("历史V4须按父schema验证费用");
        for wrong in [
            event(payer.clone(), 49, 1),
            event(AccountId32::new([0; 32]), 50, 1),
            event(payer.clone(), 50, 0),
            Vec::<EventRecord<RuntimeEvent, sp_core::H256>>::new().encode(),
        ] {
            assert!(
                check_block(&body, |at| (at == key).then(|| wrong.clone()), &metadata).is_err()
            );
        }
    }
}

#[test]
fn transaction_envelopes_and_metadata_fail_closed() {
    let metadata = local_metadata();
    let (body, _) = signed_transfer(50_000, b"fee-test");
    let raw = body[0].inner();
    assert_eq!(&raw[..2], &[0x45, 1]);
    assert!(decode_body(&body, &metadata).is_ok());
    let mut trailing = raw.to_vec();
    trailing.push(0);
    let mut unknown_extension = raw.to_vec();
    unknown_extension[1] = 255;
    let mut bad_format = raw.to_vec();
    bad_format[0] = 0x46;
    let mut bad_kind = raw.to_vec();
    bad_kind[0] = 0xc5;
    for invalid in [
        Vec::new(),
        raw[..1].to_vec(),
        raw[..raw.len() - 1].to_vec(),
        trailing,
        unknown_extension,
        bad_format,
        bad_kind,
    ] {
        assert!(decode_body(
            &[sp_runtime::OpaqueExtrinsic::from_blob(invalid)],
            &metadata
        )
        .is_err());
    }
    let mut trailing_metadata = metadata.clone();
    trailing_metadata.push(0);
    for invalid in [
        Vec::new(),
        metadata[..metadata.len() - 1].to_vec(),
        trailing_metadata,
    ] {
        assert!(decode_body(&body, &invalid).is_err());
    }
    let mut prefixed = RuntimeMetadataPrefixed::decode(&mut metadata.as_slice()).unwrap();
    let RuntimeMetadata::V16(meta) = &mut prefixed.1 else {
        panic!("V16 required")
    };
    meta.extrinsic
        .transaction_extensions_by_version
        .insert(1, vec![codec::Compact(u32::MAX)]);
    assert!(decode_body(&body, &prefixed.encode()).is_err());
}

#[test]
fn protected_call_cannot_omit_payer_or_use_undeclared_authentication() {
    let metadata = local_metadata();
    let call = RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
        beneficiary_account_id: AccountId32::new([8; 32]),
        amount: 50_000,
        remark: Vec::new().try_into().unwrap(),
    });
    let bare: UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::new_bare(call).into();
    assert!(decode_body(&[bare.into()], &metadata).is_err());
    let (body, _) = signed_transfer(50_000, b"fee-test");
    let mut prefixed = RuntimeMetadataPrefixed::decode(&mut metadata.as_slice()).unwrap();
    let RuntimeMetadata::V16(meta) = &mut prefixed.1 else {
        panic!("V16 required")
    };
    let auth = meta
        .extrinsic
        .transaction_extensions
        .iter_mut()
        .find(|ext| ext.identifier == "AccountAuth")
        .unwrap();
    auth.identifier = "UnknownAuth".into();
    assert!(decode_body(&body, &prefixed.encode()).is_err());
    let mut legacy = runtime::Runtime::metadata();
    let RuntimeMetadata::V14(meta) = &mut legacy.1 else {
        panic!("V14 required")
    };
    meta.types.types[meta.extrinsic.ty.id as usize]
        .ty
        .type_params
        .retain(|param| param.name != "Call");
    assert!(decode_body(&body, &legacy.encode()).is_err());
}

#[test]
fn duplicate_current_and_historical_fee_pallets_are_rejected() {
    let mut metadata = nodeclient::codec::decode_runtime_metadata(&local_metadata()).unwrap();
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        panic!("V16 required")
    };
    let mut duplicate = meta
        .pallets
        .iter()
        .find(|p| p.name == "Onchain")
        .unwrap()
        .clone();
    duplicate.name = "OnchainTransaction".into();
    meta.pallets.push(duplicate);
    assert!(remark_fee_enabled(&metadata, None).is_err());
}

#[test]
fn historical_settlement_uses_only_metadata_storage_prefix_and_keeps_rate_limits() {
    let mut metadata = nodeclient::codec::decode_runtime_metadata(&local_metadata()).unwrap();
    let RuntimeMetadata::V16(meta) = &mut metadata else {
        panic!("V16 required")
    };
    let pallet = meta
        .pallets
        .iter_mut()
        .find(|p| p.name == "Offchain")
        .unwrap();
    pallet.name = "OffchainTransaction".into();
    pallet.storage.as_mut().unwrap().prefix = "OffchainTransaction".into();
    let metadata = RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, metadata).encode();
    let cid = b"fixture-bank";
    let old = storage_key::rate_in(b"OffchainTransaction", cid);
    assert_eq!(settlement_rate_key(&metadata, cid).unwrap(), old);
    assert_ne!(old, storage_key::rate(cid));
    let state = BTreeMap::from([(old.clone(), 11u32.encode())]);
    assert!(check_imported_state(state.iter()).is_err());
    assert!(
        check_transition(&BTreeMap::from([(old, Some(11u32.encode()))]), |k| state
            .get(k)
            .cloned())
        .is_err()
    );
}


#[test]
fn execution_fees_follow_real_official_batch_results_and_reject_tampered_fee() {
    use sp_runtime::{BuildStorage,traits::Dispatchable};
    for (atomic,all_failed) in [(false,false),(true,false),(false,true)] {
        let storage=runtime::RuntimeGenesisConfig::default().build_storage().unwrap();
        sp_io::TestExternalities::new(storage).execute_with(|| {
            let pair=sr25519::Pair::from_seed(&[77;32]);
            let payer=chain_signing::account_id_from_public(pair.public());
            let recipient=AccountId32::new([201;32]); let absent=AccountId32::new([202;32]);
            runtime::System::set_block_number(1);
            runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),payer.clone().into(),50_000).unwrap();
            runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),recipient.clone().into(),111).unwrap();
            runtime::System::reset_events();runtime::System::note_finished_initialize();
            let make=|to,amount|RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark{beneficiary_account_id:to,amount,remark:Default::default()});
            let calls=if all_failed {vec![make(absent.clone(),1),make(absent,1)]} else {vec![make(recipient.clone(),100),make(absent,1),make(recipient,10_500)]};
            let encoded=(27u8,if atomic {2u8}else{4u8},calls).encode();
            let call=RuntimeCall::decode(&mut encoded.as_slice()).unwrap();
            let body:Vec<sp_runtime::OpaqueExtrinsic>=vec![chain_signing::build_signed_extrinsic_local(call.clone(),sp_core::H256::repeat_byte(1),0,&pair).into()];
            let info=Default::default();
            let result=call.dispatch(runtime::RuntimeOrigin::signed(payer));
            runtime::System::note_applied_extrinsic(&result,info);
            let metadata=runtime::Runtime::metadata_at_version(16).unwrap();
            let raw=runtime::System::events().encode();
            check_block(&body,|key|(key==storage_key::events()).then(||raw.clone()),metadata.as_slice()).unwrap();
            let result=transfer_outcome(&body[0],&raw,metadata.as_slice(),0).unwrap();
            assert_eq!(result.fees,if all_failed {vec![0,0]} else if atomic {vec![0,0,0]}else{vec![10,0,11]});
            assert_eq!(result.state,if all_failed {"failed"} else if atomic {"rolled_back"}else{"partial"});
            if !atomic && !all_failed {
                let mut changed=runtime::System::events();
                for event in &mut changed {if let RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid{fee,..})=&mut event.event {*fee+=1;break;}}
                assert!(check_block(&body,|key|(key==storage_key::events()).then(||changed.encode()),metadata.as_slice()).is_err());
            }
        });
    }
}
