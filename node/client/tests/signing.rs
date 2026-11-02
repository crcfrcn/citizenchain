use crate::signing::account_id_to_ss58;
use crate::signing::classify_invalid_tx;
use crate::signing::dry_run_reject_message;
use crate::signing::encode_compact_u32;
use crate::signing::now_secs;
use crate::signing::sha256_hash;

#[test]
fn encode_compact_u32_single_byte() {
    assert_eq!(encode_compact_u32(0), vec![0x00]);
    assert_eq!(encode_compact_u32(1), vec![0x04]);
    assert_eq!(encode_compact_u32(63), vec![0xfc]);
}

#[test]
fn encode_compact_u32_two_bytes() {
    assert_eq!(encode_compact_u32(64), vec![0x01, 0x01]);
}

#[test]
fn sha256_hash_deterministic() {
    let h1 = sha256_hash(b"hello");
    let h2 = sha256_hash(b"hello");
    assert_eq!(h1, h2);
    assert_ne!(h1, sha256_hash(b"world"));
}

#[test]
fn account_id_to_ss58_roundtrip() {
    let account_id = [0xAAu8; 32];
    let ss58 = account_id_to_ss58(&account_id).unwrap();
    assert!(!ss58.is_empty());
    // 验证可以用 bs58 解码回来
    let decoded = bs58::decode(&ss58).into_vec().unwrap();
    assert_eq!(&decoded[2..34], &account_id);
}

#[test]
fn classify_invalid_tx_known_variants() {
    // 0x01 00 xx = Err(InvalidTransaction::xxx)
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x02]).contains("Future"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x03]).contains("Stale"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x04]).contains("BadProof"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x01]).contains("Payment"));
}

#[test]
fn classify_invalid_tx_unknown_transaction() {
    // 0x01 01 xx = Err(UnknownTransaction::xxx)
    assert_eq!(
        classify_invalid_tx(&[0x01, 0x01, 0x00]),
        "UnknownTransaction"
    );
}

#[test]
fn classify_invalid_tx_unrecognized_code_does_not_panic() {
    // 越界/未知变体编号不得 panic，归入 Unknown
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x63]).contains("Unknown"));
    assert_eq!(classify_invalid_tx(&[0x01]), "UnknownTransaction");
}

#[test]
fn dry_run_reject_future_gives_user_hint() {
    // Future = 上一笔还没出块，前端文案必须是人话，不带技术细节
    assert_eq!(
        dry_run_reject_message(&[0x01, 0x00, 0x02], "0x010002"),
        "上一笔交易尚未出块，请稍候再试"
    );
}

#[test]
fn dry_run_reject_other_variants_keep_technical_reason() {
    // Future 之外的变体保持原有技术报错格式（含 hex 便于排查）
    let stale = dry_run_reject_message(&[0x01, 0x00, 0x03], "0x010003");
    assert!(stale.contains("交易校验失败，已拒绝提交"));
    assert!(stale.contains("Stale"));
    assert!(stale.contains("0x010003"));

    let unknown_tx = dry_run_reject_message(&[0x01, 0x01, 0x00], "0x010100");
    assert!(unknown_tx.contains("UnknownTransaction"));
}

#[test]
fn now_secs_returns_positive() {
    // 正常系统时钟下必须返回 epoch 之后的正数秒
    assert!(now_secs().unwrap() > 1_700_000_000);
}

fn recipient_metadata() -> Vec<u8> {
    use codec::Encode;
    runtime::Runtime::metadata().encode()
}

fn resolve_ethereum_recipient(
    address: &str,
    metadata: Vec<u8>,
    mapped: Result<serde_json::Value, String>,
) -> Result<[u8; 32], String> {
    use crate::signing::resolve_recipient_account_id_with_rpc;
    use serde_json::json;
    let head = format!("0x{}", "12".repeat(32));
    let mut key = sp_crypto_hashing::twox_128(b"Revive").to_vec();
    key.extend(sp_crypto_hashing::twox_128(b"OriginalAccount"));
    key.extend(hex::decode(&address.trim()[2..]).unwrap());
    let mut calls = 0;
    let result = resolve_recipient_account_id_with_rpc(address, |method, params| {
        calls += 1;
        match calls {
            1 => {
                assert_eq!((method, params), ("chain_getFinalizedHead", json!([])));
                Ok(json!(head))
            }
            2 => {
                assert_eq!((method, params), ("state_getMetadata", json!([head])));
                Ok(json!(format!("0x{}", hex::encode(&metadata))))
            }
            3 => {
                // Identity 映射键直接跟20字节地址；元数据与映射必须钉同一块。
                assert_eq!(
                    (method, params),
                    (
                        "state_getStorage",
                        json!([format!("0x{}", hex::encode(&key)), head])
                    )
                );
                mapped.clone()
            }
            _ => panic!("收款解析不应提交交易或另读 best 状态"),
        }
    });
    if result.is_ok() {
        assert_eq!(calls, 3);
    }
    result
}

