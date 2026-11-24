//! 验证以太坊查询绑定准确执行区块及独立Runtime上下文。

use super::*;

use crate::ethereum::account_at;
use crate::ethereum::initialize;
use crate::ethereum::subscription_request;
use ::runtime;
use ::runtime::opaque::Block;
use codec::Encode;
use jsonrpsee::core::client::SubscriptionClientT;
use jsonrpsee::RpcModule;
use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;
use nodeclient::types::Service;
use pallet_revive::evm::Account;
use pallet_revive::evm::Bytes;
use pallet_revive::evm::GenericTransaction;
use sc_chain_spec::ChainType;
use sc_chain_spec::Properties;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ForkChoiceStrategy;
use sc_consensus::ImportResult;
use sc_consensus::StateAction;
use sc_consensus::StorageChanges;
use sc_consensus_pow::PowAlgorithm;
use sc_network::config::MultiaddrWithPeerId;
use sc_network::config::NetworkConfiguration;
use sc_network::service::traits::NetworkBlock;
use sc_network::service::traits::NetworkPeers;
use sc_network::service::traits::NetworkService;
use sc_network::service::traits::NetworkStateInfo;
use sc_network::NetworkBackend as _;
use sc_service::config::ExecutorConfiguration;
use sc_service::config::KeystoreConfig;
use sc_service::config::RpcBatchRequestConfig;
use sc_service::config::RpcConfiguration;
use sc_service::BasePath;
use sc_service::BlocksPruning;
use sc_service::Configuration;
use sc_service::DatabaseSource;
use sc_service::PruningMode;
use sc_service::Role;
use sc_service::TransactionPoolOptions;
use sp_blockchain::HeaderBackend;
use sp_core::crypto::Ss58AddressFormat;
use sp_core::crypto::Ss58Codec;
use sp_core::sr25519;
use sp_core::Pair;
use sp_core::H160;
use sp_core::U256;
use sp_keyring::Ed25519Keyring;
use sp_keyring::Sr25519Keyring;
use sp_runtime::traits::Block as BlockT;
use sp_runtime::Digest;
use sp_runtime::DigestItem;
use sp_runtime::OpaqueExtrinsic;
use std::sync::Arc;
use std::time::Duration;

fn chain_spec() -> crate::chain::ChainSpec {
    chain_spec_with_accounts(&[])
}

/// Ethereum接入回归：仅公开RPC模型和元数据，不生产区块、不提交交易。
fn reader_model(unavailable: bool) -> (RpcModule<()>, Arc<std::sync::Mutex<Vec<H256>>>) {
    let old: Vec<u8> = runtime::Runtime::metadata().into();
    let current: Vec<u8> = runtime::Runtime::metadata_at_version(16).unwrap().to_vec();
    let reads = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut module = RpcModule::new(());
    let genesis = H256::from(primitives::genesis::GENESIS_HASH);
    let hash = move |n: u32| {
        if n == 0 {
            genesis
        } else {
            H256::from_low_u64_be(n as u64)
        }
    };
    let number = move |h: H256| {
        if h == genesis {
            0
        } else {
            h.to_low_u64_be() as u32
        }
    };
    let header = move |n: u32| {
        serde_json::json!({
            "number":format!("0x{n:x}"),"parentHash":if n==0 {H256::zero()} else {hash(n-1)},
            "stateRoot":H256::repeat_byte(2),"extrinsicsRoot":H256::repeat_byte(3),"digest":{"logs":[]},
        })
    };
    module
        .register_method("chain_getBlockHash", move |params, _, _| {
            let n: u32 = params.one()?;
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(hash(n))
        })
        .unwrap();
    module
        .register_method("chain_getFinalizedHead", move |_, _, _| {
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(hash(80))
        })
        .unwrap();
    module
        .register_method("chain_getHeader", move |params, _, _| {
            let values: Vec<serde_json::Value> = params.parse()?;
            let n = values
                .first()
                .filter(|v| !v.is_null())
                .map(|v| number(serde_json::from_value(v.clone()).unwrap()))
                .unwrap_or(80);
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(if n <= 81 {
                header(n)
            } else {
                serde_json::Value::Null
            })
        })
        .unwrap();
    module.register_method("state_getRuntimeVersion", move |params,_,_| {
        let values: Vec<serde_json::Value> = params.parse()?;
        let n=values.first().filter(|v| !v.is_null()).map(|v| number(serde_json::from_value(v.clone()).unwrap())).unwrap_or(80);
        Ok::<_, jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({"specVersion":if n<80 {1}else{5},"transactionVersion":4,
            "specName":"citizenchain","implName":"citizenchain","authoringVersion":1,"implVersion":1,"apis":[],"stateVersion":1}))
    }).unwrap();
    let selected = reads.clone();
    module
        .register_method("state_getMetadata", move |params, _, _| {
            let at: H256 = params.one()?;
            selected.lock().unwrap().push(at);
            if unavailable && number(at) == 79 {
                return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                    -32000,
                    "metadata unavailable",
                    None::<()>,
                ));
            }
            Ok(sp_core::Bytes(if number(at) < 80 {
                old.clone()
            } else {
                current.clone()
            }))
        })
        .unwrap();
    module.register_method("chain_getBlock", move |params,_,_| {
        let values: Vec<serde_json::Value> = params.parse()?;
        let n=values.first().filter(|v| !v.is_null()).map(|v| number(serde_json::from_value(v.clone()).unwrap())).unwrap_or(80);
        // 读取向量保留实测旧签名边界及末字节0x8f；固定测试备注，不宣称是真实交易hash。
        let historical = concat!(
            "650384002837cdcbf8876af4cbea8bd2a2025cf978a10cd82d7c2abf1dc75ed7",
            "530467020184ba17c706d7e2d120d1fc2728446e83a847bbae22742982320d6d",
            "d1aeb14b5bb5954a9fe9d5e1d08881895ee835d9aa49721b779d14ed827f2117",
            "8ccc20288f004400000400faa94fdcba2e0f194d6c3c18c666014b108f97f8f0",
            "d7a864a9033b523130386940420f00000000000000000000000000fc",
            "e8bda6e7aa97e5a496e4b88be79d80e5a4a7e69ab4e99ba8efbc8ce68993e59ca8e8bda6e9a1b6e599bce9878ce595aae595a6e79a84e5938de280a62e2e2e"
        );
        let mut body=vec!["0x280501000b42c14765a101".to_owned()];
        if n==80 {body.push(format!("0x{historical}"));}
        if n==81 {
            let call=runtime::RuntimeCall::System(frame_system::Call::remark{remark:b"reader".to_vec()});
            let xt=chain_signing::build_signed_extrinsic_with_pair(call.clone(),genesis,0,5,4,&Sr25519Keyring::Alice.pair());
            body.push(format!("0x{}",hex::encode(xt.encode())));
            // 公开固定测试种子；只验证读取First/Stored布局，不触达链上状态。
            for (nonce,first) in [(0,true),(1,false)] {
                let xt=chain_signing::build_pq_signed_extrinsic(call.clone(),genesis,nonce,5,4,&[7;32],first);
                body.push(format!("0x{}",hex::encode(xt.encode())));
            }
        }
        Ok::<_, jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({"block":{"header":header(n),"extrinsics":body},"justifications":null}))
    }).unwrap();
    let current: Vec<u8> = runtime::Runtime::metadata_at_version(16).unwrap().to_vec();
    module
        .register_method("state_call", move |params, _, _| {
            let values: Vec<serde_json::Value> = params.parse()?;
            let method = values[0].as_str().unwrap();
            let n = values
                .get(2)
                .map(|v| number(serde_json::from_value(v.clone()).unwrap()))
                .unwrap_or(80);
            if n > 81 {
                return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                    4003,
                    "unknown block",
                    None::<()>,
                ));
            }
            let bytes = match method {
                "Metadata_metadata_at_version" => {
                    Some(sp_core::OpaqueMetadata::new(current.clone())).encode()
                }
                "ReviveApi_eth_block" if n < 80 => {
                    return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                        4003,
                        "Exported method ReviveApi_eth_block is not found",
                        None::<()>,
                    ))
                }
                "ReviveApi_eth_block" => pallet_revive::evm::Block {
                    number: n.into(),
                    hash: H256::from_low_u64_be(n as u64 + 1000),
                    gas_limit: 1_000_000u64.into(),
                    ..Default::default()
                }
                .encode(),
                "ReviveApi_eth_block_hash" => Some(H256::from_low_u64_be(n as u64 + 1000)).encode(),
                "ReviveApi_eth_receipt_data" => Vec::<ReceiptGasInfo>::new().encode(),
                "ReviveApi_balance" => panic!("展示总余额不得调用执行侧可支配余额 API"),
                "ReviveApi_nonce" => 7u32.encode(),
                "ReviveApi_gas_price" => U256::from(3).encode(),
                "ReviveApi_code" => vec![0x60u8, 0].encode(),
                "ReviveApi_get_storage" => pallet_revive::GetStorageResult::Ok(None).encode(),
                "ReviveApi_eth_estimate_gas" => {
                    Result::<U256, pallet_revive::EthTransactError>::Ok(21000u32.into()).encode()
                }
                _ => {
                    return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                        -32601,
                        method.to_owned(),
                        None::<()>,
                    ))
                }
            };
            Ok(sp_core::Bytes(bytes))
        })
        .unwrap();
    module
        .register_method("state_getStorage", |params, _, _| {
            let values: Vec<serde_json::Value> = params.parse()?;
            let key = values[0].as_str().unwrap();
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(
                if key.ends_with("80d41e5e16056765bc8461851072c9d7") {
                    Some(sp_core::Bytes(vec![0]))
                } else if key.starts_with(&format!(
                    "0x{}",
                    hex::encode(frame_support::storage::storage_prefix(
                        b"System", b"Account"
                    ))
                )) {
                    let mut account = nodeclient::codec::AccountInfo::default();
                    account.data.free = 300_111;
                    account.data.reserved = 200;
                    account.data.frozen = 500;
                    Some(sp_core::Bytes(account.encode()))
                } else {
                    None
                },
            )
        })
        .unwrap();
    module
        .register_method(
            "system_health",
            |_, _, _| serde_json::json!({"isSyncing":false,"peers":0,"shouldHavePeers":true}),
        )
        .unwrap();
    module
        .register_method("system_accountNextIndex", |_, _, _| 8u32)
        .unwrap();
    for (subscribe, unsubscribe) in [
        ("chain_subscribeNewHeads", "chain_unsubscribeNewHeads"),
        (
            "chain_subscribeFinalizedHeads",
            "chain_unsubscribeFinalizedHeads",
        ),
    ] {
        module
            .register_subscription(
                subscribe,
                "header",
                unsubscribe,
                |_, pending, _, _| async move {
                    if let Ok(sink) = pending.accept().await {
                        sink.closed().await;
                    }
                },
            )
            .unwrap();
    }
    (module, reads)
}

#[tokio::test]
async fn ethereum_reader_binds_history_to_parent_and_keeps_contexts_independent() {
    let (module, reads) = reader_model(false);
    let rpc = RpcClient::new(Native(Arc::new(module)));
    let genesis = H256::from(primitives::genesis::GENESIS_HASH);
    let old = Blocks::bind(&rpc, genesis, H256::from_low_u64_be(80))
        .await
        .unwrap();
    let old_extrinsics = old.extrinsics().await.unwrap();
    assert_eq!(old_extrinsics.len(), 2);
    assert_eq!(
        old_extrinsics.iter().nth(1).unwrap().pallet_name().unwrap(),
        "Onchain"
    );
    let current = Blocks::bind(&rpc, genesis, H256::from_low_u64_be(81))
        .await
        .unwrap();
    assert_eq!(
        current.extrinsics().await.unwrap().len(),
        4,
        "sr与ML First/Stored均可解码"
    );
    assert_eq!(
        old.extrinsics().await.unwrap().len(),
        2,
        "新上下文不得改变旧上下文"
    );
    assert_eq!(
        *reads.lock().unwrap(),
        vec![H256::from_low_u64_be(79), H256::from_low_u64_be(80)]
    );
    let metadata: Vec<u8> = runtime::Runtime::metadata_at_version(16).unwrap().to_vec();
    let wrong = OnlineClient::<SrcChainConfig>::from_rpc_client_with(
        genesis,
        subxt::client::RuntimeVersion {
            spec_version: 5,
            transaction_version: 4,
        },
        Metadata::decode_all(&mut metadata.as_slice()).unwrap(),
        rpc,
    )
    .unwrap();
    let block = wrong.blocks().at(H256::from_low_u64_be(80)).await.unwrap();
    let error = block
        .extrinsics()
        .await
        .err()
        .expect("新版全局metadata必须复现历史错位");
    assert!(
        error.to_string().contains("VariantNotFound(143)"),
        "{error}"
    );
}

