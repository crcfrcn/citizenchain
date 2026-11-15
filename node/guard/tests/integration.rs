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

#[derive(Default)]
struct CountingImport {
    imports: AtomicUsize,
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

fn import_params(number: u32) -> BlockImportParams<Block> {
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
    )
    .unwrap();
    assert!(crate::guard::runtime_policy::check_candidate_runtime(
        &backend,
        sp_core::H256::repeat_byte(1),
        &BTreeMap::new(),
        b"invalid WASM"
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
    );
    let result = rt.block_on(guard.import_block(p)).unwrap();
    assert!(matches!(result, ImportResult::Imported(_)));
    assert_eq!(*changes.lock().unwrap(), expected);
}