#[test]
fn recipient_ss58_remains_offline_and_ethereum_invalid_input_never_reads_chain() {
    use crate::signing::resolve_recipient_account_id_with_rpc;
    let account = [0xAA; 32];
    let ss58 = account_id_to_ss58(&account).unwrap();
    assert_eq!(
        resolve_recipient_account_id_with_rpc(&format!(" {ss58} "), |_, _| panic!("SS58无需RPC"))
            .unwrap(),
        account
    );
    for address in [
        "0x",
        "0x1234",
        "0x6D66a0cB155051055F3FB3Cc73d9230D3Fb4D5BZ",
        "0x5AAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
    ] {
        assert!(
            resolve_recipient_account_id_with_rpc(address, |_, _| panic!("无效地址不得读取链"))
                .is_err()
        );
    }
    let native_hex = format!("0x{}", "aa".repeat(32));
    assert!(
        resolve_recipient_account_id_with_rpc(&native_hex, |_, _| panic!(
            "32字节hex不是Ethereum地址"
        ))
        .is_err()
    );
}

#[test]
fn recipient_ethereum_checksum_vectors_and_unmapped_accounts_match_ss58() {
    use serde_json::Value;
    // EIP-55 官方向量：全大写、全小写及混合大小写均合法。
    for address in [
        "0x52908400098527886E0F7030069857D2E4169EE7",
        "0xde709f2102306220921060314715629080e2fb77",
        "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
        "0x6D66a0cB155051055F3FB3Cc73d9230D3Fb4D5B5",
    ] {
        let account =
            resolve_ethereum_recipient(address, recipient_metadata(), Ok(Value::Null)).unwrap();
        let mut expected = [0xEE; 32];
        expected[..20].copy_from_slice(&hex::decode(&address[2..]).unwrap());
        assert_eq!(account, expected);
        let ss58 = account_id_to_ss58(&account).unwrap();
        assert_eq!(
            crate::signing::resolve_recipient_account_id(&ss58).unwrap(),
            account
        );
    }
}

#[test]
fn recipient_ethereum_uses_existing_mapping_and_never_falls_back_on_errors() {
    use serde_json::json;
    let address = "0xde709f2102306220921060314715629080e2fb77";
    assert_eq!(
        resolve_ethereum_recipient(
            address,
            recipient_metadata(),
            Ok(json!(format!("0x{}", "34".repeat(32))))
        )
        .unwrap(),
        [0x34; 32]
    );
    assert_eq!(
        resolve_ethereum_recipient(address, recipient_metadata(), Err("读取链失败".into()))
            .unwrap_err(),
        "读取链失败"
    );
    for mapped in [json!("0x1234"), json!("0xzz"), json!(false), json!({})] {
        assert!(resolve_ethereum_recipient(address, recipient_metadata(), Ok(mapped)).is_err());
    }
}

#[test]
fn recipient_ethereum_head_and_metadata_read_errors_are_preserved() {
    use serde_json::json;
    for fail_at in [1, 2] {
        let mut calls = 0;
        let error = crate::signing::resolve_recipient_account_id_with_rpc(
            "0xde709f2102306220921060314715629080e2fb77",
            |_, _| {
                calls += 1;
                if calls == fail_at {
                    Err("本机 TLS RPC 读取失败".into())
                } else {
                    Ok(json!(format!("0x{}", "12".repeat(32))))
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, "本机 TLS RPC 读取失败");
        assert_eq!(calls, fail_at);
    }
}

#[test]
fn recipient_ethereum_requires_real_revive_mapping_metadata() {
    use codec::{Decode, Encode};
    use frame_metadata::RuntimeMetadata;
    use serde_json::Value;
    let original = recipient_metadata();
    for scenario in ["missing_revive", "missing_mapping", "wrong_hasher"] {
        let mut metadata =
            frame_metadata::RuntimeMetadataPrefixed::decode(&mut &original[..]).unwrap();
        macro_rules! change {
            ($value:expr) => {{
                let value = $value;
                if scenario == "missing_revive" {
                    value.pallets.retain(|p| p.name != "Revive");
                } else {
                    let storage = value
                        .pallets
                        .iter_mut()
                        .find(|p| p.name == "Revive")
                        .unwrap()
                        .storage
                        .as_mut()
                        .unwrap();
                    if scenario == "missing_mapping" {
                        storage.entries.retain(|e| e.name != "OriginalAccount");
                    } else if let frame_metadata::v14::StorageEntryType::Map { hashers, .. } =
                        &mut storage
                            .entries
                            .iter_mut()
                            .find(|e| e.name == "OriginalAccount")
                            .unwrap()
                            .ty
                    {
                        *hashers = vec![frame_metadata::v14::StorageHasher::Blake2_128Concat];
                    }
                }
            }};
        }
        match &mut metadata.1 {
            RuntimeMetadata::V14(v) => change!(v),
            RuntimeMetadata::V15(v) => change!(v),
            RuntimeMetadata::V16(v) => change!(v),
            _ => panic!("Runtime元数据版本"),
        }
        assert!(resolve_ethereum_recipient(
            "0xde709f2102306220921060314715629080e2fb77",
            metadata.encode(),
            Ok(Value::Null)
        )
        .is_err());
    }
    assert!(resolve_ethereum_recipient(
        "0xde709f2102306220921060314715629080e2fb77",
        vec![],
        Ok(Value::Null)
    )
    .is_err());
}