#[tokio::test]
async fn ethereum_reader_decodes_extension_one_without_stale_global_metadata() {
    let (module, _) = reader_model(false);
    let rpc = RpcClient::new(Native(Arc::new(module)));
    let genesis = H256::from(primitives::genesis::GENESIS_HASH);
    // V15只有扩展版本0；长驻旧上下文必须准确复现现网的版本1解码错误。
    let old: Vec<u8> = runtime::Runtime::metadata().into();
    let stale = OnlineClient::<SrcChainConfig>::from_rpc_client_with(
        genesis,
        subxt::client::RuntimeVersion {
            spec_version: 1,
            transaction_version: 4,
        },
        Metadata::decode_all(&mut old.as_slice()).unwrap(),
        rpc.clone(),
    )
    .unwrap();
    let block = stale.blocks().at(H256::from_low_u64_be(81)).await.unwrap();
    let error = block
        .extrinsics()
        .await
        .err()
        .expect("旧metadata必须拒绝版本1");
    let diagnostic = format!("{error:?}");
    assert!(
        diagnostic.contains("ExtrinsicExtensionVersionNotFound"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("extension_version: 1"), "{diagnostic}");

    let bound = Blocks::bind(&rpc, genesis, H256::from_low_u64_be(81))
        .await
        .unwrap();
    let body = bound.extrinsics().await.unwrap();
    assert_eq!(body.len(), 4);
    for item in body.iter() {
        assert!(
            item.pallet_name().is_ok(),
            "V5普通签名和ML First/Stored均须正常解码"
        );
    }
}

#[tokio::test]
async fn ethereum_reader_advances_finality_and_default_estimation_together() {
    let (mut module, _) = reader_model(false);
    let original = Native(Arc::new(module.clone()));
    let estimates = Arc::new(std::sync::Mutex::new(Vec::<H256>::new()));
    let selected = estimates.clone();
    module.remove_method("state_call");
    module
        .register_async_method("state_call", move |params, _, _| {
            let original = original.clone();
            let selected = selected.clone();
            async move {
                let values: Vec<serde_json::Value> = params.parse()?;
                if values[0] == "ReviveApi_eth_estimate_gas" {
                    selected
                        .lock()
                        .unwrap()
                        .push(serde_json::from_value(values[2].clone()).unwrap());
                }
                original
                    .request("state_call", serde_json::Value::Array(values))
                    .await
                    .map_err(|error| match error {
                        ClientError::Jsonrpsee(jsonrpsee::core::ClientError::Call(error)) => error,
                        other => jsonrpsee::types::ErrorObjectOwned::from(other),
                    })
            }
        })
        .unwrap();
    let native = Native(Arc::new(module));
    let blocks = Blocks::new(&native).await.unwrap();
    let api = OnlineClient::<SrcChainConfig>::from_rpc_client(blocks.rpc.clone())
        .await
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let receipts = ReceiptProvider::new(
        DbContext::new(pool, DbContext::DEFAULT_MAX_VARIABLE_NUMBER),
        blocks.clone(),
        ReceiptExtractor::new(api).await.unwrap(),
        Some(RETAINED_BLOCKS),
    )
    .await
    .unwrap();
    let ethereum = Ethereum {
        native,
        blocks,
        receipts,
        fees: FeeHistoryProvider::default(),
        prefix: Arc::new(Mutex::new(None)),
        heads: broadcast::channel(16).0,
        logs: broadcast::channel(16).0,
    };
    ethereum.rebuild().await.unwrap();
    let next = ethereum
        .blocks
        .block_by_hash(&H256::from_low_u64_be(81))
        .await
        .unwrap()
        .unwrap();
    ethereum.finalized(next.clone()).await.unwrap();
    // 重复最终性通知幂等；较旧通知不能把指针退回历史迁移状态。
    ethereum.finalized(next).await.unwrap();
    let old = ethereum
        .blocks
        .block_by_hash(&H256::from_low_u64_be(80))
        .await
        .unwrap()
        .unwrap();
    ethereum.finalized(old).await.unwrap();
    for tag in [
        BlockTag::Latest,
        BlockTag::Pending,
        BlockTag::Safe,
        BlockTag::Finalized,
    ] {
        assert_eq!(
            ethereum.resolve_hash(tag.into()).await.unwrap(),
            H256::from_low_u64_be(81)
        );
    }
    let rpc = ethereum.into_rpc();
    let default_gas: U256 = rpc
        .call(
            "eth_estimateGas",
            jsonrpsee::rpc_params![GenericTransaction::default()],
        )
        .await
        .unwrap();
    let explicit_gas: U256 = rpc
        .call(
            "eth_estimateGas",
            jsonrpsee::rpc_params![GenericTransaction::default(), "0x51"],
        )
        .await
        .unwrap();
    assert_eq!(default_gas, explicit_gas);
    assert_eq!(default_gas, U256::from(21000));
    assert_eq!(
        *estimates.lock().unwrap(),
        vec![H256::from_low_u64_be(81); 2],
        "默认和显式估算必须实际调用同一最新状态，不能只比较碰巧相同的报价"
    );
}

#[tokio::test]
async fn ethereum_reader_rejects_missing_parent_metadata_without_retrying_latest() {
    let (module, reads) = reader_model(true);
    let rpc = RpcClient::new(Native(Arc::new(module)));
    assert!(Blocks::bind(
        &rpc,
        H256::from(primitives::genesis::GENESIS_HASH),
        H256::from_low_u64_be(80)
    )
    .await
    .is_err());
    assert_eq!(*reads.lock().unwrap(), vec![H256::from_low_u64_be(79)]);
}

#[test]
fn ethereum_reader_checks_block_identity_and_pending_first_block() {
    assert_eq!(
        execution_hash(0, H256::repeat_byte(1), H256::zero()),
        H256::repeat_byte(1)
    );
    assert_eq!(
        execution_hash(80, H256::repeat_byte(2), H256::repeat_byte(1)),
        H256::repeat_byte(1)
    );
    assert!(checked_block(Default::default(), 80, None, true)
        .unwrap()
        .is_none());
    let block = pallet_revive::evm::Block {
        number: 80.into(),
        hash: H256::repeat_byte(3),
        ..Default::default()
    };
    assert!(checked_block(block.clone(), 80, Some(block.hash), false)
        .unwrap()
        .is_some());
    assert!(checked_block(block.clone(), 81, Some(block.hash), false).is_err());
    assert!(checked_block(block.clone(), 80, None, false).is_err());
    assert!(checked_block(block.clone(), 80, Some(block.hash), true).is_err());
}

#[test]
fn ethereum_reader_preserves_raw_json_error_data_and_large_numbers() {
    let value =
        raw_result(r#"{"jsonrpc":"2.0","id":1,"result":340282366920938463463374607431768211455}"#)
            .unwrap();
    assert_eq!(value.get(), "340282366920938463463374607431768211455");
    assert_eq!(
        raw_result(r#"{"jsonrpc":"2.0","id":1,"result":null}"#)
            .unwrap()
            .get(),
        "null"
    );
    let error=raw_result(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"denied","data":{"reason":"fixed"}}}"#).unwrap_err();
    assert!(error.to_string().contains("denied"));
}

/// 地址映射与账户快照共同钉块；缺失账户才为零，错误不得伪装成零余额。
#[tokio::test]
async fn ethereum_balance_uses_total_at_same_block_and_propagates_errors() {
    let (mut native, _) = reader_model(false);
    native.remove_method("state_getStorage").unwrap();
    let reads = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = reads.clone();
    native
        .register_method("state_getStorage", move |params, _, _| {
            let values: Vec<serde_json::Value> = params.parse()?;
            let key = values[0].as_str().unwrap();
            let mapping = format!(
                "0x{}",
                hex::encode(frame_support::storage::storage_prefix(
                    b"Revive",
                    b"OriginalAccount"
                ))
            );
            let accounts = format!(
                "0x{}",
                hex::encode(frame_support::storage::storage_prefix(
                    b"System", b"Account"
                ))
            );
            let error = || {
                jsonrpsee::types::ErrorObjectOwned::owned(
                    4003,
                    "balance storage denied",
                    None::<()>,
                )
            };
            if !key.starts_with(&mapping) && !key.starts_with(&accounts) {
                return Ok(Some(sp_core::Bytes(vec![0])));
            }
            assert_eq!(values.len(), 2, "映射与余额都必须显式指定块");
            let at: H256 = serde_json::from_value(values[1].clone()).unwrap();
            observed.lock().unwrap().push((key.to_string(), at));
            if key.starts_with(&mapping) {
                let address = hex::decode(&key[mapping.len()..]).unwrap();
                return match address[0] {
                    1 => Ok(Some(sp_core::Bytes(
                        sp_runtime::AccountId32::new(
                            [if at.to_low_u64_be() == 81 { 8 } else { 7 }; 32],
                        )
                        .encode(),
                    ))),
                    4 => Ok(Some(sp_core::Bytes(vec![0]))),
                    5 => Err(error()),
                    _ => Ok(None),
                };
            }
            let account = hex::decode(&key[key.len() - 64..]).unwrap();
            match account[0] {
                3 => return Ok(None),
                6 => return Ok(Some(sp_core::Bytes(vec![0; 32]))),
                9 => return Err(error()),
                _ => {}
            }
            let mut info = nodeclient::codec::AccountInfo::default();
            info.data.free = match account[0] {
                7 => 1_000,
                8 => 2_000,
                10 => u128::MAX,
                _ => 300_111,
            };
            info.data.reserved = 200;
            info.data.frozen = 800;
            Ok(Some(sp_core::Bytes(info.encode())))
        })
        .unwrap();
    let (module, task) = initialize(native).await.unwrap();
    let ratio = U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO);
    for (block, number, total) in [
        (serde_json::json!("latest"), 80, 1_200),
        (serde_json::json!("pending"), 80, 1_200),
        (serde_json::json!("safe"), 80, 1_200),
        (serde_json::json!("finalized"), 80, 1_200),
        (serde_json::json!("0x51"), 81, 2_200),
        (serde_json::json!(H256::from_low_u64_be(1080)), 80, 1_200),
    ] {
        reads.lock().unwrap().clear();
        let balance: U256 = module
            .call(
                "eth_getBalance",
                jsonrpsee::rpc_params![H160::repeat_byte(1), block],
            )
            .await
            .unwrap();
        assert_eq!(balance, U256::from(total as u128) * ratio);
        let snapshot = reads.lock().unwrap();
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot
            .iter()
            .all(|(_, at)| *at == H256::from_low_u64_be(number)));
    }
    let fallback: U256 = module
        .call(
            "eth_getBalance",
            jsonrpsee::rpc_params![H160::repeat_byte(2), "finalized"],
        )
        .await
        .unwrap();
    assert_eq!(fallback, U256::from(300_311u128) * ratio);
    let absent: U256 = module
        .call(
            "eth_getBalance",
            jsonrpsee::rpc_params![H160::repeat_byte(3), "finalized"],
        )
        .await
        .unwrap();
    assert_eq!(absent, U256::zero());
    for address in [4, 5, 6, 9, 10] {
        let error = module
            .call::<_, U256>(
                "eth_getBalance",
                jsonrpsee::rpc_params![H160::repeat_byte(address), "finalized"],
            )
            .await
            .unwrap_err();
        if address == 5 || address == 9 {
            match error {
                jsonrpsee::core::server::MethodsError::JsonRpc(error) => {
                    assert_eq!(error.code(), 4003)
                }
                other => panic!("原生错误码必须保留: {other:?}"),
            }
        }
    }
    drop(task);
}

#[tokio::test]
async fn ethereum_reader_keeps_all_rpc_methods_and_standard_newheads_subscription() {
    let (native, _) = reader_model(false);
    let (module, task) = initialize(native).await.unwrap();
    let methods = module
        .method_names()
        .collect::<std::collections::BTreeSet<_>>();
    let expected = [
        "eth_accounts",
        "eth_blockNumber",
        "eth_call",
        "eth_chainId",
        "eth_estimateGas",
        "eth_gasPrice",
        "eth_getBalance",
        "eth_getBlockByHash",
        "eth_getBlockByNumber",
        "eth_getBlockTransactionCountByHash",
        "eth_getBlockTransactionCountByNumber",
        "eth_getCode",
        "eth_getLogs",
        "eth_getStorageAt",
        "eth_getTransactionByBlockHashAndIndex",
        "eth_getTransactionByBlockNumberAndIndex",
        "eth_getTransactionByHash",
        "eth_getTransactionCount",
        "eth_getTransactionReceipt",
        "eth_maxPriorityFeePerGas",
        "eth_sendRawTransaction",
        "eth_sendTransaction",
        "eth_syncing",
        "net_listening",
        "net_version",
        "web3_clientVersion",
        "eth_feeHistory",
        "eth_subscribe",
        "eth_unsubscribe",
    ]
    .into_iter()
    .collect();
    assert_eq!(methods, expected);
    assert!(module
        .call::<_, bool>("net_listening", Vec::<()>::new())
        .await
        .unwrap());
    assert_eq!(
        module
            .call::<_, U256>("eth_chainId", Vec::<()>::new())
            .await
            .unwrap(),
        U256::from(2027)
    );
    assert_eq!(
        module
            .call::<_, U256>(
                "eth_getBalance",
                jsonrpsee::rpc_params![H160::zero(), "latest"]
            )
            .await
            .unwrap(),
        U256::from(300_311u128) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
    );
    let storage: Bytes = module
        .call(
            "eth_getStorageAt",
            jsonrpsee::rpc_params![H160::zero(), U256::zero(), "latest"],
        )
        .await
        .unwrap();
    assert_eq!(storage.0, vec![0; 32]);
    let ethereum_hash = H256::from_low_u64_be(1080);
    let balance: U256 = module
        .call(
            "eth_getBalance",
            jsonrpsee::rpc_params![
                H160::zero(),
                BlockNumberOrTagOrHash::BlockHash(ethereum_hash)
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        balance,
        U256::from(300_311u128) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
        "Ethereum哈希必须先映射到原生块"
    );
    assert!(module
        .call::<_, U256>(
            "eth_getBalance",
            jsonrpsee::rpc_params![
                H160::zero(),
                BlockNumberOrTagOrHash::BlockHash(H256::from_low_u64_be(9999))
            ]
        )
        .await
        .is_err());
    assert_eq!(
        module
            .call::<_, U256>(
                "eth_getTransactionCount",
                jsonrpsee::rpc_params![H160::zero(), "latest"]
            )
            .await
            .unwrap(),
        U256::from(7)
    );
    assert_eq!(
        module
            .call::<_, U256>(
                "eth_getTransactionCount",
                jsonrpsee::rpc_params![H160::zero(), "pending"]
            )
            .await
            .unwrap(),
        U256::from(8)
    );
    let block: Option<pallet_revive::evm::Block> = module
        .call(
            "eth_getBlockByHash",
            jsonrpsee::rpc_params![ethereum_hash, false],
        )
        .await
        .unwrap();
    assert_eq!(block.unwrap().hash, ethereum_hash);
    let code: Bytes = module
        .call(
            "eth_getCode",
            jsonrpsee::rpc_params![H160::zero(), "latest"],
        )
        .await
        .unwrap();
    assert_eq!(code.0, vec![0x60, 0]);
    let gas: U256 = module
        .call(
            "eth_estimateGas",
            jsonrpsee::rpc_params![GenericTransaction::default(), "latest"],
        )
        .await
        .unwrap();
    assert_eq!(gas, U256::from(21000));
    let error = module
        .call::<_, H256>(
            "eth_sendRawTransaction",
            jsonrpsee::rpc_params![Bytes(vec![0])],
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("Invalid transaction"),
        "错误载荷不得进入提交路径: {error}"
    );
    let subscription = module
        .subscribe("eth_subscribe", jsonrpsee::rpc_params!["newHeads"], 16)
        .await
        .unwrap();
    drop(subscription);
    assert!(module
        .subscribe(
            "eth_subscribe",
            jsonrpsee::rpc_params!["newBlockHeaders"],
            16
        )
        .await
        .is_err());
    drop(task);
    // 同一Native会话若在初始化途中换链，必须拒绝；冻结身份仍由原NodeGuard负责。
    let (mut native, _) = reader_model(false);
    native.remove_method("chain_getBlockHash");
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    native
        .register_method("chain_getBlockHash", move |params, _, _| {
            let n: u32 = params.one()?;
            let hash = if n == 0 {
                if reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    H256::from(primitives::genesis::GENESIS_HASH)
                } else {
                    H256::repeat_byte(8)
                }
            } else {
                H256::from_low_u64_be(n as u64)
            };
            Ok::<_, jsonrpsee::types::ErrorObjectOwned>(hash)
        })
        .unwrap();
    let error = initialize(native)
        .await
        .err()
        .expect("同一读取会话的链身份不得改变");
    assert!(
        error.to_string().contains("Genesis hash mismatch"),
        "{error}"
    );
}

