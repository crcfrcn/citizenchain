//! 通过实际区块导入验证时间戳、用户交易及制度守卫。

use super::*;

use crate::guard::has_user_transaction;
use crate::guard::import_if_verified;
use crate::guard::verify_finalize_issuance;
use codec::Encode;
use nodeclient::types::FullClient;
use runtime::opaque::Block;
use sc_chain_spec::ChainType;
use sc_chain_spec::Properties;
use sc_consensus::BlockCheckParams;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ImportResult;
use sc_network::config::NetworkConfiguration;
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
use sp_api::ProvideRuntimeApi;
use sp_consensus::BlockOrigin;
use sp_consensus::Error as ConsensusError;
use sp_consensus_pow::POW_ENGINE_ID;
use sp_core::crypto::Ss58AddressFormat;
use sp_core::crypto::Ss58Codec;
use sp_core::sr25519;
use sp_core::Pair as _;
use sp_keyring::Sr25519Keyring;
use sp_runtime::traits::Block as BlockT;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[test]
fn block_requires_timestamp_and_at_least_one_user_transaction() {
    assert!(!has_user_transaction(0));
    assert!(!has_user_transaction(1));
    assert!(has_user_transaction(2));
}

fn metadata_with_genesis_name(name: &str) -> Vec<u8> {
    let bytes = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = nodeclient::codec::decode_runtime_metadata(bytes.as_slice()).unwrap();
    let frame_metadata::RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("测试需要真实 V16 元数据");
    };
    let pallet = value
        .pallets
        .iter_mut()
        .find(|p| p.name == "Genesis")
        .unwrap();
    pallet.name = name.into();
    pallet.storage.as_mut().unwrap().prefix = name.into();
    frame_metadata::RuntimeMetadataPrefixed(0x6174_656d, metadata).encode()
}

#[test]
fn runtime_storage_uses_executing_metadata_and_preserves_immutable_guards() {
    // 名称由元数据决定，不能靠预设的新旧名称或版本回退通过这个用例。
    let metadata = metadata_with_genesis_name("ExecutedGenesis");
    let storage = RuntimeStorage::from_metadata(&metadata).unwrap();
    let key = |name: &[u8]| nodedesktop::storage::prefix(b"ExecutedGenesis", name);
    let mut state = BTreeMap::from([
        (
            key(b"CitizensDeclaration"),
            primitives::genesis::CITIZENS.as_bytes().to_vec().encode(),
        ),
        (
            key(b"CountryDeclaration"),
            primitives::genesis::COUNTRY.as_bytes().to_vec().encode(),
        ),
        (
            key(b"CitizenMax"),
            primitives::genesis::GENESIS_CITIZEN_MAX.encode(),
        ),
    ]);
    let untouched = state.clone();
    assert_eq!(
        genesis_pallet::check_full_state(|k| state.get(k).cloned()),
        Err(genesis_pallet::GuardError::CitizensDeclarationMissing),
    );
    let read = |k: &[u8]| state.get(&storage.key(k, true)).cloned();
    assert_eq!(
        genesis_pallet::check_transition(&BTreeMap::new(), &read, &read),
        Ok(())
    );
    let immutable = genesis_pallet::storage_key::citizens_declaration();
    let raw = BTreeMap::from([(
        key(b"CitizensDeclaration"),
        Some(state[&key(b"CitizensDeclaration")].clone()),
    )]);
    let changes = storage.changes(&raw).unwrap();
    assert_eq!(
        genesis_pallet::check_transition(&changes, &read, &read),
        Err(genesis_pallet::GuardError::ImmutableStorageTouched(
            immutable.clone()
        )),
    );
    assert_eq!(state, untouched);
    state.remove(&key(b"CitizensDeclaration"));
    // 候选名称下的同值记录不能冒充执行 Runtime 的真实存储。
    state.insert(
        immutable.clone(),
        primitives::genesis::CITIZENS.as_bytes().to_vec().encode(),
    );
    assert_eq!(
        genesis_pallet::check_full_state(|k| state.get(&storage.key(k, true)).cloned()),
        Err(genesis_pallet::GuardError::CitizensDeclarationMissing),
    );
    let mut collision = raw;
    collision.insert(immutable, None);
    assert!(storage.changes(&collision).is_err());
    let actual_key = key(b"CitizenMax");
    let rule_key = storage.key(&actual_key, false);
    assert_eq!(&actual_key[16..], &rule_key[16..]);
    assert_eq!(storage.key(&rule_key, true), actual_key);
    assert_eq!(storage.key(b":code", true), b":code");
}