/// 只在测试创世中预置公开签名夹具；原生账户继续使用官方 Revive 映射。
fn chain_spec_with_accounts(
    accounts: &[(sp_runtime::AccountId32, u128)],
) -> crate::chain::ChainSpec {
    let wasm = runtime::WASM_BINARY.expect("必须执行真实源码 WASM，禁止跳过");
    let mut genesis = genesis::config::genesis_config::<runtime::RuntimeGenesisConfig>();
    // 仅替换隔离夹具的 GRANDPA 权威，使用公开测试密钥完成真实投票。
    // 正式创世、链身份和 Runtime 源码不从此测试配置取得。
    let authority = sp_consensus_grandpa::AuthorityId::from(Ed25519Keyring::Alice.public());
    genesis["grandpa"]["authorities"] = serde_json::json!([[
        authority.to_ss58check_with_version(Ss58AddressFormat::custom(
            primitives::core_const::SS58_FORMAT
        )),
        1,
    ]]);
    for account in [
        Account::default().substrate_account(),
        Sr25519Keyring::Alice.to_account_id(),
        // Native receive-only fixture, separate from every miner/fee account.
        sp_runtime::AccountId32::new([0xa7; 32]),
    ] {
        genesis["balances"]["balances"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!([
                account.to_ss58check_with_version(Ss58AddressFormat::custom(
                    primitives::core_const::SS58_FORMAT
                )),
                1_000_000_000_000u128,
            ]));
    }
    for (account, balance) in accounts {
        let address = account.to_ss58check_with_version(Ss58AddressFormat::custom(
            primitives::core_const::SS58_FORMAT,
        ));
        let balances = genesis["balances"]["balances"].as_array_mut().unwrap();
        balances.retain(|entry| entry[0] != address);
        balances.push(serde_json::json!([address, balance]));
    }
    let mut properties = Properties::new();
    properties.insert(
        "ss58Format".into(),
        serde_json::json!(primitives::core_const::SS58_FORMAT),
    );
    properties.insert("tokenDecimals".into(), serde_json::json!(2));
    properties.insert("tokenSymbol".into(), serde_json::json!("GMB"));
    crate::chain::ChainSpec::builder(wasm, None)
        .with_name("Ethereum RPC acceptance")
        .with_id("ethereum-rpc-acceptance")
        .with_chain_type(ChainType::Development)
        .with_protocol_id("ethereum-rpc-acceptance")
        .with_properties(properties)
        .with_genesis_config_patch(genesis)
        .build()
}

fn test_config(node_name: &str, tokio_handle: tokio::runtime::Handle) -> Configuration {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gmb-ethereum-rpc-{node_name}-{}-{unique}",
        std::process::id()
    ));
    test_config_at(node_name, tokio_handle, root)
}

/// 调用方交付本产品生成目录；每个节点仍拥有独立数据库与网络身份。
fn test_config_at(
    node_name: &str,
    tokio_handle: tokio::runtime::Handle,
    root: std::path::PathBuf,
) -> Configuration {
    std::fs::create_dir_all(&root).expect("创建本轮 Ethereum RPC 隔离验收目录");
    let base_path = BasePath::new(root.clone());
    let mut network = NetworkConfiguration::new(
        node_name,
        "citizenchain-ethereum-rpc-test/0.1",
        Default::default(),
        None,
    );
    network.allow_non_globals_in_dht = true;
    network
        .listen_addresses
        .push("/ip4/127.0.0.1/tcp/0".parse().expect("test listen address"));

    Configuration {
        impl_name: "citizenchain-ethereum-rpc-test".into(),
        impl_version: "0.1".into(),
        role: Role::Full,
        tokio_handle,
        transaction_pool: TransactionPoolOptions::default(),
        network,
        keystore: KeystoreConfig::InMemory,
        database: DatabaseSource::RocksDb {
            path: root.join("db"),
            cache_size: 128,
        },
        trie_cache_maximum_size: Some(16 * 1024 * 1024),
        warm_up_trie_cache: None,
        state_pruning: Some(PruningMode::ArchiveAll),
        blocks_pruning: BlocksPruning::KeepAll,
        chain_spec: Box::new(chain_spec()),
        executor: ExecutorConfiguration::default(),
        wasm_runtime_overrides: None,
        rpc: RpcConfiguration {
            addr: None,
            max_connections: Default::default(),
            cors: None,
            methods: Default::default(),
            max_request_size: Default::default(),
            max_response_size: Default::default(),
            id_provider: Default::default(),
            max_subs_per_conn: Default::default(),
            port: 9944,
            message_buffer_capacity: Default::default(),
            batch_config: RpcBatchRequestConfig::Unlimited,
            rate_limit: None,
            rate_limit_whitelisted_ips: Default::default(),
            rate_limit_trust_proxy_headers: Default::default(),
            request_logger_limit: 1024,
        },
        prometheus_config: None,
        telemetry_endpoints: None,
        offchain_worker: Default::default(),
        force_authoring: false,
        disable_grandpa: true,
        dev_key_seed: None,
        tracing_targets: None,
        tracing_receiver: Default::default(),
        announce_block: true,
        data_path: root,
        base_path,
    }
}

fn native(partial: &Service) -> RpcModule<()> {
    use sc_rpc_api::{author::AuthorApiServer, chain::ChainApiServer, state::StateApiServer};
    let spawn: Arc<dyn sp_core::traits::SpawnNamed> = Arc::new(partial.task_manager.spawn_handle());
    let mut module = RpcModule::new(());
    module
        .merge(sc_rpc::chain::new_full(partial.client.clone(), spawn.clone()).into_rpc())
        .unwrap();
    let (state, _) = sc_rpc::state::new_full(partial.client.clone(), spawn.clone(), None);
    module.merge(state.into_rpc()).unwrap();
    module
        .merge(
            sc_rpc::author::Author::new(
                partial.client.clone(),
                partial.transaction_pool.clone(),
                partial.keystore_container.keystore(),
                spawn,
            )
            .into_rpc(),
        )
        .unwrap();
    module
        .merge(
            crate::rpc::create_full(crate::rpc::FullDeps {
                client: partial.client.clone(),
                pool: partial.transaction_pool.clone(),
                keystore: partial.keystore_container.keystore(),
                miner_signing: Arc::new(std::sync::Mutex::new(())),
                cpu_hashrate_fn: || 0.0,
                gpu_hashrate_fn: None,
                offchain_clearing_rpc: None,
            })
            .unwrap(),
        )
        .unwrap();
    module.extensions_mut().insert(sc_rpc_api::DenyUnsafe::No);
    module
}

fn endpoint(address: std::net::SocketAddr) -> sc_rpc_server::RpcEndpoint {
    sc_rpc_server::RpcEndpoint {
        listen_addr: address,
        batch_config: jsonrpsee::server::BatchRequestConfig::Unlimited,
        max_connections: 32,
        max_payload_in_mb: 2,
        max_payload_out_mb: 2,
        max_subscriptions_per_connection: 16,
        max_buffer_capacity_per_connection: 64,
        rate_limit: None,
        rate_limit_trust_proxy_headers: false,
        rate_limit_whitelisted_ips: vec![],
        cors: Some(vec!["https://localhost".into()]),
        rpc_methods: sc_rpc_server::RpcMethods::Safe,
        is_optional: false,
        retry_random_port: false,
    }
}

async fn serve(
    partial: &Service,
    credentials: &rcgen::CertifiedKey,
) -> (
    String,
    tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
) {
    serve_on_port(partial, credentials, 0).await
}

/// 显式会话使用固定端口；占用时失败，普通自动化夹具继续使用随机端口。
async fn serve_on_port(
    partial: &Service,
    credentials: &rcgen::CertifiedKey,
    port: u16,
) -> (
    String,
    tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
) {
    let mut module = native(partial);
    let (ethereum, synchronization) = initialize(module.clone()).await.unwrap();
    module.merge(ethereum).unwrap();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .expect("测试 RPC 端口被占用或不能绑定");
    let address = listener.local_addr().unwrap();
    drop(listener);
    let server_name = session_server_name().unwrap();
    let mut tls = if server_name.is_some() {
        // 服务器会话原位读取部署方的可信证书，不复制、导出或打印私钥。
        nodedesktop::tls::RpcTls::load().unwrap()
    } else {
        nodedesktop::tls::RpcTls::from_der(
            vec![credentials.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(credentials.key_pair.serialize_der()),
        )
        .unwrap()
    };
    let mut rpc_endpoint = endpoint(address);
    if let Some(path) = std::env::var_os("CITIZENCHAIN_TEST_PAGE_INPUT") {
        let root = std::path::PathBuf::from(
            std::env::var_os("CITIZENCHAIN_TEST_SESSION_ROOT")
                .expect("隔离页面必须属于显式测试会话"),
        );
        let path = std::path::PathBuf::from(path);
        assert_eq!(
            path,
            root.parent().unwrap().join("install.html"),
            "隔离页面只能来自本轮固定现场"
        );
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= 128 * 1024
        );
        assert_eq!(
            path.canonicalize().unwrap(),
            path,
            "隔离页面路径不能经过链接"
        );
        let page = std::fs::read(&path).unwrap();
        // 隔离页面的唯一受信 Origin 来自已核对的完整视图，不能放开任意网页。
        rpc_endpoint.cors = Some(vec![session_page(&page).unwrap()]);
        tls = tls.with_test_page(page).unwrap();
    }
    let task = tokio::spawn(async move {
        tokio::try_join!(
            async {
                synchronization
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
            },
            tls.run(vec![rpc_endpoint], module)
        )?;
        Ok(())
    });
    (
        format!(
            "https://{}:{}",
            server_name.as_deref().unwrap_or("localhost"),
            address.port()
        ),
        task,
    )
}

async fn response(
    http: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let target = reqwest::Url::parse(url).unwrap();
            let mut request = http.post(url);
            if target.host_str() != Some("localhost") {
                // SNI 保留证书名，HTTP Host 使用既有回环白名单，两者职责独立。
                request = request.header("Host", format!("127.0.0.1:{}", target.port().unwrap()));
            }
            match request
                .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
                .send()
                .await
            {
                Ok(response) => return response.json().await.unwrap(),
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("真实 TLS 请求超时")
}

async fn rpc(
    http: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let response = response(http, url, method, params).await;
    assert!(response.get("error").is_none(), "{method}: {response}");
    response["result"].clone()
}

async fn wait_block(http: &reqwest::Client, url: &str, number: u32) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while rpc(http, url, "eth_blockNumber", serde_json::json!([])).await
            != format!("0x{number:x}")
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("索引必须追上真实最佳链");
}

fn transaction(
    nonce: u32,
    to: Option<H160>,
    input: Vec<u8>,
    value: u128,
) -> (String, OpaqueExtrinsic) {
    transaction_from(&Account::default(), nonce, to, input, value)
}

fn transaction_from(
    account: &Account,
    nonce: u32,
    to: Option<H160>,
    input: Vec<u8>,
    value: u128,
) -> (String, OpaqueExtrinsic) {
    let fee_gas = pallet_revive::evm::fees::native_fee_to_gas::<runtime::Runtime>(
        primitives::fee_policy::calculate_onchain_fee(value),
    )
    .expect("测试金额必须有可表达的费用 gas");
    let tx = GenericTransaction {
        from: Some(account.address()),
        to,
        nonce: Some(nonce.into()),
        chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
        gas: Some(100_000_000u64.max(fee_gas).into()),
        // 测试签名复用已消费 SDK 的钱包报价，不能手填旧的一分 gas 价格掩盖问题。
        gas_price: Some(pallet_revive::Pallet::<runtime::Runtime>::evm_base_fee()),
        value: Some(U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)),
        input: Bytes(input).into(),
        r#type: Some(pallet_revive::evm::TYPE_LEGACY.into()),
        ..Default::default()
    };
    signed_transaction(account, tx)
}

/// 同一真实签名与原生包装供固定夹具及钱包 RPC 报价交易使用。
fn signed_transaction(account: &Account, tx: GenericTransaction) -> (String, OpaqueExtrinsic) {
    let payload = account
        .sign_transaction(tx.try_into_unsigned().unwrap())
        .signed_payload();
    let xt: runtime::UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::new_bare(
        runtime::RuntimeCall::Revive(pallet_revive::Call::eth_transact {
            payload: payload.clone(),
        }),
    )
    .into();
    (format!("0x{}", hex::encode(payload)), xt.into())
}

/// 按实际区块读取 Runtime 事件，费用与新合约 ED 转移分别验证。
fn paid_fees(client: &FullClient, hash: sp_core::H256) -> Vec<(sp_runtime::AccountId32, u128)> {
    use sc_client_api::StorageProvider;
    let key = frame_support::storage::storage_prefix(b"System", b"Events");
    let data = client
        .storage(hash, &sp_storage::StorageKey(key.to_vec()))
        .unwrap()
        .unwrap();
    Vec::<frame_system::EventRecord<runtime::RuntimeEvent, sp_core::H256>>::decode_all(
        &mut &data.0[..],
    )
    .unwrap()
    .into_iter()
    .filter_map(|record| match record.event {
        runtime::RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid { account_id, fee }) => {
            Some((account_id, fee))
        }
        _ => None,
    })
    .collect()
}

/// 重组分支也必须包含合法用户交易，不能用空块绕过正式导入约束。
fn native_remark(client: &Arc<FullClient>, nonce: u32) -> OpaqueExtrinsic {
    let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
        &format!("{:?}", client.info().genesis_hash),
        nonce,
        runtime::VERSION.spec_version,
        runtime::VERSION.transaction_version,
        b"mixed Ethereum index",
    )
    .unwrap();
    OpaqueExtrinsic::try_from_encoded_extrinsic(&hex::decode(hex.trim_start_matches("0x")).unwrap())
        .unwrap()
}

/// 同一矿工的维护与热钱包提交共用锁，真实V5验证和交易池nonce不得互相覆盖。
#[cfg(feature = "runtime-tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mapping_backfill_v5_pool_nonce_and_signing_are_shared() {
    use codec::Decode;
    use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
    use sp_keystore::Keystore;
    use sp_runtime::{generic::Preamble, traits::ExtensionVariant};

    let config = test_config("mapping-backfill", tokio::runtime::Handle::current());
    let root = config.base_path.path().to_path_buf();
    let partial = crate::service::testing::new_partial_for_test(&config).unwrap();
    let keystore = partial.keystore_container.keystore();
    keystore
        .sr25519_generate_new(nodemining::pow::POW_AUTHOR_KEY_TYPE, Some("//Alice"))
        .unwrap();
    let signing = Arc::new(std::sync::Mutex::new(()));
    let mut submissions = Vec::new();
    for remark in [
        b"mapping maintenance".to_vec(),
        b"ordinary miner remark".to_vec(),
    ] {
        let client = partial.client.clone();
        let pool = partial.transaction_pool.clone();
        let keystore = keystore.clone();
        let signing = signing.clone();
        submissions.push(tokio::task::spawn_blocking(move || {
            crate::rpc::submit_powr_signed_tx(
                &client,
                &pool,
                &keystore,
                &signing,
                runtime::RuntimeCall::System(frame_system::Call::remark { remark }),
            )
            .unwrap()
            .0
        }));
    }
    let first = submissions.remove(0).await.unwrap();
    let second = submissions.remove(0).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(partial.transaction_pool.status().ready, 2);
    assert_eq!(partial.transaction_pool.status().future, 0);
    let mut nonces = Vec::new();
    for transaction in partial.transaction_pool.ready() {
        let encoded = transaction.data().encode();
        let extrinsic = runtime::UncheckedExtrinsic::decode(&mut encoded.as_slice()).unwrap();
        let Preamble::General(ExtensionVariant::Other(pipeline)) = extrinsic.0.preamble else {
            panic!("矿工交易必须是V5认证管线1");
        };
        nonces.push(pipeline.extension.1 .7 .0);
    }
    nonces.sort();
    assert_eq!(nonces, vec![0, 1]);
    // 仅本测试现场；不触碰任何正式链数据库。
    drop(keystore);
    let backend = Arc::downgrade(&partial.backend);
    let tasks = stop_partial(partial);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !tasks.running_tasks().is_empty() || backend.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("回填定向测试任务和数据库必须先释放");
    drop(config);
    std::fs::remove_dir_all(root).unwrap();
}

fn proposal(
    client: &Arc<FullClient>,
    parent: sp_core::H256,
    body: Vec<OpaqueExtrinsic>,
    timestamp: u64,
) -> BlockImportParams<Block> {
    let author = sr25519::Pair::from_string("//Alice//pow", None).unwrap();
    let mut digest = Digest::default();
    digest.push(DigestItem::PreRuntime(
        sp_consensus_pow::POW_ENGINE_ID,
        author.public().encode(),
    ));
    let mut builder = sc_block_builder::BlockBuilderBuilder::new(&**client)
        .on_parent_block(parent)
        .fetch_parent_block_number(&**client)
        .unwrap()
        .with_inherent_digests(digest)
        .build()
        .unwrap();
    let timestamp_xt: runtime::UncheckedExtrinsic =
        sp_runtime::generic::UncheckedExtrinsic::new_bare(runtime::RuntimeCall::Timestamp(
            runtime::TimestampCall::set { now: timestamp },
        ))
        .into();
    builder.push(timestamp_xt.into()).unwrap();
    for xt in body {
        builder.push(xt).unwrap();
    }
    let (block, changes) = builder.build().unwrap().into_inner();
    let (header, body) = block.deconstruct();
    let mut params: BlockImportParams<Block> =
        BlockImportParams::new(sp_consensus::BlockOrigin::Own, header);
    params.body = Some(body);
    params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(changes));
    // 真实 PoW seal 与两层守卫一起验收，不使用跳过签名或导入规则的测试后端。
    let prehash = params.header.hash();
    let difficulty = nodemining::pow::SimplePow::new(client.clone())
        .difficulty(parent)
        .unwrap();
    let nonce = (0u64..)
        .find(|n| {
            nodemining::pow::hash_meets_difficulty(
                &nodemining::pow::pow_hash(prehash.as_ref(), *n),
                difficulty,
            )
        })
        .unwrap();
    params.post_digests.push(DigestItem::Seal(
        sp_consensus_pow::POW_ENGINE_ID,
        (nonce, author.sign(prehash.as_ref())).encode(),
    ));
    params.post_hash = Some(params.post_hash());
    params.fork_choice = Some(ForkChoiceStrategy::Custom(true));
    params.insert_intermediate(
        sc_consensus_pow::INTERMEDIATE_KEY,
        sc_consensus_pow::PowIntermediate::<U256> { difficulty: None },
    );
    params
}

/// 保留真实网络与数据库生命周期；同步只能经过生产导入队列执行 WASM 和两层守卫。
struct TestNode {
    client: Arc<FullClient>,
    backend: Arc<FullBackend>,
    network: Arc<dyn NetworkService>,
    sync: Arc<sc_network_sync::SyncingService<Block>>,
    pending_finality: Option<Box<dyn FnOnce() + Send>>,
    _task_manager: sc_service::TaskManager,
}

impl TestNode {
    // 消费节点并保留官方任务登记表，调用方继续核对Backend真正释放。
    fn stop(self) -> sc_service::TaskRegistry {
        self._task_manager.into_task_registry()
    }

    fn start_finality(&mut self) {
        self.pending_finality
            .take()
            .expect("每个节点只启动一次 GRANDPA")();
    }
}

// 未启动网络的重开验收同样先释放全部Service持有者，再等待后台任务退出。
fn stop_partial(service: Service) -> sc_service::TaskRegistry {
    service.task_manager.into_task_registry()
}

/// 将真实Wasm排演的受信离线检查点装入独立节点，只验收生产HTTPS/RPC读取链路。
/// 排演头没有PoW seal；它们不经过共识导入，也不作为PoW或GRANDPA有效性的证据。
/// 共识安全由本文件已有真实出块、坏块拒绝、分叉及重启测试分别验证。
#[cfg(feature = "runtime-tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "通过塔塔控制台交付已对账Wasm归档、原链头部及Runtime端到端程序"]
async fn replay_checkpoint_serves_exact_candidate_over_verified_https() {
    use codec::DecodeAll;
    use sc_client_api::backend::{Backend as _, BlockImportOperation as _, NewBlockState};
    use sp_runtime::traits::{Hash as _, Header as _};
    use sp_state_machine::Backend as _;
    use std::{io::BufRead, path::PathBuf};
    type Header = <Block as BlockT>::Header;
    let input = |name| PathBuf::from(std::env::var_os(name).expect("缺少显式隔离验收输入"));
    let root = input("CITIZENCHAIN_RPC_TEST_ROOT");
    let allowed = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/test"))
        .canonicalize()
        .unwrap();
    assert!(root.is_absolute() && root.starts_with(allowed));
    std::fs::create_dir(&root).expect("只能创建本轮全新隔离现场");
    let read = |name| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(input(name)).unwrap()).unwrap()
    };
    let headers = read("CITIZENCHAIN_TEST_HEADERS");
    let report = read("CITIZENCHAIN_REPLAY_REPORT");
    assert_eq!(report["passed"], true);
    assert_eq!(report["spec_version"], 8);
    assert_eq!(report["transaction_version"], 7);
    let genesis = H256::from(primitives::genesis::GENESIS_HASH);
    assert_eq!(headers["genesis"], serde_json::json!(genesis));
    let mut lines =
        std::io::BufReader::new(std::fs::File::open(input("CITIZENCHAIN_REPLAY_ARCHIVE")).unwrap())
            .lines();
    let first: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    let header = |row: &serde_json::Value| {
        Header::decode_all(
            &mut hex::decode(row["header"].as_str().unwrap())
                .unwrap()
                .as_slice(),
        )
        .unwrap()
    };
    let from = *header(&first).number();
    assert!(first["state"].is_array());
    let mut config = test_config_at(
        "candidate-checkpoint",
        tokio::runtime::Handle::current(),
        root.clone(),
    );
    config.chain_spec = Box::new(crate::chain::chain_config().unwrap());
    assert!(
        config.network.boot_nodes.is_empty(),
        "隔离测试不得连接主链引导节点"
    );
    // 固定主网创世必须走生产入口；测试创世助手刻意只接受本次内置Wasm。
    let partial = crate::service::new_partial(&config).unwrap();
    eprintln!("原冻结创世已装入本轮隔离数据库");
    assert_eq!(
        partial.client.info().genesis_hash,
        genesis,
        "必须保留冻结创世"
    );
    let mut previous = genesis;
    let mut number = 0;
    let mut import_header = |next: Header| {
        assert_eq!(*next.parent_hash(), previous);
        assert_eq!(*next.number(), number + 1);
        previous = next.hash();
        number += 1;
        let mut op = partial.backend.begin_operation().unwrap();
        op.set_block_data(next, None, None, None, NewBlockState::Normal, false)
            .unwrap();
        partial.backend.commit_operation(op).unwrap();
        if number % 1000 == 0 {
            eprintln!("已装配原链与排演头部至 {number}");
        }
    };
    for row in headers["headers"].as_array().unwrap().iter().skip(1) {
        let next: Header = serde_json::from_value(row["header"].clone()).unwrap();
        assert_eq!(serde_json::json!(next.hash()), row["hash"]);
        import_header(next);
    }
    for row in report["blocks"].as_array().unwrap() {
        let next = header(row);
        if *next.number() >= from {
            break;
        }
        import_header(next);
    }
    let mut complete = false;
    let mut imported = 0;
    for row in std::iter::once(Ok(serde_json::to_string(&first).unwrap())).chain(lines) {
        let row: serde_json::Value = serde_json::from_str(&row.unwrap()).unwrap();
        assert!(!complete, "完成记录后不能追加归档");
        if row["complete"] == true {
            assert_eq!(row["genesis"], serde_json::json!(genesis));
            assert_eq!(row["last_hash"], serde_json::json!(previous));
            assert_eq!(row["wasm_hash"], report["wasm_hash"]);
            complete = true;
            continue;
        }
        let next = header(&row);
        assert_eq!(*next.parent_hash(), previous);
        assert_eq!(*next.number(), number + 1);
        let body = row["body"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                OpaqueExtrinsic::decode_all(
                    &mut hex::decode(value.as_str().unwrap()).unwrap().as_slice(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sp_runtime::traits::BlakeTwo256::ordered_trie_root(
                body.iter().map(Encode::encode).collect(),
                sp_runtime::StateVersion::V0
            ),
            *next.extrinsics_root()
        );
        let mut op = partial.backend.begin_operation().unwrap();
        let root = if let Some(rows) = row["state"].as_array() {
            assert_eq!(imported, 0);
            let top = rows
                .iter()
                .map(|pair| {
                    (
                        hex::decode(pair[0].as_str().unwrap()).unwrap(),
                        hex::decode(pair[1].as_str().unwrap()).unwrap(),
                    )
                })
                .collect();
            op.reset_storage(
                sp_runtime::Storage {
                    top,
                    ..Default::default()
                },
                sp_runtime::StateVersion::V0,
            )
            .unwrap()
        } else {
            partial
                .backend
                .begin_state_operation(&mut op, previous)
                .unwrap();
            let changes = row["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        hex::decode(pair[0].as_str().unwrap()).unwrap(),
                        pair[1].as_str().map(|value| hex::decode(value).unwrap()),
                    )
                })
                .collect::<Vec<_>>();
            let (root, transaction) = op.state().unwrap().unwrap().storage_root(
                changes
                    .iter()
                    .map(|(key, value)| (key.as_slice(), value.as_deref())),
                sp_runtime::StateVersion::V0,
            );
            op.update_db_storage(transaction).unwrap();
            op.update_storage(changes, Vec::new()).unwrap();
            root
        };
        assert_eq!(root, *next.state_root(), "归档状态根必须逐块准确匹配");
        previous = next.hash();
        number += 1;
        imported += 1;
        op.set_block_data(next, Some(body), None, None, NewBlockState::Best, true)
            .unwrap();
        partial.backend.commit_operation(op).unwrap();
    }
    assert!(
        complete && imported >= 257,
        "必须完成全部对账且覆盖近期执行父状态"
    );
    eprintln!("已逐块重算 {imported} 个近期归档状态与正文根");
    // 官方后端要求按父子顺序最终化；批量提交仍保留每个真实头的顺序。
    let mut op = partial.backend.begin_operation().unwrap();
    for height in 1..=number {
        op.mark_finalized(
            partial.backend.blockchain().hash(height).unwrap().unwrap(),
            None,
        )
        .unwrap();
    }
    partial.backend.commit_operation(op).unwrap();
    eprintln!("受信检查点已按原父子顺序最终化");
    let released = Arc::downgrade(&partial.backend);
    let tasks = stop_partial(partial);
    tokio::time::timeout(Duration::from_secs(30), async {
        while !tasks.running_tasks().is_empty() || released.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("检查点写入服务必须释放数据库");
    eprintln!("检查点写入服务和数据库已释放，开始重开生产节点");

    // 重新打开生产服务；真实System RPC、Runtime API、Ethereum装配及TLS均不替换。
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    assert_ne!(address.port(), 9944);
    config.rpc.addr = Some(vec![endpoint(address)]);
    let credentials =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
    let tls = nodedesktop::tls::RpcTls::from_der(
        vec![credentials.cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(credentials.key_pair.serialize_der()),
    )
    .unwrap();
    let certificate = root.join("public-certificate.hex");
    // 此文件只有公开证书，私钥不导出、不写日志或环境变量。
    std::fs::write(&certificate, hex::encode(credentials.cert.der())).unwrap();
    let failure = Arc::new(std::sync::Mutex::new(None));
    // 服务装配会创建同步客户端；在阻塞线程调用公开入口，保留当前Tokio Handle。
    let feedback = failure.clone();
    let mut manager = tokio::task::spawn_blocking(move || {
        crate::service::new_full(
            config,
            0,
            None,
            Some(tls),
            Some(feedback),
            None,
            None,
            None,
            None,
        )
    })
    .await
    .unwrap()
    .unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let error = failure.lock().unwrap().clone();
            assert!(error.is_none(), "真实RPC服务装配失败：{error:?}");
            if tokio::net::TcpStream::connect(address).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "真实HTTPS节点未就绪");
    let rpc = format!("https://{address}/");
    let runtime = input("CITIZENCHAIN_TEST_RUNTIME_EXECUTABLE");
    let node = input("CITIZENCHAIN_TEST_NODEJS");
    let wasm = input("CITIZENCHAIN_TEST_WASM");
    let result = tokio::task::spawn_blocking(move || {
        for (executable, test) in [
            (
                std::env::current_exe().unwrap(),
                "e2e_tests::isolated_node_https_genesis_and_health",
            ),
            (
                runtime,
                "finalized_runtime_api_and_exact_wasm_match_over_verified_https",
            ),
        ] {
            let status = std::process::Command::new(executable)
                .args([test, "--exact", "--ignored", "--nocapture"])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("CITIZENCHAIN_TEST_RPC", &rpc)
                .env("CITIZENCHAIN_TEST_CERTIFICATE", &certificate)
                .env("CITIZENCHAIN_TEST_NODEJS", &node)
                .env("CITIZENCHAIN_TEST_WASM", &wasm)
                .status()
                .unwrap();
            if !status.success() {
                return false;
            }
        }
        true
    });
    let passed = tokio::select! {
        result = result => result.unwrap(),
        result = manager.future() => panic!("真实节点在验收期间退出：{result:?}"),
    };
    let tasks = tokio::task::spawn_blocking(move || manager.into_task_registry())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !tasks.running_tasks().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("真实隔离节点后台任务必须全部停止");
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    assert!(passed, "隔离Node或Runtime端到端失败");
    std::fs::remove_dir_all(root).unwrap();
}

fn start_test_network(config: &mut Configuration, partial: Service, voter: bool) -> TestNode {
    type NetworkBackend = sc_network::NetworkWorker<Block, <Block as BlockT>::Hash>;
    let tls = nodedesktop::certificate::load_or_generate_tls_cert(config.base_path.path()).unwrap();
    config.network.tls_private_key_der = Some(tls.private_key_der);
    config.network.tls_certificate_chain_der = Some(tls.certificate_chain_der);
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        import_queue,
        transaction_pool,
        keystore_container,
        other: (_, grandpa_link, _),
        ..
    } = partial;
    let mut net_config = sc_network::config::FullNetworkConfiguration::<
        Block,
        <Block as BlockT>::Hash,
        NetworkBackend,
    >::new(&config.network, config.prometheus_registry().cloned());
    let metrics = NetworkBackend::register_notification_metrics(config.prometheus_registry());
    let protocol = sc_consensus_grandpa::protocol_standard_name(
        &client.info().genesis_hash,
        &config.chain_spec,
    );
    let (grandpa_protocol, notification_service) =
        sc_consensus_grandpa::grandpa_peers_set_config::<_, NetworkBackend>(
            protocol.clone(),
            metrics.clone(),
            net_config.peer_store_handle(),
        );
    net_config.add_notification_protocol(grandpa_protocol);
    let warp = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::new(),
    ));
    let (network, _, _, sync) = sc_service::build_network(sc_service::BuildNetworkParams {
        config,
        net_config,
        client: client.clone(),
        transaction_pool: transaction_pool.clone(),
        spawn_handle: task_manager.spawn_handle(),
        spawn_essential_handle: task_manager.spawn_essential_handle(),
        import_queue,
        block_announce_validator_builder: None,
        warp_sync_config: Some(sc_service::WarpSyncConfig::WithProvider(warp)),
        block_relay: None,
        metrics,
    })
    .unwrap();
    let keystore = if voter {
        let keystore = keystore_container.keystore();
        let public = keystore
            .ed25519_generate_new(sp_consensus_grandpa::KEY_TYPE, Some("//Alice"))
            .unwrap();
        assert_eq!(public, Ed25519Keyring::Alice.public());
        Some(keystore)
    } else {
        None
    };
    let grandpa_config = sc_consensus_grandpa::Config {
        gossip_duration: Duration::from_millis(333),
        justification_generation_period: 1,
        name: Some(config.network.node_name.clone()),
        observer_enabled: false,
        keystore,
        local_role: config.role,
        telemetry: None,
        protocol_name: protocol,
    };
    let grandpa_network = network.clone();
    let grandpa_sync = sync.clone();
    let spawn = task_manager.spawn_handle();
    let essential = task_manager.spawn_essential_handle();
    // 保留真实通知服务与导入链接，先完成未最终化分叉，再启动官方投票/观察任务。
    let pending_finality = Some(Box::new(move || {
        if voter {
            let params = sc_consensus_grandpa::GrandpaParams {
                config: grandpa_config,
                link: grandpa_link,
                network: grandpa_network,
                sync: grandpa_sync,
                notification_service,
                voting_rule: (),
                prometheus_registry: None,
                shared_voter_state: sc_consensus_grandpa::SharedVoterState::empty(),
                telemetry: None,
                offchain_tx_pool_factory:
                    sc_transaction_pool_api::OffchainTransactionPoolFactory::new(transaction_pool),
            };
            essential.spawn_blocking(
                "grandpa-voter",
                None,
                sc_consensus_grandpa::run_grandpa_voter(params).unwrap(),
            );
        } else {
            spawn.spawn_blocking(
                "grandpa-observer",
                None,
                sc_consensus_grandpa::run_grandpa_observer(
                    grandpa_config,
                    grandpa_link,
                    grandpa_network,
                    grandpa_sync,
                    notification_service,
                )
                .unwrap(),
            );
        }
    }) as Box<dyn FnOnce() + Send>);
    TestNode {
        client,
        backend,
        network,
        sync,
        pending_finality,
        _task_manager: task_manager,
    }
}