#[test]
fn runtime_storage_rejects_invalid_or_ambiguous_metadata() {
    assert!(RuntimeStorage::from_metadata(b"invalid").is_err());
    let mut bytes = metadata_with_genesis_name("ExecutedGenesis");
    bytes.push(0);
    assert!(RuntimeStorage::from_metadata(&bytes).is_err());
    let bytes = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = nodeclient::codec::decode_runtime_metadata(bytes.as_slice()).unwrap();
    let frame_metadata::RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16")
    };
    value.pallets[1].index = value.pallets[0].index;
    assert!(RuntimeStorage::from_metadata(
        &frame_metadata::RuntimeMetadataPrefixed(0x6174_656d, metadata).encode()
    )
    .is_err());
    assert!(RuntimeStorage::from_metadata(&metadata_with_genesis_name("System")).is_err());
    let identity = RuntimeStorage::from_metadata(bytes.as_slice()).unwrap();
    let key = genesis_pallet::storage_key::citizen_max();
    assert_eq!(identity.key(&key, true), key);
}

/// 使用完整真实Wasm差量核对三阶段存储视图及永久事实；不把离线合成账户当作主链交易。
/// 归档初始准备合并到首个差量；PoW、费用及签名执行仍由独立真实导入用例验证。
#[test]
#[ignore = "通过塔塔显式交付完成的Wasm归档、原状态和本轮候选"]
fn replayed_migration_preserves_storage_views_and_permanent_facts() {
    use codec::DecodeAll;
    use sc_service::ChainSpec as _;
    use sp_core::traits::{CodeExecutor as _, RuntimeCode, WrappedRuntimeCode};
    use sp_runtime::traits::{Hash as _, Header as _};
    use std::io::BufRead as _;
    let input = |name| std::env::var_os(name).expect("需要明确交付真实排演材料");
    let read_json = |name| {
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(input(name)).unwrap()).unwrap()
    };
    let snapshot = read_json("CITIZENCHAIN_FULL_SNAPSHOT");
    let report = read_json("CITIZENCHAIN_REPLAY_REPORT");
    assert_eq!(report["passed"], true);
    assert_eq!(report["at"], snapshot["at"]);
    assert_eq!(report["spec_version"], 8);
    assert_eq!(report["transaction_version"], 7);
    let bytes = |value: &serde_json::Value| {
        let value = value.as_str().unwrap();
        hex::decode(value.strip_prefix("0x").unwrap_or(value)).unwrap()
    };
    let mut state: BTreeMap<Vec<u8>, Vec<u8>> = snapshot["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (bytes(&row[0]), bytes(&row[1])))
        .collect();
    assert_eq!(state.len(), snapshot["rows"].as_array().unwrap().len());
    let version = sp_storage::StateVersion::try_from(
        snapshot["version"]["stateVersion"].as_u64().unwrap() as u8,
    )
    .unwrap();
    assert!(snapshot["children"].as_array().unwrap().is_empty());
    let mut memory = sp_state_machine::new_in_mem::<sp_runtime::traits::BlakeTwo256>();
    memory.insert(
        [(
            None,
            state
                .iter()
                .map(|(key, value)| (key.clone(), Some(value.clone())))
                .collect::<Vec<_>>(),
        )],
        version,
    );
    let root = *memory.root();
    assert_eq!(root.as_bytes(), bytes(&snapshot["header"]["stateRoot"]));
    let code = std::fs::read(input("CITIZENCHAIN_REPLAY_WASM")).unwrap();
    assert_eq!(
        hex::encode(sp_crypto_hashing::blake2_256(&code)),
        report["wasm_hash"].as_str().unwrap()
    );
    let metadata = |code: &[u8]| {
        let fetcher = WrappedRuntimeCode(code.into());
        let code = RuntimeCode {
            code_fetcher: &fetcher,
            heap_pages: None,
            hash: sp_crypto_hashing::blake2_256(code).to_vec(),
        };
        let executor = sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::default();
        let (result, _) = executor.call(
            &mut sp_state_machine::BasicExternalities::new_empty(),
            &code,
            "Metadata_metadata",
            &[],
            sp_core::traits::CallContext::Onchain { import: false },
        );
        sp_core::OpaqueMetadata::decode_all(&mut result.unwrap().as_slice()).unwrap()
    };
    let old_metadata = metadata(&state[sp_storage::well_known_keys::CODE]);
    let new_metadata = metadata(&code);
    let old_layout = RuntimeStorage::from_metadata(&old_metadata).unwrap();
    let new_layout = RuntimeStorage::from_metadata(&new_metadata).unwrap();
    let spec =
        node::chain::ChainSpec::from_json_bytes(include_bytes!("../../citizenchain.json").to_vec())
            .unwrap();
    let reference = crate::guard::cid_lifecycle::GenesisReference::from_frozen(
        &GenesisState::new(spec.as_storage_builder().build_storage().unwrap(), version).unwrap(),
    )
    .unwrap();
    let lines =
        std::io::BufReader::new(std::fs::File::open(input("CITIZENCHAIN_REPLAY_ARCHIVE")).unwrap())
            .lines();
    let mut count = 0usize;
    let mut completed = false;
    let mut last = None;
    let first = u32::from_str_radix(
        snapshot["header"]["number"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap()
        + 1;
    for line in lines {
        let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        if row["complete"] == true {
            assert_eq!(row["wasm_hash"], report["wasm_hash"]);
            assert_eq!(
                row["last_hash"].as_str().unwrap(),
                format!("{:#x}", last.unwrap())
            );
            completed = true;
            continue;
        }
        assert!(!completed, "完成标记后不能追加状态");
        let header = <Block as sp_runtime::traits::Block>::Header::decode_all(
            &mut bytes(&row["header"]).as_slice(),
        )
        .unwrap();
        assert_eq!(*header.number(), first + count as u32);
        let body = row["body"]
            .as_array()
            .unwrap()
            .iter()
            .map(&bytes)
            .collect::<Vec<_>>();
        assert!(has_user_transaction(body.len()));
        assert_eq!(
            sp_runtime::traits::BlakeTwo256::ordered_trie_root(body, version),
            *header.extrinsics_root()
        );
        if let Some(parent) = last {
            assert_eq!(*header.parent_hash(), parent);
        } else {
            assert_eq!(
                format!("{:#x}", header.parent_hash()),
                snapshot["at"].as_str().unwrap()
            );
        }
        let mut raw: BTreeMap<Vec<u8>, Option<Vec<u8>>> = row["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    bytes(&pair[0]),
                    (!pair[1].is_null()).then(|| bytes(&pair[1])),
                )
            })
            .collect();
        if count == 0 {
            let initial: BTreeMap<_, _> = row["state"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (bytes(&pair[0]), bytes(&pair[1])))
                .collect();
            // 保留实际差量中的同值写入；只补入离线基点的候选及合成账户准备差异。
            for key in state.keys().chain(initial.keys()) {
                if !raw.contains_key(key) && state.get(key) != initial.get(key) {
                    raw.insert(key.clone(), initial.get(key).cloned());
                }
            }
        }
        let physical = |key: &[u8]| Ok(state.get(key).cloned());
        let physical_post = |key: &[u8]| {
            Ok(raw
                .get(key)
                .cloned()
                .unwrap_or_else(|| state.get(key).cloned()))
        };
        let before = (if count == 0 { &old_layout } else { &new_layout })
            .clone()
            .with_state(physical)
            .unwrap();
        let after = new_layout.clone().with_state(physical_post).unwrap();
        runtime::guard_migration_transition(
            |key| state.get(key).cloned(),
            |key| {
                raw.get(key)
                    .cloned()
                    .unwrap_or_else(|| state.get(key).cloned())
            },
        )
        .unwrap();
        let delta = before
            .transition_changes(&after, &raw, physical)
            .unwrap_or_else(|e| {
                panic!(
                    "区块{} {:?}->{:?}存储视图失败：{e}",
                    header.number(),
                    before.migration(),
                    after.migration()
                )
            });
        let parent = |key: &[u8]| before.read(key, physical).unwrap();
        let post = |key: &[u8]| after.read(key, physical_post).unwrap();
        let parent_legacy = |key: &[u8]| {
            before
                .read_raw(key, physical)
                .unwrap()
                .is_some_and(|r| r.legacy)
        };
        let post_legacy = |key: &[u8]| {
            after
                .read_raw(key, physical_post)
                .unwrap()
                .is_some_and(|r| r.legacy)
        };
        crate::guard::genesis_pallet::check_transition(&delta, &parent, &post).unwrap();
        crate::guard::cid_lifecycle::check_transition(
            *header.number(),
            &delta,
            &parent,
            &post,
            &reference,
        )
        .unwrap();
        let phase_changed = before.migration() != after.migration();
        if phase_changed || crate::guard::cid_lifecycle::needs_full_check(&delta) {
            let keys = state
                .keys()
                .map(|key| before.logical_key(key).unwrap())
                .chain(delta.keys().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            crate::guard::cid_lifecycle::check_full_state(&keys, &post, &reference).unwrap();
        }
        let vote_keys = delta
            .contains_key(sp_storage::well_known_keys::CODE)
            .then(|| {
                state
                    .keys()
                    .map(|key| before.logical_key(key).unwrap())
                    .chain(delta.keys().cloned())
                    .filter(|key| {
                        key.starts_with(&national_body_composition::storage_key::proposal_prefix())
                            || key.starts_with(
                                &national_body_composition::storage_key::threshold_prefix(),
                            )
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            });
        national_body_composition::check_transition_with_layout(
            &delta,
            &parent,
            &post,
            parent_legacy,
            post_legacy,
            before.migration() == runtime::MigrationState::Running
                || after.migration() == runtime::MigrationState::Running,
            vote_keys.as_deref(),
        )
        .unwrap();
        memory.insert(
            [(
                None,
                raw.iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>(),
            )],
            version,
        );
        assert_eq!(
            memory.root(),
            header.state_root(),
            "实际差量必须重算出逐块状态根"
        );
        for (key, value) in raw {
            if let Some(value) = value {
                state.insert(key, value);
            } else {
                state.remove(&key);
            }
        }
        assert_eq!(state.get(sp_storage::well_known_keys::CODE), Some(&code));
        if phase_changed && after.migration() == runtime::MigrationState::Complete {
            assert!(
                !state.keys().any(|key| runtime::guard_retired_key(key)),
                "完成态不能残留旧来源"
            );
        }
        last = Some(header.hash());
        count += 1;
        if phase_changed || count % 1000 == 0 {
            eprintln!(
                "节点存储与永久事实排演：区块{}，{:?}",
                header.number(),
                after.migration()
            );
        }
    }
    assert!(completed);
    assert_eq!(count, report["blocks"].as_array().unwrap().len());
    assert_eq!(
        runtime::guard_migration_state(|key| state.get(key).cloned()).unwrap(),
        runtime::MigrationState::Complete
    );
    let read = |key: &[u8]| Ok(state.get(key).cloned());
    let layout = new_layout.with_state(read).unwrap();
    let immutable = genesis_pallet::storage_key::citizens_declaration();
    let physical = layout.key(&immutable, true);
    let raw = BTreeMap::from([(physical.clone(), Some(state[&physical].clone()))]);
    let delta = layout.transition_changes(&layout, &raw, read).unwrap();
    assert!(
        genesis_pallet::check_transition(&delta, &|key| layout.read(key, read).unwrap(), &|key| {
            layout.read(key, read).unwrap()
        })
        .is_err(),
        "完成后同值写入仍拒绝"
    );
    let retired = nodedesktop::storage::prefix(
        runtime::guard_entity_names(true).0,
        b":__STORAGE_VERSION__:",
    );
    assert!(layout
        .transition_changes(
            &layout,
            &BTreeMap::from([(retired, Some(0u16.encode()))]),
            read
        )
        .is_err());
    let version = nodedesktop::storage::prefix(
        runtime::guard_entity_names(true).1,
        b":__STORAGE_VERSION__:",
    );
    assert!(layout
        .clone()
        .with_state(|key| if key == version {
            Ok(Some(2u16.encode()))
        } else {
            read(key)
        })
        .is_err());
}

#[derive(Default)]
pub(super) struct CountingImport {
    pub(super) imports: AtomicUsize,
}

#[async_trait::async_trait]
impl BlockImport<Block> for CountingImport {
    type Error = ConsensusError;

    async fn check_block(
        &self,
        _block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        Ok(ImportResult::AlreadyInChain)
    }

    async fn import_block(
        &self,
        _block: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        self.imports.fetch_add(1, Ordering::SeqCst);
        Ok(ImportResult::AlreadyInChain)
    }
}

fn test_chain_spec() -> node::chain::ChainSpec {
    let wasm = runtime::WASM_BINARY.expect("test requires embedded runtime WASM");
    let mut genesis_patch = genesis::config::genesis_config::<runtime::RuntimeGenesisConfig>();
    let alice = Sr25519Keyring::Alice
        .to_account_id()
        .to_ss58check_with_version(Ss58AddressFormat::custom(
            primitives::core_const::SS58_FORMAT,
        ));
    genesis_patch["balances"]["balances"]
        .as_array_mut()
        .expect("genesis balances array")
        .push(serde_json::json!([alice, 1_000_000_000_000u128]));

    let mut properties = Properties::new();
    properties.insert(
        "ss58Format".into(),
        serde_json::json!(primitives::core_const::SS58_FORMAT),
    );
    properties.insert("tokenDecimals".into(), serde_json::json!(2));
    properties.insert("tokenSymbol".into(), serde_json::json!("GMB"));

    node::chain::ChainSpec::builder(wasm, None)
        .with_name("CitizenChain NodeGuard Test")
        .with_id("citizenchain-node-guard-test")
        .with_chain_type(ChainType::Development)
        .with_protocol_id("citizenchain-node-guard-test")
        .with_properties(properties)
        .with_genesis_config_patch(genesis_patch)
        .build()
}

fn test_config(tokio_handle: tokio::runtime::Handle) -> Configuration {
    // SDK 的 new_temp_dir 在同一进程复用目录；服务测试必须各自持有独立数据库。
    // 子目录归调用方的本轮临时现场，工具退出且结果消费后由产品生命周期统一清场。
    static NEXT_BASE: AtomicUsize = AtomicUsize::new(0);
    // 标准库默认临时目录保持本机路径表示，避免Windows扩展前缀传入RocksDB。
    let root = std::env::temp_dir().join(format!(
        "node-guard-bad-block-{}-{}",
        std::process::id(),
        NEXT_BASE.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir(&root).expect("create fresh independent node guard test base");
    let base_path = BasePath::new(root.clone());
    let network = NetworkConfiguration::new(
        "node-guard-bad-block-test",
        "citizenchain-node-guard-test/0.1",
        Default::default(),
        None,
    );

    Configuration {
        impl_name: "citizenchain-node-guard-test".into(),
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
        chain_spec: Box::new(test_chain_spec()),
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
        disable_grandpa: false,
        dev_key_seed: None,
        tracing_targets: None,
        tracing_receiver: Default::default(),
        announce_block: true,
        data_path: root,
        base_path,
    }
}

fn remark_extrinsic(
    genesis_hash: <Block as BlockT>::Hash,
    spec: u32,
    tx: u32,
) -> <Block as BlockT>::Extrinsic {
    let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
        &format!("{genesis_hash:?}"),
        0,
        spec,
        tx,
        b"node-guard-bad-block",
    )
    .expect("build signed remark extrinsic");
    let raw = hex::decode(hex.trim_start_matches("0x")).expect("decode remark extrinsic hex");
    let mut body = raw.as_slice();
    let length = <codec::Compact<u32> as codec::Decode>::decode(&mut body).unwrap();
    assert_eq!(length.0 as usize, body.len());
    assert_eq!(&body[..2], &[0x45, chain_signing::mldsa::EXTENSION_VERSION]);
    sp_runtime::OpaqueExtrinsic::try_from_encoded_extrinsic(&raw)
        .expect("decode opaque remark extrinsic")
}

fn timestamp_extrinsic(now: u64) -> <Block as BlockT>::Extrinsic {
    let xt: runtime::UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::new_bare(
        runtime::RuntimeCall::Timestamp(runtime::TimestampCall::set { now }),
    )
    .into();
    xt.into()
}

fn legal_remark_block_params(client: &Arc<FullClient>) -> BlockImportParams<Block> {
    let parent_hash = client.info().genesis_hash;
    let pow_author =
        sr25519::Pair::from_string("//Alice//pow", None).expect("derive test pow author");
    let mut digest = sp_runtime::Digest::default();
    digest.push(sp_runtime::DigestItem::PreRuntime(
        POW_ENGINE_ID,
        pow_author.public().encode(),
    ));
    let mut builder = sc_block_builder::BlockBuilderBuilder::new(&**client)
        .on_parent_block(parent_hash)
        .fetch_parent_block_number(&**client)
        .expect("fetch genesis number")
        .with_inherent_digests(digest)
        .build()
        .expect("create block builder");

    builder
        .push(timestamp_extrinsic(1_782_950_406_000))
        .expect("push timestamp inherent");
    builder
        .push(remark_extrinsic(
            parent_hash,
            client
                .runtime_api()
                .version(parent_hash)
                .unwrap()
                .spec_version,
            client
                .runtime_api()
                .version(parent_hash)
                .unwrap()
                .transaction_version,
        ))
        .expect("push signed remark");

    let built = builder.build().expect("build legal remark block");
    let (block, storage_changes) = built.into_inner();
    let (header, body) = block.deconstruct();
    let mut params = BlockImportParams::new(BlockOrigin::Own, header);
    params.body = Some(body);
    params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(storage_changes));
    params
}

pub(super) fn import_params(number: u32) -> BlockImportParams<Block> {
    use sp_consensus::BlockOrigin;
    use sp_core::H256;
    use sp_runtime::Digest;

    let header = runtime::opaque::Header::new(
        number,
        H256::repeat_byte(1),
        H256::repeat_byte(2),
        H256::repeat_byte(3),
        Digest::default(),
    );
    BlockImportParams::new(BlockOrigin::NetworkInitialSync, header)
}

#[test]
fn guarded_import_delegates_only_after_explicit_success() {
    let inner = crate::guard::finalize_issuance_tests::CountingImport::default();
    let accepted =
        futures::executor::block_on(import_if_verified(&inner, import_params(1), Ok(())))
            .expect("verified import result");
    assert_eq!(accepted, ImportResult::AlreadyInChain);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

    let rejected = futures::executor::block_on(import_if_verified(
        &inner,
        import_params(2),
        Err("malicious state".into()),
    ))
    .expect("known bad result");
    assert_eq!(rejected, ImportResult::KnownBad);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

    let rejected_again = futures::executor::block_on(import_if_verified(
        &inner,
        import_params(3),
        Err("another malicious state".into()),
    ))
    .expect("second known bad result");
    assert_eq!(rejected_again, ImportResult::KnownBad);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

    // 连续拒绝不保存污染状态；随后合法区块仍只委派一次。
    let accepted_after_rejection =
        futures::executor::block_on(import_if_verified(&inner, import_params(4), Ok(())))
            .expect("verified import after rejection");
    assert_eq!(accepted_after_rejection, ImportResult::AlreadyInChain);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 2);
}

#[test]
fn finalize_total_issuance_is_exact_and_overflow_is_rejected() {
    let key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let pre = BTreeMap::from([(key.clone(), 100u128.encode())]);
    let post = BTreeMap::from([(key.clone(), 130u128.encode())]);
    let mut plan = FinalizeIssuancePlan::default();
    plan.add(30).unwrap();
    verify_finalize_issuance(&|k| pre.get(k).cloned(), &|k| post.get(k).cloned(), &plan).unwrap();
    plan.add(1).unwrap();
    assert!(
        verify_finalize_issuance(&|k| pre.get(k).cloned(), &|k| post.get(k).cloned(), &plan)
            .is_err()
    );
    let mut plan = FinalizeIssuancePlan::default();
    plan.add(u128::MAX).unwrap();
    assert!(plan.add(1).is_err());
}

#[test]
#[ignore = "requires explicitly supplied WASM_FILE"]
fn candidate_runtime_interface_rules_are_enforced() {
    let wasm = runtime::WASM_BINARY.expect("WASM_FILE is required; no skipped assertion");
    // 当前父metadata包含FLAT_FEE；旧WASM不能再冒充本次候选通过检查。
    let parent_metadata = runtime::Runtime::metadata_at_version(16).expect("V16 metadata");
    let backend: sp_state_machine::InMemoryBackend<sp_runtime::traits::BlakeTwo256> = (
        sp_runtime::Storage::default(),
        sp_core::storage::StateVersion::V1,
    )
        .into();
    crate::guard::runtime_policy::check_candidate_runtime(
        &backend,
        sp_core::H256::repeat_byte(1),
        &BTreeMap::new(),
        wasm,
        Some(parent_metadata.as_slice()),
    )
    .unwrap();
    assert!(crate::guard::runtime_policy::check_candidate_runtime(
        &backend,
        sp_core::H256::repeat_byte(1),
        &BTreeMap::new(),
        b"invalid WASM",
        None,
    )
    .is_err());
}

#[test]
#[ignore = "requires explicitly supplied WASM_FILE"]
fn normal_import_uses_only_its_verified_execution_result() {
    struct ObserveImport {
        changes: Arc<std::sync::Mutex<Vec<(Vec<u8>, Option<Vec<u8>>)>>>,
    }
    #[async_trait::async_trait]
    impl BlockImport<Block> for ObserveImport {
        type Error = ConsensusError;
        async fn check_block(
            &self,
            _: BlockCheckParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            Ok(ImportResult::AlreadyInChain)
        }
        async fn import_block(
            &self,
            p: BlockImportParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            let StateAction::ApplyChanges(StorageChanges::Changes(c)) = p.state_action else {
                panic!("verified execution changes required")
            };
            *self.changes.lock().unwrap() = c.main_storage_changes;
            Ok(ImportResult::Imported(Default::default()))
        }
    }
    assert!(runtime::WASM_BINARY.is_some(), "WASM_FILE required");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = test_config(rt.handle().clone());
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager: _task_manager,
        ..
    } = node::service::testing::new_partial_for_test(&config).unwrap();
    let mut p = legal_remark_block_params(&client);
    let StateAction::ApplyChanges(StorageChanges::Changes(c)) = &mut p.state_action else {
        panic!("builder changes")
    };
    let expected = c.main_storage_changes.clone();
    c.main_storage_changes
        .push((b"external-unverified-state".to_vec(), Some(vec![1])));
    let changes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let identity =
        crate::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .unwrap();
    let guard = NodeGuard::new_for_test(
        ObserveImport {
            changes: changes.clone(),
        },
        client,
        backend,
        identity,
        crate::guard::testing::TestGenesisIdentity::genesis_state(config.chain_spec.as_ref()),
    );
    let result = rt.block_on(guard.import_block(p)).unwrap();
    assert!(matches!(result, ImportResult::Imported(_)));
    assert_eq!(*changes.lock().unwrap(), expected);
}

#[test]
fn complete_state_import_checks_root_rules_and_runtime_before_delegating() {
    // 保留生产拒绝原因，避免完整状态回归只剩 KnownBad 断言而无法定位规则。
    struct GuardLogger;
    impl log::Log for GuardLogger {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target() == "node-guard" && metadata.level() == log::Level::Error
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                eprintln!("{}", record.args());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: GuardLogger = GuardLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Error);
    use sc_consensus::{ImportedState, StateAction, StorageChanges};
    use sp_blockchain::HeaderBackend;
    use sp_runtime::traits::Header as _;
    assert!(
        runtime::WASM_BINARY.is_some(),
        "完整状态回归必须内置本轮测试WASM"
    );
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = test_config(rt.handle().clone());
    let root = config.base_path.path().to_path_buf();
    let partial = {
        let _enter = rt.enter();
        node::service::testing::new_partial_for_test(&config).unwrap()
    };
    let identity =
        crate::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .unwrap();
    let genesis =
        crate::guard::testing::TestGenesisIdentity::genesis_state(config.chain_spec.as_ref());
    let guard = NodeGuard::new_for_test(
        CountingImport::default(),
        partial.client.clone(),
        partial.backend.clone(),
        identity,
        genesis,
    );
    let original = config
        .chain_spec
        .as_storage_builder()
        .build_storage()
        .unwrap();
    for case in ["fee", "runtime", "root", "valid"] {
        let mut storage = original.clone();
        match case {
            "fee" => {
                storage.top.insert(
                    crate::guard::runtime_policy::storage_key::max_rate(),
                    11u32.encode(),
                );
            }
            "runtime" => {
                storage
                    .top
                    .insert(sp_storage::well_known_keys::CODE.to_vec(), vec![0]);
            }
            "root" => {
                storage
                    .top
                    .insert(b"invalid-root-fixture".to_vec(), vec![1]);
            }
            _ => {}
        }
        let declared = if case == "root" {
            original.clone()
        } else {
            storage.clone()
        };
        let memory =
            crate::guard::GenesisState::from_storage(declared, runtime::VERSION.state_version());
        let mut params = import_params(1);
        params
            .header
            .set_parent_hash(partial.client.info().genesis_hash);
        params.header.set_state_root(memory.state_root);
        params.state_action = StateAction::ApplyChanges(StorageChanges::Import(ImportedState {
            block: params.post_hash(),
            state: sc_client_api::KeyValueStates::from([(
                Vec::new(),
                (storage.top.into_iter().collect::<Vec<_>>(), Vec::new()),
            )]),
        }));
        let result = rt.block_on(guard.import_block(params)).unwrap();
        if case == "valid" {
            assert_eq!(result, ImportResult::AlreadyInChain);
            assert_eq!(guard.inner.imports.load(Ordering::SeqCst), 1);
        } else {
            assert_eq!(result, ImportResult::KnownBad, "{case}");
            assert_eq!(guard.inner.imports.load(Ordering::SeqCst), 0, "{case}");
        }
        assert_eq!(partial.client.info().best_number, 0);
    }
    drop(guard);
    node::service::testing::stop_partial(partial, &rt);
    drop(config);
    std::fs::remove_dir_all(root).unwrap();
}