async fn wait_finalized(node: &TestNode, hash: sp_core::H256, number: u32) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while node.client.info().finalized_hash != hash {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("独立节点必须通过真实 GRANDPA 获得同一最终块");
    assert_eq!(node.client.info().finalized_number, number);
}

async fn assert_finalized_rpc(http: &reqwest::Client, url: &str, expected: &serde_json::Value) {
    for tag in ["safe", "finalized"] {
        let block = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let block = rpc(
                    http,
                    url,
                    "eth_getBlockByNumber",
                    serde_json::json!([tag, false]),
                )
                .await;
                if block["hash"] == expected["hash"] {
                    break block;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("每个 HTTPS RPC 必须推进到经 GRANDPA 验证的同一最终块");
        assert_eq!(block["number"], expected["number"]);
    }
}

async fn connect_peer(peer: &TestNode, source: &TestNode) {
    assert_eq!(
        peer.client.info().genesis_hash,
        source.client.info().genesis_hash
    );
    tokio::time::timeout(Duration::from_secs(20), async {
        while source.network.listen_addresses().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        peer.network
            .add_reserved_peer(MultiaddrWithPeerId {
                multiaddr: source
                    .network
                    .listen_addresses()
                    .into_iter()
                    .next()
                    .unwrap(),
                peer_id: source.network.local_peer_id(),
            })
            .unwrap();
        while peer.sync.num_connected_peers() == 0 || source.sync.num_connected_peers() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("真实 TLS P2P 节点必须连接");
}

async fn wait_peer(peer: &TestNode, hash: sp_core::H256) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while peer.client.info().best_hash != hash {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("节点必须通过 P2P 导入同一规范块，禁止直接写入跟随节点");
}

/// 逐字段比较独立节点的真实 HTTPS 结果，禁止仅凭高度一致宣称状态同步。
async fn assert_peer_state(
    http: &reqwest::Client,
    source: &str,
    peer: &str,
    signer: H160,
    contract: H160,
    transactions: &[serde_json::Value],
) {
    for (method, params) in [
        ("eth_chainId", serde_json::json!([])),
        ("eth_getCode", serde_json::json!([contract, "latest"])),
        (
            "eth_getStorageAt",
            serde_json::json!([contract, "0x0", "latest"]),
        ),
        ("eth_getBalance", serde_json::json!([signer, "latest"])),
        (
            "eth_getTransactionCount",
            serde_json::json!([signer, "latest"]),
        ),
        (
            "eth_getLogs",
            serde_json::json!([{"fromBlock":"0x1","toBlock":"latest","address":contract}]),
        ),
    ] {
        assert_eq!(
            rpc(http, peer, method, params.clone()).await,
            rpc(http, source, method, params).await,
            "跨节点 {method} 必须一致"
        );
    }
    for transaction in transactions {
        let params = serde_json::json!([transaction]);
        assert_eq!(
            rpc(http, peer, "eth_getTransactionReceipt", params.clone()).await,
            rpc(http, source, "eth_getTransactionReceipt", params).await,
            "回执必须绑定同步后的规范链及实际交易位置"
        );
    }
}

/// 自动回归也真实运行会话；只有显式参数延长就绪后的保留时间，不忽略测试。
fn session_seconds(value: &str) -> Result<u64, &'static str> {
    match value.parse::<u64>() {
        Ok(seconds @ 1..=1800) => Ok(seconds),
        _ => Err("测试会话就绪后的保留时间必须为 1 至 1800 秒"),
    }
}

fn session_port(value: &str) -> Result<u16, &'static str> {
    match value.parse::<u16>() {
        Ok(port @ 1..=u16::MAX) if value == port.to_string() => Ok(port),
        _ => Err("显式测试 RPC 端口必须是 1 至 65535 的规范整数"),
    }
}

/// 消费者核对完整隔离视图，不能仅凭标题接受仍指向正式资产的页面。
fn session_page(page: &[u8]) -> Result<String, &'static str> {
    let page = std::str::from_utf8(page).map_err(|_| "隔离页面不是 UTF-8")?;
    let original = std::str::from_utf8(nodedesktop::tls::INSTALL_PAGE).unwrap();
    let address = |html: &str| -> Option<String> {
        Some(html.split("id=\"rpc\" type=\"url\" inputmode=\"url\" autocomplete=\"off\" spellcheck=\"false\" value=\"")
                .nth(1)?.split('"').next()?.to_owned())
    };
    let formal = address(original).ok_or("正式页面地址标记缺失")?;
    let isolated = address(page).ok_or("隔离页面地址标记缺失")?;
    let url = reqwest::Url::parse(&isolated).map_err(|_| "隔离 RPC 地址无效")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != isolated
        || isolated == formal
    {
        return Err("隔离页面必须使用独立的无参数 HTTPS RPC 根地址");
    }
    let formal_origin = formal.strip_suffix('/').ok_or("正式 RPC 根地址无效")?;
    let isolated_origin = isolated.strip_suffix('/').ok_or("隔离 RPC 根地址无效")?;
    if original.matches(formal_origin).count() != 3 {
        return Err("正式页面地址绑定数量漂移");
    }
    let expected = original
        .replace(formal_origin, isolated_origin)
        .replace("<h1>公民链</h1>", "<h1>公民链 · 隔离测试链</h1>");
    if page != expected {
        return Err("隔离页面业务内容或地址绑定漂移");
    }
    Ok(url.origin().ascii_serialization())
}

fn session_server_name() -> Result<Option<String>, &'static str> {
    let value = match std::env::var("CITIZENCHAIN_TEST_RPC_SERVER_NAME") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(_) => return Err("测试 RPC 证书名不是 UTF-8"),
    };
    if value.is_empty()
        || value.len() > 253
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err("测试 RPC 证书名必须是无端口和路径的 DNS 主机名");
    }
    if std::env::var_os("CITIZENCHAIN_TEST_SESSION_ROOT").is_none()
        || std::env::var_os("CITIZENCHAIN_TEST_RPC_CA").is_none()
    {
        return Err("可信服务器 TLS 仅允许显式测试会话并交付验证 CA");
    }
    Ok(Some(value))
}

/// 测试失败、超时或取消时也关闭监听与出块任务，避免占用后续验收资源。
struct AbortSessionTask<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortSessionTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[test]
fn metamask_session_settings_reject_invalid_ports_pages_and_duration() {
    for value in ["1", "18081", "65535"] {
        assert!(session_port(value).is_ok());
    }
    for value in ["", "0", "65536", "-1", "1.5", "018081", " 18081"] {
        assert!(session_port(value).is_err());
    }
    let original = std::str::from_utf8(nodedesktop::tls::INSTALL_PAGE).unwrap();
    let origin = original
        .split("value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .trim_end_matches('/');
    let page = original
        .replace(origin, "https://isolated.example")
        .replace("<h1>公民链</h1>", "<h1>公民链 · 隔离测试链</h1>");
    assert_eq!(
        session_page(page.as_bytes()).unwrap(),
        "https://isolated.example"
    );
    for invalid in [
        original.to_owned(),
        page.replace("wallet_addEthereumChain", "wallet_requestSnaps"),
        page.replace("https://isolated.example", "http://isolated.example"),
        page.replace(
            "url.origin !== 'https://isolated.example'",
            &format!("url.origin !== '{origin}'"),
        ),
        page.clone() + "<!-- changed -->",
    ] {
        assert!(session_page(invalid.as_bytes()).is_err());
    }
    for value in ["1", "5", "1800"] {
        assert!(session_seconds(value).is_ok());
    }
    for value in ["", "0", "1801", "-1", "1.5", "18446744073709551616"] {
        assert!(session_seconds(value).is_err(), "{value}");
    }
}

async fn session_receipt(
    http: &reqwest::Client,
    url: &str,
    transaction: &serde_json::Value,
) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let receipt = rpc(
                http,
                url,
                "eth_getTransactionReceipt",
                serde_json::json!([transaction]),
            )
            .await;
            if !receipt.is_null() {
                assert_eq!(receipt["status"], "0x1", "测试账户转账必须真实成功");
                return receipt;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("交易池中的签名交易必须经真实出块取得成功回执")
}

/// 独立读取同一原生账本总余额，与 Ethereum 展示核对；实际可支配金额另行校验。
fn session_native_balance(client: &FullClient, account: &sp_runtime::AccountId32) -> u128 {
    use sc_client_api::StorageProvider;
    let key = frame_system::Account::<runtime::Runtime>::hashed_key_for(account);
    let data = client
        .storage(client.info().best_hash, &sp_storage::StorageKey(key))
        .unwrap()
        .expect("测试账户必须存在于真实原生账本");
    type AccountInfo = frame_system::AccountInfo<
        runtime::Nonce,
        <runtime::Runtime as frame_system::Config>::AccountData,
    >;
    let account = AccountInfo::decode_all(&mut &data.0[..]).unwrap();
    account
        .data
        .free
        .checked_add(account.data.reserved)
        .unwrap()
}

// Runtime专项消费自身已生成的WASM；节点默认测试不创建整链会话。
#[cfg(feature = "runtime-tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn metamask_three_node_session_uses_pool_pow_tls_and_grandpa() {
    use sc_transaction_pool_api::{
        ChainEvent, InPoolTransaction, MaintainedTransactionPool, TransactionPool,
    };
    use sp_runtime::transaction_validity::TransactionSource;
    let explicit_root = std::env::var_os("CITIZENCHAIN_TEST_SESSION_ROOT");
    // 普通自动化直接使用标准库默认临时目录；沿用本轮唯一子目录及排他创建。
    let root = explicit_root
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "citizenchain-session-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        });
    let seconds = session_seconds(
        &std::env::var("CITIZENCHAIN_TEST_SESSION_SECONDS").unwrap_or_else(|_| "1".into()),
    )
    .unwrap();
    let port = match std::env::var("CITIZENCHAIN_TEST_RPC_PORT") {
        Ok(value) => {
            assert!(explicit_root.is_some(), "固定测试端口必须属于显式会话");
            session_port(&value).unwrap()
        }
        Err(std::env::VarError::NotPresent) if explicit_root.is_none() => 0,
        Err(_) => panic!("显式测试会话必须交付规范的 CITIZENCHAIN_TEST_RPC_PORT"),
    };
    let server_name = session_server_name().unwrap();
    std::fs::create_dir(&root).expect("只创建本轮全新会话，禁止覆盖旧数据库");
    // 这两个固定密钥是公开、可复现的测试向量，永远不能控制正式资产。
    let accounts = [
        Account::from_secret_key([1; 32]),
        Account::from_secret_key([2; 32]),
    ];
    let initial = 1_000_000u128; // 每个账户 10000 GMB，真实账本按分保存。
    let endowed = accounts
        .iter()
        .map(|account| (account.substrate_account(), initial))
        .collect::<Vec<_>>();
    println!("三节点会话：从源码 WASM 计算本轮测试创世状态");
    let mut session_spec: Box<dyn sc_service::ChainSpec> =
        Box::new(chain_spec_with_accounts(&endowed));
    let storage = session_spec.as_storage_builder().build_storage().unwrap();
    assert_eq!(
        storage
            .top
            .get(sp_core::storage::well_known_keys::CODE)
            .map(Vec::as_slice),
        runtime::WASM_BINARY,
        "测试创世必须包含本轮准确源码 WASM"
    );
    // 同一会话只执行一次创世构造；三个数据库独立导入相同、已核对源码的 storage。
    session_spec.set_storage(storage);
    let mut configs = ["primary", "peer-two", "peer-three"].map(|name| {
        let mut config = test_config_at(name, tokio::runtime::Handle::current(), root.join(name));
        config.chain_spec = session_spec.cloned_box();
        config
    });
    configs[0].role = Role::Authority;
    println!("三节点会话：初始化主节点的源码 WASM 与独立数据库");
    let partial = crate::service::testing::new_partial_for_test(&configs[0]).unwrap();
    let pool = partial.transaction_pool.clone();
    let client = partial.client.clone();
    let identity = nodeguard::guard::testing::TestGenesisIdentity::from_chain_spec(
        configs[0].chain_spec.as_ref(),
    )
    .unwrap();
    let raw_import = sc_consensus_pow::PowBlockImport::new(
        partial.other.0.clone(),
        client.clone(),
        nodemining::pow::SimplePow::new(client.clone()),
        0,
        partial.select_chain.clone(),
        |_, ()| async { Ok((sp_timestamp::InherentDataProvider::from_system_time(),)) },
    );
    let genesis = nodeguard::guard::testing::TestGenesisIdentity::genesis_state(
        configs[0].chain_spec.as_ref(),
    );
    let guarded = nodeguard::guard::NodeGuard::new_for_test(
        raw_import,
        client.clone(),
        partial.backend.clone(),
        identity,
        genesis.clone(),
    );
    let importer = nodeguard::constitution::guard::ConstitutionGuard::new(
        guarded,
        client.clone(),
        partial.backend.clone(),
        genesis,
    );
    let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    // 只导出公开测试证书。RPC 私钥始终保留内存，浏览器信任在后续步骤配置。
    let mut builder = reqwest::Client::builder().https_only(true).no_proxy();
    if let Some(name) = &server_name {
        let ca_path =
            std::path::PathBuf::from(std::env::var_os("CITIZENCHAIN_TEST_RPC_CA").unwrap());
        assert!(ca_path.is_absolute() && ca_path.canonicalize().unwrap() == ca_path);
        let ca = std::fs::read(ca_path).unwrap();
        builder = builder
            .add_root_certificate(reqwest::Certificate::from_pem(&ca).unwrap())
            .resolve(name, (std::net::Ipv4Addr::LOCALHOST, 0).into());
    } else {
        std::fs::write(
            root.join("rpc-certificate.der"),
            credentials.cert.der().as_ref(),
        )
        .unwrap();
        builder = builder.add_root_certificate(
            reqwest::Certificate::from_der(credentials.cert.der().as_ref()).unwrap(),
        );
    }
    let http = builder.timeout(Duration::from_secs(15)).build().unwrap();
    let (url, rpc_task) = serve_on_port(&partial, &credentials, port).await;
    let mut rpc_tasks = vec![AbortSessionTask(rpc_task)];
    let mut primary = start_test_network(&mut configs[0], partial, true);
    let mut peers = Vec::new();
    let mut urls = vec![url.clone()];
    for config in &mut configs[1..] {
        println!("三节点会话：初始化 {}", config.network.node_name);
        let partial = crate::service::testing::new_partial_for_test(config).unwrap();
        let (peer_url, task) = serve(&partial, &credentials).await;
        rpc_tasks.push(AbortSessionTask(task));
        urls.push(peer_url);
        let mut peer = start_test_network(config, partial, false);
        connect_peer(&peer, &primary).await;
        peer.start_finality();
        peers.push(peer);
    }
    primary.start_finality();
    println!("三节点会话：TLS P2P 已连接，核对原生与 Ethereum 同一账户总余额");
    for (node, endpoint) in urls.iter().enumerate() {
        let endpoint_client = if node == 0 {
            &client
        } else {
            &peers[node - 1].client
        };
        assert_eq!(
            rpc(&http, endpoint, "eth_chainId", serde_json::json!([])).await,
            "0x7eb"
        );
        for account in &accounts {
            assert_eq!(
                session_native_balance(endpoint_client, &account.substrate_account()),
                initial
            );
            let balance: U256 = serde_json::from_value(
                rpc(
                    &http,
                    endpoint,
                    "eth_getBalance",
                    serde_json::json!([account.address(), "latest"]),
                )
                .await,
            )
            .unwrap();
            assert_eq!(
                balance,
                U256::from(initial) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
            );
            assert_eq!(
                rpc(
                    &http,
                    endpoint,
                    "eth_getTransactionCount",
                    serde_json::json!([account.address(), "latest"])
                )
                .await,
                "0x0"
            );
        }
    }
    let (stop_authoring, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let author_pool = pool.clone();
    let author_client = client.clone();
    let synchronization = primary.sync.clone();
    let mut authoring = AbortSessionTask(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        let mut previous_timestamp = 0;
        loop {
            tokio::select! {
                _ = &mut stopped => return Ok::<(), String>(()),
                _ = interval.tick() => {}
            }
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_millis() as u64;
            if timestamp < previous_timestamp + 12_000 {
                continue;
            }
            // 只消费真实交易池中的 ready 交易；空池不制造空块或修改账户 nonce。
            let body = author_pool
                .ready()
                .take(1)
                .map(|transaction| (**transaction.data()).clone())
                .collect::<Vec<_>>();
            if body.is_empty() {
                continue;
            }
            let block = proposal(
                &author_client,
                author_client.info().best_hash,
                body,
                timestamp,
            );
            let hash = block.post_hash();
            let result = importer
                .import_block(block)
                .await
                .map_err(|error| format!("{error:?}"))?;
            if !matches!(result, ImportResult::Imported(_)) {
                return Err(format!("正式守卫拒绝测试交易区块：{result:?}"));
            }
            author_pool
                .maintain(ChainEvent::NewBestBlock {
                    hash,
                    tree_route: None,
                })
                .await;
            synchronization.announce_block(hash, None);
            previous_timestamp = timestamp;
        }
    }));
    let price: U256 =
        serde_json::from_value(rpc(&http, &url, "eth_gasPrice", serde_json::json!([])).await)
            .unwrap();
    assert_eq!(price, U256::from(1_000_000_000u64));
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_maxPriorityFeePerGas",
            serde_json::json!([])
        )
        .await,
        "0x0"
    );
    // 非整分金额和无效价格参数必须在真实 WASM 报价阶段拒绝，不能进入交易池。
    for invalid in [
        serde_json::json!({"value":U256::one()}),
        serde_json::json!({"gasPrice":price - U256::one()}),
        serde_json::json!({"maxFeePerGas":price - U256::one(),"maxPriorityFeePerGas":"0x0"}),
        serde_json::json!({"maxFeePerGas":price,"maxPriorityFeePerGas":price + U256::one()}),
    ] {
        let mut request = invalid;
        request["from"] = serde_json::to_value(accounts[0].address()).unwrap();
        request["to"] = serde_json::to_value(accounts[1].address()).unwrap();
        assert!(
            response(&http, &url, "eth_estimateGas", serde_json::json!([request]))
                .await
                .get("error")
                .is_some()
        );
    }
    assert_eq!(
        session_native_balance(&client, &accounts[0].substrate_account()),
        initial
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionCount",
            serde_json::json!([accounts[0].address(), "latest"])
        )
        .await,
        "0x0"
    );
    let transfers = [
        (0, 1, 100u128, 10u128),
        (1, 0, 10_499, 10),
        (0, 1, 10_500, 11),
        (1, 0, 100_000, 100),
    ];
    let mut receipts = Vec::new();
    let mut transactions = Vec::new();
    let mut nonces = [0u32; 2];
    for (index, &(sender, recipient, value, fee)) in transfers.iter().enumerate() {
        println!("三节点会话：广播并等待第 {} 笔真实转账回执", index + 1);
        // 报价和签名携带同一钱包字段；非零优先费不能仅在签名替身中覆盖。
        let eip1559 = index % 2 == 1;
        let priority = if index == 1 {
            U256::one()
        } else {
            U256::from(1_000_000u64)
        };
        let mut request = serde_json::json!({"from":accounts[sender].address(),
                "to":accounts[recipient].address(),
                "value":U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)});
        if eip1559 {
            request["maxFeePerGas"] = serde_json::to_value(price * U256::from(2)).unwrap();
            request["maxPriorityFeePerGas"] = serde_json::to_value(priority).unwrap();
        } else {
            request["gasPrice"] = serde_json::to_value(price + U256::one()).unwrap();
        }
        let estimate: U256 = serde_json::from_value(
            rpc(&http, &url, "eth_estimateGas", serde_json::json!([request])).await,
        )
        .unwrap();
        assert!(
            estimate * price
                >= U256::from(fee) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
        );
        // 真实钱包按报价增加 gas 缓冲；Legacy 与 EIP-1559 均不能改变实际业务费。
        let tx = GenericTransaction {
            from: Some(accounts[sender].address()),
            to: Some(accounts[recipient].address()),
            chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
            nonce: Some(nonces[sender].into()),
            gas: Some(estimate * U256::from(2)),
            gas_price: if eip1559 {
                None
            } else {
                Some(price + U256::one())
            },
            max_fee_per_gas: if eip1559 {
                Some(price * U256::from(2))
            } else {
                None
            },
            max_priority_fee_per_gas: if eip1559 { Some(priority) } else { None },
            value: Some(
                U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
            ),
            r#type: Some(
                if eip1559 {
                    pallet_revive::evm::TYPE_EIP1559
                } else {
                    pallet_revive::evm::TYPE_LEGACY
                }
                .into(),
            ),
            ..Default::default()
        };
        let (raw, _) = signed_transaction(&accounts[sender], tx);
        let hash = rpc(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([raw]),
        )
        .await;
        receipts.push(session_receipt(&http, &url, &hash).await);
        transactions.push(hash);
        nonces[sender] += 1;
    }
    // 用独立的公开 Alice 账户产生第三个合法块，验证真实最终性且不扰动测试账户。
    pool.submit_one(
        client.info().best_hash,
        TransactionSource::Local,
        native_remark(&client, 0),
    )
    .await
    .unwrap();
    wait_block(&http, &url, 5).await;
    let hash = client.info().best_hash;
    for peer in &peers {
        wait_peer(peer, hash).await;
    }
    wait_finalized(&primary, hash, 5).await;
    for peer in &peers {
        wait_finalized(peer, hash, 5).await;
    }
    let head = rpc(
        &http,
        &url,
        "eth_getBlockByNumber",
        serde_json::json!(["latest", false]),
    )
    .await;
    for endpoint in &urls {
        assert_finalized_rpc(&http, endpoint, &head).await;
    }
    let mut fees = [0u128; 2];
    let mut expected = [initial; 2];
    for (index, receipt) in receipts.iter().enumerate() {
        // Ethereum 回执哈希与原生头哈希不同；按已验证的同一高度读取原生费用事件。
        let number = (index + 1) as u32;
        assert_eq!(receipt["blockNumber"], format!("0x{number:x}"));
        let ethereum_block = rpc(
            &http,
            &url,
            "eth_getBlockByNumber",
            serde_json::json!([format!("0x{number:x}"), false]),
        )
        .await;
        assert_eq!(ethereum_block["hash"], receipt["blockHash"]);
        let block_hash = client
            .hash(number)
            .unwrap()
            .expect("成功转账必须具有同高度原生区块");
        let paid = paid_fees(&client, block_hash);
        assert_eq!(
            paid.len(),
            1,
            "一笔 Ethereum 转账只能产生一次真实手续费事件"
        );
        let (sender, recipient, value, fee) = transfers[index];
        assert_eq!(paid[0], (accounts[sender].substrate_account(), fee));
        let used: U256 = serde_json::from_value(receipt["gasUsed"].clone()).unwrap();
        let effective: U256 = serde_json::from_value(receipt["effectiveGasPrice"].clone()).unwrap();
        assert_eq!(effective, price);
        assert_eq!(
            used * effective,
            U256::from(fee) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
        );
        assert_eq!(
            ethereum_block["baseFeePerGas"],
            serde_json::to_value(price).unwrap()
        );
        fees[sender] += fee;
        expected[sender] -= value + fee;
        expected[recipient] += value;
    }
    let history = rpc(
        &http,
        &url,
        "eth_feeHistory",
        serde_json::json!(["0x1", "latest", [0, 50, 100]]),
    )
    .await;
    assert_eq!(history["baseFeePerGas"], serde_json::json!([price, price]));
    assert_eq!(
        history["reward"],
        serde_json::json!([["0x0", "0x0", "0x0"]])
    );
    for (node, endpoint) in urls.iter().enumerate() {
        let endpoint_client = if node == 0 {
            &client
        } else {
            &peers[node - 1].client
        };
        for (index, account) in accounts.iter().enumerate() {
            assert_eq!(
                session_native_balance(endpoint_client, &account.substrate_account()),
                expected[index]
            );
            let balance: U256 = serde_json::from_value(
                rpc(
                    &http,
                    endpoint,
                    "eth_getBalance",
                    serde_json::json!([account.address(), "latest"]),
                )
                .await,
            )
            .unwrap();
            assert_eq!(
                balance,
                U256::from(expected[index])
                    * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
            );
            assert_eq!(
                rpc(
                    &http,
                    endpoint,
                    "eth_getTransactionCount",
                    serde_json::json!([account.address(), "latest"])
                )
                .await,
                "0x2"
            );
        }
        for (transaction, receipt) in transactions.iter().zip(&receipts) {
            assert_eq!(
                rpc(
                    &http,
                    endpoint,
                    "eth_getTransactionReceipt",
                    serde_json::json!([transaction])
                )
                .await,
                *receipt
            );
        }
    }
    let ready = serde_json::json!({"rpc":url,"chain_id":2027,"genesis_hash":client.info().genesis_hash,
            "runtime_wasm_blake2_256":sp_core::H256::from(sp_crypto_hashing::blake2_256(runtime::WASM_BINARY.unwrap())),
            "nodes":3,"finalized_hash":hash,"ethereum_finalized_hash":head["hash"],"finalized_number":5,
            "accounts":accounts.iter().enumerate().map(|(index, account)| serde_json::json!({
                "address":account.address(),"account_id":format!("0x{}", hex::encode(account.substrate_account())),
                "initial_fen":initial,"balance_fen":expected[index],
                "ethereum_balance_fen":expected[index],
                "existential_deposit_fen":runtime::EXISTENTIAL_DEPOSIT,"fee_fen":fees[index],"nonce":nonces[index]
            })).collect::<Vec<_>>(),"transactions":transactions,"hold_seconds":seconds});
    std::fs::write(
        root.join("ready.json"),
        serde_json::to_vec_pretty(&ready).unwrap(),
    )
    .unwrap();
    println!("测试链真实就绪：{ready}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while tokio::time::Instant::now() < deadline {
        assert!(!authoring.0.is_finished(), "测试出块任务提前结束");
        assert!(
            rpc_tasks.iter().all(|task| !task.0.is_finished()),
            "测试 RPC 提前结束"
        );
        let stop = root.join("stop");
        if let Ok(metadata) = std::fs::symlink_metadata(&stop) {
            assert!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "停止请求必须是普通文件"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    stop_authoring.send(()).unwrap();
    (&mut authoring.0).await.unwrap().unwrap();
    for task in &mut rpc_tasks {
        task.0.abort();
        let _ = (&mut task.0).await;
    }
    // 出块任务已持有并释放importer；这里释放外部交易池和客户端持有者。
    drop(pool);
    drop(client);
    for node in peers.into_iter().chain([primary]) {
        let backend = Arc::downgrade(&node.backend);
        let tasks = node.stop();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !tasks.running_tasks().is_empty() || backend.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("三节点测试任务及数据库必须释放后才能清理会话");
    }
    let port = url.rsplit_once(':').unwrap().1.parse::<u16>().unwrap();
    let released = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .expect("停止后必须释放原 RPC 端口");
    drop(released);
    std::fs::write(root.join("stopped.json"), br#"{"stopped":true}"#).unwrap();
    println!("测试会话已停止，监听及三节点服务已关闭");
    if explicit_root.is_none() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn standard_subscription_kinds_and_parameter_boundaries_are_enforced() {
    use jsonrpsee::types::Params;
    assert_eq!(
        subscription_request(&Params::new(Some(r#"["newHeads"]"#)))
            .unwrap()
            .0,
        SubscriptionKind::NewBlockHeaders
    );
    assert_eq!(
        subscription_request(&Params::new(Some(r#"["logs",{}]"#)))
            .unwrap()
            .0,
        SubscriptionKind::Logs
    );
    for invalid in [
        r#"[]"#,
        r#"["newBlockHeaders"]"#,
        r#"["unknown"]"#,
        r#"["logs",1]"#,
        r#"["logs",{},null]"#,
    ] {
        assert!(
            subscription_request(&Params::new(Some(invalid))).is_err(),
            "{invalid}"
        );
    }
}

#[tokio::test]
async fn pending_mapping_reads_native_storage_and_rejects_corrupt_accounts() {
    let address = H160::from_low_u64_be(42);
    let mapped = sp_runtime::AccountId32::new([7; 32]);
    // 没有 Externalities 的宿主环境也必须正确读取已有映射，并对损坏编码拒绝服务。
    for (raw, expected) in [
        (
            None,
            Some(
                pallet_revive::AccountId32Mapper::<runtime::Runtime>::to_fallback_account_id(
                    &address,
                ),
            ),
        ),
        (Some(mapped.encode()), Some(mapped.clone())),
        (Some(vec![7; 31]), None),
        (Some(vec![7; 33]), None),
    ] {
        let mut native = RpcModule::new(());
        native
            .register_method("state_getStorage", move |_, _, _| {
                raw.clone().map(sp_core::Bytes)
            })
            .unwrap();
        let actual = account_at(&native, address, None).await;
        match expected {
            Some(expected) => assert_eq!(actual.unwrap(), expected),
            None => assert!(actual.is_err(), "损坏映射不能回退到另一账户"),
        }
    }
    assert!(
        account_at(&RpcModule::new(()), address, None)
            .await
            .is_err(),
        "存储读取失败不能视为无映射"
    );
}

#[cfg(feature = "runtime-tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_transactions_receipts_logs_finality_reorg_and_restart_over_real_tls_wasm() {
    let mut config = test_config("ethereum", tokio::runtime::Handle::current());
    config.role = Role::Authority;
    let root = config.base_path.path().to_path_buf();
    let identity =
        nodeguard::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .unwrap();
    let partial = crate::service::testing::new_partial_for_test(&config).unwrap();
    let raw_import = sc_consensus_pow::PowBlockImport::new(
        partial.other.0.clone(),
        partial.client.clone(),
        nodemining::pow::SimplePow::new(partial.client.clone()),
        0,
        partial.select_chain.clone(),
        |_, ()| async { Ok((sp_timestamp::InherentDataProvider::from_system_time(),)) },
    );
    let genesis =
        nodeguard::guard::testing::TestGenesisIdentity::genesis_state(config.chain_spec.as_ref());
    let guarded = nodeguard::guard::NodeGuard::new_for_test(
        raw_import,
        partial.client.clone(),
        partial.backend.clone(),
        identity,
        genesis.clone(),
    );
    let importer = nodeguard::constitution::guard::ConstitutionGuard::new(
        guarded,
        partial.client.clone(),
        partial.backend.clone(),
        genesis,
    );
    let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let http = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .add_root_certificate(
            reqwest::Certificate::from_der(credentials.cert.der().as_ref()).unwrap(),
        )
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let (url, server) = serve(&partial, &credentials).await;
    let mut partial = start_test_network(&mut config, partial, true);
    let mut peer_config = test_config("ethereum-peer", tokio::runtime::Handle::current());
    let peer_root = peer_config.base_path.path().to_path_buf();
    let peer_partial = crate::service::testing::new_partial_for_test(&peer_config).unwrap();
    let (peer_url, peer_server) = serve(&peer_partial, &credentials).await;
    let mut peer = start_test_network(&mut peer_config, peer_partial, false);
    connect_peer(&peer, &partial).await;
    assert_eq!(
        rpc(&http, &url, "eth_chainId", serde_json::json!([])).await,
        "0x7eb"
    );
    assert_eq!(
        rpc(&http, &url, "eth_accounts", serde_json::json!([])).await,
        serde_json::json!([])
    );
    let signer = Account::default().address();
    let before = rpc(
        &http,
        &url,
        "eth_getBalance",
        serde_json::json!([signer, "latest"]),
    )
    .await;
    let code = vec![
        0x60, 42, 0x60, 0, 0x55, 0x60, 42, 0x60, 0, 0x52, 0x60, 7, 0x60, 32, 0x60, 0, 0xa1, 0x00,
    ];
    let mut init = vec![
        0x60,
        code.len() as u8,
        0x60,
        12,
        0x60,
        0,
        0x39,
        0x60,
        code.len() as u8,
        0x60,
        0,
        0xf3,
    ];
    init.extend_from_slice(&code);
    let (deploy_raw, deploy_xt) = transaction(0, None, init, 200);
    let deployment = rpc(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([deploy_raw]),
    )
    .await;
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionCount",
            serde_json::json!([signer, "pending"])
        )
        .await,
        "0x1"
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionCount",
            serde_json::json!([signer, "latest"])
        )
        .await,
        "0x0"
    );
    let block1 = proposal(
        &partial.client,
        partial.client.info().genesis_hash,
        vec![deploy_xt],
        1_782_950_406_000,
    );
    let hash1 = block1.post_hash();
    assert!(matches!(
        importer.import_block(block1).await.unwrap(),
        ImportResult::Imported(_)
    ));
    wait_block(&http, &url, 1).await;
    partial.sync.announce_block(hash1, None);
    wait_peer(&peer, hash1).await;
    wait_block(&http, &peer_url, 1).await;
    let receipt = rpc(
        &http,
        &url,
        "eth_getTransactionReceipt",
        serde_json::json!([deployment]),
    )
    .await;
    assert_eq!(receipt["status"], "0x1");
    assert_eq!(receipt["transactionIndex"], "0x0");
    let contract: H160 = serde_json::from_value(receipt["contractAddress"].clone()).unwrap();
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getCode",
            serde_json::json!([contract, "latest"])
        )
        .await,
        format!("0x{}", hex::encode(&code))
    );
    assert_peer_state(
        &http,
        &url,
        &peer_url,
        signer,
        contract,
        &[deployment.clone()],
    )
    .await;
    let balance: U256 = serde_json::from_value(before).unwrap();
    let after: U256 = serde_json::from_value(
        rpc(
            &http,
            &url,
            "eth_getBalance",
            serde_json::json!([signer, "latest"]),
        )
        .await,
    )
    .unwrap();
    assert_eq!(
        balance - after,
        U256::from(
            200 + runtime::EXISTENTIAL_DEPOSIT + primitives::fee_policy::calculate_onchain_fee(200)
        ) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
    );
    assert_eq!(
        paid_fees(&partial.client, hash1),
        vec![(
            Account::default().substrate_account(),
            primitives::fee_policy::calculate_onchain_fee(200)
        )]
    );
    assert_eq!(
        serde_json::from_value::<U256>(
            rpc(
                &http,
                &url,
                "eth_getBalance",
                serde_json::json!([contract, "latest"])
            )
            .await
        )
        .unwrap(),
        U256::from(200 + runtime::EXISTENTIAL_DEPOSIT)
            * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
        "合约展示总余额包含转入金额和原生账户保留金"
    );
    let native_contract =
        pallet_revive::AccountId32Mapper::<runtime::Runtime>::to_fallback_account_id(&contract);
    assert_eq!(
        session_native_balance(&partial.client, &native_contract),
        200 + runtime::EXISTENTIAL_DEPOSIT
    );
    // 保留原2.00 GMB可支出额断言，直接核对未改动的执行API；总额展示不释放ED。
    let execution: sp_core::Bytes = serde_json::from_value(
        rpc(
            &http,
            &url,
            "state_call",
            serde_json::json!([
                "ReviveApi_balance",
                format!("0x{}", hex::encode(contract.encode())),
                hash1
            ]),
        )
        .await,
    )
    .unwrap();
    let spendable = U256::decode_all(&mut execution.0.as_slice()).unwrap();
    assert_eq!(
        spendable,
        U256::from(200) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
        "合约执行余额仍排除账户保留金"
    );
    let storage = rpc(
        &http,
        &url,
        "eth_getStorageAt",
        serde_json::json!([contract, "0x0", "latest"]),
    )
    .await;
    let _ = rpc(
        &http,
        &url,
        "eth_call",
        serde_json::json!([{"from":signer,"to":contract,"data":"0x"},"latest"]),
    )
    .await;
    let gas = rpc(
        &http,
        &url,
        "eth_estimateGas",
        serde_json::json!([{"from":signer,"to":contract,"data":"0x"}]),
    )
    .await;
    assert!(serde_json::from_value::<U256>(gas).unwrap() > U256::zero());
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getStorageAt",
            serde_json::json!([contract, "0x0", "latest"])
        )
        .await,
        storage
    );
    assert_eq!(
        serde_json::from_value::<U256>(
            rpc(
                &http,
                &url,
                "eth_getBalance",
                serde_json::json!([signer, "latest"])
            )
            .await
        )
        .unwrap(),
        after
    );
    let (call_raw, call_xt) = transaction(1, Some(contract), vec![], 0);
    let call_hash = rpc(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([call_raw]),
    )
    .await;
    let branch_a = proposal(
        &partial.client,
        hash1,
        vec![call_xt.clone()],
        1_782_950_412_000,
    );
    let hash_a = branch_a.post_hash();
    importer.import_block(branch_a).await.unwrap();
    wait_block(&http, &url, 2).await;
    partial.sync.announce_block(hash_a, None);
    wait_peer(&peer, hash_a).await;
    wait_block(&http, &peer_url, 2).await;
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([call_hash])
        )
        .await["status"],
        "0x1"
    );
    assert_peer_state(
        &http,
        &url,
        &peer_url,
        signer,
        contract,
        &[deployment.clone(), call_hash.clone()],
    )
    .await;
    // 同高不同最佳链真实重组：替换后旧回执、日志与状态必须消失。
    let branch_b = proposal(
        &partial.client,
        hash1,
        vec![native_remark(&partial.client, 0)],
        1_782_950_418_000,
    );
    let hash2 = branch_b.post_hash();
    importer.import_block(branch_b).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([call_hash]),
        )
        .await
        .is_null()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getLogs",
            serde_json::json!([{"fromBlock":"0x2","toBlock":"0x2","address":contract}])
        )
        .await,
        serde_json::json!([])
    );
    let mut roots = rustls::RootCertStore::empty();
    roots.add(credentials.cert.der().clone()).unwrap();
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let ws = jsonrpsee::ws_client::WsClientBuilder::new()
        .with_custom_cert_store(tls)
        .build(&url.replace("https:", "wss:"))
        .await
        .unwrap();
    let mut heads = ws
        .subscribe::<serde_json::Value, _>(
            "eth_subscribe",
            jsonrpsee::rpc_params!["newHeads"],
            "eth_unsubscribe",
        )
        .await
        .unwrap();
    let mut logs = ws
        .subscribe::<serde_json::Value, _>(
            "eth_subscribe",
            jsonrpsee::rpc_params!["logs", serde_json::json!({"address":contract})],
            "eth_unsubscribe",
        )
        .await
        .unwrap();
    let (fail_raw, fail_xt) = transaction(2, None, vec![0x60, 0, 0x60, 0, 0xfd], 200);
    let fail_hash = rpc(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([fail_raw]),
    )
    .await;
    let remark = native_remark(&partial.client, 1);
    let block3 = proposal(
        &partial.client,
        hash2,
        vec![remark, call_xt, fail_xt],
        1_782_950_424_000,
    );
    let hash3 = block3.post_hash();
    importer.import_block(block3).await.unwrap();
    wait_block(&http, &url, 3).await;
    // 同高分支不强迫跟随节点切换；更重分支经网络导入后才验证旧回执替换。
    partial.sync.announce_block(hash3, None);
    wait_peer(&peer, hash3).await;
    wait_block(&http, &peer_url, 3).await;
    assert_peer_state(
        &http,
        &url,
        &peer_url,
        signer,
        contract,
        &[deployment.clone(), call_hash.clone(), fail_hash.clone()],
    )
    .await;
    assert_eq!(
        paid_fees(&peer.client, hash3),
        paid_fees(&partial.client, hash3)
    );
    let head = tokio::time::timeout(Duration::from_secs(10), heads.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(head["number"], "0x3");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), logs.next())
            .await
            .is_err(),
        "最佳链日志不得冒充最终日志"
    );
    let success = rpc(
        &http,
        &url,
        "eth_getTransactionReceipt",
        serde_json::json!([call_hash]),
    )
    .await;
    let failure = rpc(
        &http,
        &url,
        "eth_getTransactionReceipt",
        serde_json::json!([fail_hash]),
    )
    .await;
    assert_eq!(success["transactionIndex"], "0x0");
    assert_eq!(success["logs"][0]["logIndex"], "0x0");
    assert_eq!(failure["transactionIndex"], "0x1");
    assert_eq!(failure["status"], "0x0");
    assert!(failure["contractAddress"].is_null());
    assert_eq!(failure["logs"], serde_json::json!([]));
    let first: U256 = serde_json::from_value(success["gasUsed"].clone()).unwrap();
    let second: U256 = serde_json::from_value(failure["gasUsed"].clone()).unwrap();
    assert_eq!(
        serde_json::from_value::<U256>(failure["cumulativeGasUsed"].clone()).unwrap(),
        first + second
    );
    // 分叉验收完成之前不能固化旧分支；最佳块通知不构成最终性证明。
    assert_eq!(partial.client.info().finalized_number, 0);
    assert_eq!(peer.client.info().finalized_number, 0);
    peer.start_finality();
    partial.start_finality();
    wait_finalized(&partial, hash3, 3).await;
    wait_finalized(&peer, hash3, 3).await;
    let log = tokio::time::timeout(Duration::from_secs(10), logs.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(log["transactionHash"], call_hash);
    let finalized3 = rpc(
        &http,
        &url,
        "eth_getBlockByNumber",
        serde_json::json!(["0x3", false]),
    )
    .await;
    assert_eq!(finalized3["number"], "0x3");
    for node_url in [&url, &peer_url] {
        assert_finalized_rpc(&http, node_url, &finalized3).await;
    }
    assert!(response(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([deploy_raw])
    )
    .await
    .get("error")
    .is_some());
    let after_calls: U256 = serde_json::from_value(
        rpc(
            &http,
            &url,
            "eth_getBalance",
            serde_json::json!([signer, "latest"]),
        )
        .await,
    )
    .unwrap();
    assert_eq!(
        after - after_calls,
        U256::from(
            primitives::fee_policy::calculate_onchain_fee(0)
                + primitives::fee_policy::calculate_onchain_fee(200)
        ) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
    );
    assert_eq!(
        paid_fees(&partial.client, hash3)
            .into_iter()
            .filter(|(account_id, _)| *account_id == Account::default().substrate_account())
            .collect::<Vec<_>>(),
        vec![
            (
                Account::default().substrate_account(),
                primitives::fee_policy::calculate_onchain_fee(0)
            ),
            (
                Account::default().substrate_account(),
                primitives::fee_policy::calculate_onchain_fee(200)
            ),
        ],
        "成功调用和失败部署均只收一次既有原生费用"
    );
    // 迟到的第三个独立数据库从块0追块，不能复用前两个节点的数据库或索引。
    let mut late_config = test_config("ethereum-late", tokio::runtime::Handle::current());
    let late_root = late_config.base_path.path().to_path_buf();
    let late_partial = crate::service::testing::new_partial_for_test(&late_config).unwrap();
    assert_eq!(late_partial.client.info().best_number, 0);
    let (late_url, late_server) = serve(&late_partial, &credentials).await;
    let mut late = start_test_network(&mut late_config, late_partial, false);
    connect_peer(&late, &partial).await;
    partial.sync.announce_block(hash3, None);
    wait_peer(&late, hash3).await;
    wait_block(&http, &late_url, 3).await;
    late.start_finality();
    wait_finalized(&late, hash3, 3).await;
    assert_finalized_rpc(&http, &late_url, &finalized3).await;
    assert_peer_state(
        &http,
        &url,
        &late_url,
        signer,
        contract,
        &[deployment.clone(), call_hash.clone(), fail_hash.clone()],
    )
    .await;

    // 真正断开第二节点并释放 RocksDB；离线期间的新调用只在源节点执行和收费。
    peer_server.abort();
    let _ = peer_server.await;
    let peer_backend = Arc::downgrade(&peer.backend);
    let tasks = peer.stop();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !tasks.running_tasks().is_empty() || peer_backend.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("重启前跟随节点数据库必须释放");
    let (offline_raw, offline_xt) = transaction(3, Some(contract), vec![], 0);
    let offline_hash = rpc(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([offline_raw]),
    )
    .await;
    let block4 = proposal(&partial.client, hash3, vec![offline_xt], 1_782_950_430_000);
    let hash4 = block4.post_hash();
    importer.import_block(block4).await.unwrap();
    wait_block(&http, &url, 4).await;
    partial.sync.announce_block(hash4, None);
    wait_peer(&late, hash4).await;
    wait_block(&http, &late_url, 4).await;
    let peer_partial = crate::service::testing::new_partial_for_test(&peer_config).unwrap();
    assert_eq!(peer_partial.client.info().best_hash, hash3);
    let (peer_url, peer_server) = serve(&peer_partial, &credentials).await;
    let mut peer = start_test_network(&mut peer_config, peer_partial, false);
    peer.start_finality();
    connect_peer(&peer, &partial).await;
    wait_peer(&peer, hash4).await;
    wait_block(&http, &peer_url, 4).await;
    let finalized4 = rpc(
        &http,
        &url,
        "eth_getBlockByNumber",
        serde_json::json!(["0x4", false]),
    )
    .await;
    assert_eq!(finalized4["number"], "0x4");
    for node in [&partial, &peer, &late] {
        wait_finalized(node, hash4, 4).await;
    }
    for node_url in [&url, &peer_url, &late_url] {
        assert_finalized_rpc(&http, node_url, &finalized4).await;
    }
    for peer_url in [&peer_url, &late_url] {
        assert_peer_state(
            &http,
            &url,
            peer_url,
            signer,
            contract,
            &[
                deployment.clone(),
                call_hash.clone(),
                fail_hash.clone(),
                offline_hash.clone(),
            ],
        )
        .await;
    }
    assert_eq!(
        paid_fees(&peer.client, hash4),
        vec![(
            Account::default().substrate_account(),
            primitives::fee_policy::calculate_onchain_fee(0),
        )],
        "同步重放不产生第二份费用事件"
    );
    let after_offline: U256 = serde_json::from_value(
        rpc(
            &http,
            &peer_url,
            "eth_getBalance",
            serde_json::json!([signer, "latest"]),
        )
        .await,
    )
    .unwrap();
    assert_eq!(
        after_calls - after_offline,
        U256::from(primitives::fee_policy::calculate_onchain_fee(0))
            * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
    );
    // Receive through a native account's Ethereum alias, using a real signed
    // Ethereum transaction. The SS58/AccountId balance must change on all peers.
    let native_recipient = sp_runtime::AccountId32::new([0xa7; 32]);
    let native_address =
        <pallet_revive::AccountId32Mapper<runtime::Runtime> as pallet_revive::AddressMapper<
            runtime::Runtime,
        >>::to_address(&native_recipient);
    let native_before = session_native_balance(&partial.client, &native_recipient);
    let (alias_raw, alias_xt) = transaction(4, Some(native_address), vec![], 500);
    let alias_hash = rpc(
        &http,
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([alias_raw]),
    )
    .await;
    let alias_block = proposal(&partial.client, hash4, vec![alias_xt], 1_782_950_436_000);
    let alias_block_hash = alias_block.post_hash();
    importer.import_block(alias_block).await.unwrap();
    wait_block(&http, &url, 5).await;
    partial.sync.announce_block(alias_block_hash, None);
    for node in [&peer, &late] {
        wait_peer(node, alias_block_hash).await;
        assert_eq!(
            session_native_balance(&node.client, &native_recipient),
            native_before + 500
        );
    }
    assert_eq!(
        session_native_balance(&partial.client, &native_recipient),
        native_before + 500
    );
    let alias_receipt = rpc(
        &http,
        &url,
        "eth_getTransactionReceipt",
        serde_json::json!([alias_hash]),
    )
    .await;
    assert_eq!(alias_receipt["status"], "0x1");
    assert_eq!(alias_receipt["to"], serde_json::json!(native_address));
    assert_eq!(
        paid_fees(&partial.client, alias_block_hash),
        vec![(
            Account::default().substrate_account(),
            primitives::fee_policy::calculate_onchain_fee(500),
        )]
    );

    for (peer, peer_server) in [(peer, peer_server), (late, late_server)] {
        peer_server.abort();
        let _ = peer_server.await;
        let backend = Arc::downgrade(&peer.backend);
        let tasks = peer.stop();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !tasks.running_tasks().is_empty() || backend.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("清理前每个独立节点数据库必须释放");
    }
    drop(peer_config);
    drop(late_config);
    std::fs::remove_dir_all(peer_root).unwrap();
    std::fs::remove_dir_all(late_root).unwrap();
    // 真正关闭监听、同步任务和链数据库，再重新打开同一数据库重建近期索引。
    drop(heads);
    drop(logs);
    drop(ws);
    server.abort();
    let _ = server.await;
    drop(importer);
    let backend = Arc::downgrade(&partial.backend);
    let tasks = partial.stop();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !tasks.running_tasks().is_empty() || backend.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let restarted = crate::service::testing::new_partial_for_test(&config).unwrap();
    let (url, restarted_server) = serve(&restarted, &credentials).await;
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([deployment])
        )
        .await["status"],
        "0x1"
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([fail_hash])
        )
        .await["contractAddress"],
        serde_json::Value::Null
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getLogs",
            serde_json::json!([{"fromBlock":"0x1","toBlock":"0x4","address":contract}])
        )
        .await
        .as_array()
        .unwrap()
        .len(),
        2
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionCount",
            serde_json::json!([signer, "latest"])
        )
        .await,
        "0x5"
    );
    assert_eq!(
        rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([alias_hash])
        )
        .await["status"],
        "0x1",
    );
    assert_eq!(
        session_native_balance(&restarted.client, &native_recipient),
        native_before + 500
    );
    restarted_server.abort();
    let _ = restarted_server.await;
    let backend = Arc::downgrade(&restarted.backend);
    let tasks = stop_partial(restarted);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !tasks.running_tasks().is_empty() || backend.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(config);
    std::fs::remove_dir_all(root).unwrap();
}
