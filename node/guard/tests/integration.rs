use super::*;

use crate::guard::has_user_transaction;
use crate::guard::import_if_verified;
use crate::guard::partition_imported_state;
use crate::guard::verify_finalize_issuance;
use crate::guard::verify_imported_policy_state;
use crate::guard::verify_imported_state_params;
use codec::Encode;
use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;
use runtime::opaque::Block;
use sc_chain_spec::ChainType;
use sc_chain_spec::Properties;
use sc_client_api::backend::Backend as _;
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
use sp_state_machine::Backend as _;
use sp_storage::ChildInfo;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn full_runtime_genesis_storage() -> sp_runtime::Storage {
    use sp_runtime::BuildStorage;

    let config: runtime::RuntimeGenesisConfig =
        serde_json::from_value(genesis::config::genesis_config::<
            runtime::RuntimeGenesisConfig,
        >())
        .expect("decode complete runtime genesis config");
    config
        .build_storage()
        .expect("build complete runtime genesis storage")
}

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

#[derive(Clone, Default)]
struct SharedCountingImport {
    imports: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl BlockImport<Block> for SharedCountingImport {
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

fn skip_without_wasm_binary(test_name: &str) -> bool {
    if runtime::WASM_BINARY.is_some() {
        return false;
    }
    eprintln!("{test_name}: 跳过真实服务级坏块验收；当前测试构建未内置 WASM_BINARY");
    true
}

fn remark_extrinsic(genesis_hash: <Block as BlockT>::Hash) -> <Block as BlockT>::Extrinsic {
    let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
        &format!("{genesis_hash:?}"),
        0,
        runtime::VERSION.spec_version,
        runtime::VERSION.transaction_version,
        b"node-guard-bad-block",
    )
    .expect("build signed remark extrinsic");
    let raw = hex::decode(hex.trim_start_matches("0x")).expect("decode remark extrinsic hex");
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
        .push(remark_extrinsic(parent_hash))
        .expect("push signed remark");

    let built = builder.build().expect("build legal remark block");
    let (block, storage_changes) = built.into_inner();
    let (header, body) = block.deconstruct();
    let mut params = BlockImportParams::new(BlockOrigin::Own, header);
    params.body = Some(body);
    params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(storage_changes));
    params
}

fn mutate_precomputed_changes_to_guarded_state(
    params: &mut BlockImportParams<Block>,
    client: &Arc<FullClient>,
    backend: &Arc<FullBackend>,
    update_header_root: bool,
) {
    let parent_hash = *params.header.parent_hash();
    let StateAction::ApplyChanges(StorageChanges::Changes(changes)) = &mut params.state_action
    else {
        panic!("legal remark block must carry precomputed storage changes");
    };
    assert!(
        changes.child_storage_changes.is_empty(),
        "本坏块样本只篡改主存储，若出现 child delta 必须显式扩展重算逻辑"
    );

    let guarded_key = crate::guard::genesis_pallet::storage_key::citizen_max();
    let guarded_value = 1_443_497_379u64.encode();
    if let Some((_, value)) = changes
        .main_storage_changes
        .iter_mut()
        .find(|(key, _)| key == &guarded_key)
    {
        *value = Some(guarded_value);
    } else {
        changes
            .main_storage_changes
            .push((guarded_key, Some(guarded_value)));
    }

    let parent_state = backend
        .state_at(parent_hash, TrieCacheContext::Untrusted)
        .expect("open parent state");
    let state_version = client
        .runtime_api()
        .version(parent_hash)
        .expect("read runtime version")
        .state_version();
    let (bad_root, bad_transaction) = parent_state.full_storage_root(
        changes
            .main_storage_changes
            .iter()
            .map(|(key, value)| (&key[..], value.as_deref())),
        std::iter::empty::<(&ChildInfo, std::iter::Empty<(&[u8], Option<&[u8]>)>)>(),
        state_version,
    );
    changes.transaction_storage_root = bad_root;
    changes.transaction = bad_transaction;
    if update_header_root {
        params.header.set_state_root(bad_root);
    }
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

/// 构造真实完整状态导入形态，覆盖 `params.with_state()` 分支。
fn import_params_with_state(
    number: u32,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
) -> BlockImportParams<Block> {
    let mut params = import_params(number);
    let block = params.header.hash();
    let key_values = state.into_iter().collect::<Vec<_>>();
    let state =
        sc_client_api::KeyValueStates::from([(Vec::new(), (key_values, Vec::<Vec<u8>>::new()))]);
    params.state_action =
        StateAction::ApplyChanges(StorageChanges::Import(sc_consensus::ImportedState {
            block,
            state,
        }));
    params
}

fn cid_genesis_reference(
    top: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> crate::guard::cid_lifecycle::GenesisReference {
    let cid_keys: Vec<Vec<u8>> = top
        .keys()
        .filter(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
        .cloned()
        .collect();
    crate::guard::cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
        top.get(key).cloned()
    })
    .expect("build CID genesis reference")
}

const NEW_BALANCES_FLAGS: u128 = 0x80000000_00000000_00000000_00000000;

fn account(free: u128, providers: u32, flags: u128) -> Vec<u8> {
    (0u32, 0u32, providers, 0u32, (free, 0u128, 0u128, flags)).encode()
}

#[test]
fn combined_rewards_for_same_account_are_checked_once() {
    let recipient_account_id = [9u8; 32];
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let account_key =
        crate::guard::fullnode_issuance::storage_key::system_account(&recipient_account_id);
    let mut pre = BTreeMap::new();
    let mut post = BTreeMap::new();
    pre.insert(total_key.clone(), 1_000_000u128.encode());
    post.insert(total_key.clone(), 2_999_800u128.encode());
    pre.insert(account_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
    post.insert(
        account_key.clone(),
        account(1_999_900, 1, NEW_BALANCES_FLAGS),
    );
    let pre_delta = BTreeMap::new();
    let post_delta = BTreeMap::from([
        (total_key, Some(2_999_800u128.encode())),
        (account_key, Some(account(1_999_900, 1, NEW_BALANCES_FLAGS))),
    ]);
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    plan.add(recipient_account_id, 999_900)
        .expect("first reward");
    plan.add(recipient_account_id, 999_900)
        .expect("second reward");
    assert_eq!(
        verify_finalize_issuance(
            &pre_delta,
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        ),
        Ok(())
    );
}

#[test]
fn unexpected_finalize_account_change_is_rejected() {
    let recipient_account_id = [1u8; 32];
    let attacker = [2u8; 32];
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let recipient_key =
        crate::guard::fullnode_issuance::storage_key::system_account(&recipient_account_id);
    let attacker_key = crate::guard::fullnode_issuance::storage_key::system_account(&attacker);
    let mut pre = BTreeMap::new();
    let mut post = BTreeMap::new();
    pre.insert(total_key.clone(), 1_000u128.encode());
    post.insert(total_key.clone(), 1_100u128.encode());
    pre.insert(recipient_key.clone(), account(0, 1, NEW_BALANCES_FLAGS));
    post.insert(recipient_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
    pre.insert(attacker_key.clone(), account(50, 1, NEW_BALANCES_FLAGS));
    post.insert(attacker_key.clone(), account(51, 1, NEW_BALANCES_FLAGS));
    let post_delta = BTreeMap::from([
        (total_key, Some(1_100u128.encode())),
        (recipient_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
        (attacker_key, Some(account(51, 1, NEW_BALANCES_FLAGS))),
    ]);
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    plan.add(recipient_account_id, 100).expect("reward");
    let error = verify_finalize_issuance(
        &BTreeMap::new(),
        &post_delta,
        &|key| pre.get(key).cloned(),
        &|key| post.get(key).cloned(),
        &plan,
    )
    .expect_err("attacker change must fail");
    assert!(error.contains("未登记账户变化"));
}

#[test]
fn initialize_business_transfer_is_not_misclassified_as_finalize_issuance() {
    let payer = [3u8; 32];
    let payee = [4u8; 32];
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let payer_key = crate::guard::fullnode_issuance::storage_key::system_account(&payer);
    let payee_key = crate::guard::fullnode_issuance::storage_key::system_account(&payee);
    // on_initialize 的零和业务转账在 finalize 前、后读取结果完全相同；NodeGuard
    // 只应核对 finalize 自身变化，不能把合法订阅续费误判成原生发行。
    let state = BTreeMap::from([
        (total_key.clone(), 1_000u128.encode()),
        (payer_key.clone(), account(900, 1, NEW_BALANCES_FLAGS)),
        (payee_key.clone(), account(100, 1, NEW_BALANCES_FLAGS)),
    ]);
    let delta = BTreeMap::from([
        (payer_key, Some(account(900, 1, NEW_BALANCES_FLAGS))),
        (payee_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
    ]);
    assert_eq!(
        verify_finalize_issuance(
            &delta,
            &delta,
            &|key| state.get(key).cloned(),
            &|key| state.get(key).cloned(),
            &crate::guard::FinalizeIssuancePlan::default(),
        ),
        Ok(())
    );
}

#[test]
fn new_reward_account_must_match_balances_default_shape() {
    let recipient_account_id = [3u8; 32];
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let account_key =
        crate::guard::fullnode_issuance::storage_key::system_account(&recipient_account_id);
    let pre = BTreeMap::from([(total_key.clone(), 1_000u128.encode())]);
    let mut post = BTreeMap::from([(total_key.clone(), 1_100u128.encode())]);
    post.insert(account_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
    let post_delta = BTreeMap::from([
        (total_key, Some(1_100u128.encode())),
        (account_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
    ]);
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    plan.add(recipient_account_id, 100).expect("reward");
    assert_eq!(
        verify_finalize_issuance(
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        ),
        Ok(())
    );
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
fn imported_state_bad_cases_return_known_bad_before_inner_import() {
    let storage = full_runtime_genesis_storage();
    let top = storage.top;
    let reference = cid_genesis_reference(&top);

    for case in blockchain_harness::ImportedStateBadCaseKind::all() {
        let inner = crate::guard::finalize_issuance_tests::CountingImport::default();
        let params = import_params_with_state(0, imported_state_bad_case(*case, &top));
        assert!(
            params.with_state(),
            "{} must enter with_state path",
            case.label()
        );

        let verdict = verify_imported_state_params(&params, Some(&reference));
        let err = verdict
            .as_ref()
            .expect_err("bad imported state must fail before inner import");
        assert!(
            err.starts_with(case.expected_error_prefix()),
            "{} must fail with prefix {}, actual {err}",
            case.label(),
            case.expected_error_prefix()
        );

        let result =
            futures::executor::block_on(import_if_verified(&inner, params, verdict.map(|_| ())))
                .expect("known bad import result");
        assert_eq!(result, ImportResult::KnownBad);
        assert_eq!(
            inner.imports.load(Ordering::SeqCst),
            0,
            "{} must not delegate inner import",
            case.label()
        );
    }
}

#[test]
fn legal_block_zero_imported_state_delegates_after_guard_success() {
    let storage = full_runtime_genesis_storage();
    let top = storage.top;
    let reference = cid_genesis_reference(&top);
    let inner = crate::guard::finalize_issuance_tests::CountingImport::default();
    let params = import_params_with_state(0, top.clone());

    let stats = verify_imported_state_params(&params, Some(&reference))
        .expect("legal block zero complete state must pass");
    assert_eq!(stats.scanned, top.len());

    let result = futures::executor::block_on(import_if_verified(&inner, params, Ok(())))
        .expect("legal imported state should delegate");
    assert_eq!(result, ImportResult::AlreadyInChain);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);
}

#[test]
fn imported_running_reward_and_annual_state_pass_all_policies() {
    use primitives::pow_const::{BLOCKS_PER_YEAR, FULLNODE_BLOCK_REWARD};

    let genesis = full_runtime_genesis_storage().top;
    let reference = cid_genesis_reference(&genesis);
    for block in [1, BLOCKS_PER_YEAR as u32] {
        let mut state = genesis.clone();
        let miner = [7u8; 32];
        let issued = u128::from(block) * FULLNODE_BLOCK_REWARD;
        state.insert(
            crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
            block.encode(),
        );
        state.insert(
            crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
            issued.encode(),
        );
        state.insert(
            crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
            (block, miner, miner, FULLNODE_BLOCK_REWARD).encode(),
        );
        state.insert(
            crate::guard::fullnode_issuance::storage_key::last_authored(&miner),
            block.encode(),
        );
        state.insert(
            crate::guard::fullnode_issuance::storage_key::system_account(&miner),
            account(issued, 1, NEW_BALANCES_FLAGS),
        );
        let cid = b"GD001-CTZN1-TEST";
        state.insert(
            crate::guard::citizen_issuance::storage_key::rewarded_count(),
            1u64.encode(),
        );
        state.insert(
            crate::guard::citizen_issuance::storage_key::identity_claimed(cid),
            ().encode(),
        );
        state.insert(
            crate::guard::citizen_issuance::storage_key::account_rewarded(&miner),
            ().encode(),
        );
        if u64::from(block) == BLOCKS_PER_YEAR {
            let total: u128 = primitives::cid::china::china_ch::CHINA_CH
                .iter()
                .map(|bank| bank.stake_amount / 10_000 * 100)
                .sum();
            state.insert(
                crate::guard::provincialbank_interest::storage_key::last_settled_year(),
                1u32.encode(),
            );
            state.insert(
                crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
                total.encode(),
            );
            state.insert(
                crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
                (1u32, 43u32, total).encode(),
            );
        }
        let inner = crate::guard::finalize_issuance_tests::CountingImport::default();
        let params = import_params_with_state(block, state);
        let verdict = verify_imported_state_params(&params, Some(&reference));
        verdict.as_ref().expect("运行中完整状态须通过全部策略");
        let result =
            futures::executor::block_on(import_if_verified(&inner, params, verdict.map(|_| ())))
                .unwrap();
        assert_eq!(result, ImportResult::AlreadyInChain);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn imported_state_uses_main_storage_and_never_child_overrides() {
    let top = full_runtime_genesis_storage().top;
    let reference = cid_genesis_reference(&top);
    let key = crate::guard::genesis_pallet::storage_key::citizen_max();
    let original = top[&key].clone();
    // 两个字段必须同时为空才是主状态；另覆盖导出态和单字段非空情况。
    for (root, parents) in [
        (vec![1], vec![b":child_storage:default:test".to_vec()]),
        (vec![1], Vec::new()),
        (Vec::new(), vec![b":child_storage:default:test".to_vec()]),
    ] {
        for child_first in [false, true] {
            for case in 0..3 {
                let mut main = top.clone();
                let child_value = if case == 0 {
                    1u64.encode()
                } else {
                    original.clone()
                };
                if case == 1 {
                    main.insert(key.clone(), 1u64.encode());
                }
                if case == 2 {
                    main.remove(&key);
                }
                let main_len = main.len();
                let mut params = import_params_with_state(0, main);
                if let StateAction::ApplyChanges(StorageChanges::Import(imported)) =
                    &mut params.state_action
                {
                    imported.state.0.extend(
                        sc_client_api::KeyValueStates::from([(
                            root.clone(),
                            (vec![(key.clone(), child_value)], parents.clone()),
                        )])
                        .0,
                    );
                    if child_first {
                        imported.state.0.reverse();
                    }
                    assert_eq!(imported.state.len(), main_len + 1);
                }
                let inner = crate::guard::finalize_issuance_tests::CountingImport::default();
                let verdict = verify_imported_state_params(&params, Some(&reference));
                if case == 0 {
                    assert_eq!(verdict.as_ref().unwrap().scanned, main_len);
                } else {
                    assert!(verdict.as_ref().unwrap_err().starts_with("创世模块:"));
                }
                let result = futures::executor::block_on(import_if_verified(
                    &inner,
                    params,
                    verdict.map(|_| ()),
                ))
                .unwrap();
                assert_eq!(
                    result,
                    if case == 0 {
                        ImportResult::AlreadyInChain
                    } else {
                        ImportResult::KnownBad
                    }
                );
                assert_eq!(inner.imports.load(Ordering::SeqCst), usize::from(case == 0));
            }
        }
    }
}

#[test]
fn precomputed_changes_must_match_reexecuted_normal_block() {
    if skip_without_wasm_binary("precomputed_changes_must_match_reexecuted_normal_block") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let config = test_config(runtime.handle().clone());
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager: _task_manager,
        ..
    } = node::service::testing::new_partial_for_test(&config).expect("create partial node service");
    let identity =
        crate::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .expect("declared test genesis");
    let guard = crate::guard::NodeGuard::new_for_test(
        crate::guard::finalize_issuance_tests::CountingImport::default(),
        client.clone(),
        backend.clone(),
        identity,
    );

    let legal = legal_remark_block_params(&client);
    assert_eq!(
        guard.detect_violation(&legal),
        Ok(false),
        "真实 BlockBuilder 产物的预计算 changes 必须与节点重放结果一致"
    );

    let mut malicious = legal_remark_block_params(&client);
    mutate_precomputed_changes_to_guarded_state(&mut malicious, &client, &backend, false);
    let err = guard
        .detect_violation(&malicious)
        .expect_err("篡改预计算 changes 必须被节点守卫 fail-closed");
    assert!(
        err.contains("预计算"),
        "应由预计算 changes 一致性检查拒绝，实际错误: {err}"
    );
}

#[test]
fn imported_state_rejects_when_cid_startup_baseline_unavailable() {
    let storage = full_runtime_genesis_storage();
    let params = import_params_with_state(0, storage.top);
    let err =
        verify_imported_state_params(&params, None).expect_err("完整状态导入必须依赖 CID 启动基准");
    assert!(
        err.contains("CID 生命周期启动基准不可用"),
        "错误必须说明是启动基准不可用，实际错误: {err}"
    );
}

#[test]
fn current_wasm_passes_candidate_runtime_policy_behavior_probes() {
    if skip_without_wasm_binary("current_wasm_passes_candidate_runtime_policy_behavior_probes") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let config = test_config(runtime.handle().clone());
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager: _task_manager,
        ..
    } = node::service::testing::new_partial_for_test(&config).expect("create partial node service");
    let genesis_hash = client.info().genesis_hash;
    let parent_state = backend
        .state_at(genesis_hash, TrieCacheContext::Untrusted)
        .expect("read genesis state");
    crate::guard::runtime_policy::check_candidate_runtime(
        &parent_state,
        genesis_hash,
        genesis_hash,
        0,
        &BTreeMap::new(),
        runtime::WASM_BINARY.expect("checked above"),
        genesis_hash,
    )
    .expect("current WASM must satisfy node-side runtime policy behavior probes");
}

#[test]
fn self_consistent_bad_precomputed_block_is_known_bad_before_inner_import() {
    if skip_without_wasm_binary(
        "self_consistent_bad_precomputed_block_is_known_bad_before_inner_import",
    ) {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let config = test_config(runtime.handle().clone());
    // 回归同进程共用目录造成的 RocksDB 锁冲突；不能以串行化掩盖夹具隔离错误。
    let another_config = test_config(runtime.handle().clone());
    assert_ne!(config.base_path.path(), another_config.base_path.path());
    assert_ne!(config.database.path(), another_config.database.path());
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager: _task_manager,
        ..
    } = node::service::testing::new_partial_for_test(&config).expect("create partial node service");
    let inner = crate::guard::finalize_issuance_tests::SharedCountingImport::default();
    let imports = inner.imports.clone();
    let identity =
        crate::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .expect("declared test genesis");
    let guard =
        crate::guard::NodeGuard::new_for_test(inner, client.clone(), backend.clone(), identity);

    let mut malicious = legal_remark_block_params(&client);
    mutate_precomputed_changes_to_guarded_state(&mut malicious, &client, &backend, true);
    let result = runtime
        .block_on(guard.import_block(malicious))
        .expect("node guard import result");
    assert_eq!(result, ImportResult::KnownBad);
    assert_eq!(
        imports.load(Ordering::SeqCst),
        0,
        "自洽坏块必须在委派 inner import 前被拒绝"
    );
}

#[test]
fn finalize_plan_rejects_overflow_wrong_total_and_non_free_mutation() {
    let mut overflow = crate::guard::FinalizeIssuancePlan::default();
    overflow.add([1u8; 32], u128::MAX).expect("first amount");
    assert_eq!(overflow.add([2u8; 32], 1), Err(()));

    let recipient_account_id = [4u8; 32];
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let account_key =
        crate::guard::fullnode_issuance::storage_key::system_account(&recipient_account_id);
    let pre = BTreeMap::from([
        (total_key.clone(), 1_000u128.encode()),
        (account_key.clone(), account(10, 1, NEW_BALANCES_FLAGS)),
    ]);
    let mut post = BTreeMap::from([
        (total_key.clone(), 1_099u128.encode()),
        (account_key.clone(), account(110, 1, NEW_BALANCES_FLAGS)),
    ]);
    let mut post_delta = post
        .iter()
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    plan.add(recipient_account_id, 100).expect("reward");
    let wrong_total = verify_finalize_issuance(
        &BTreeMap::new(),
        &post_delta,
        &|key| pre.get(key).cloned(),
        &|key| post.get(key).cloned(),
        &plan,
    )
    .expect_err("wrong total must fail");
    assert!(wrong_total.contains("总发行差额错误"));

    post.insert(total_key.clone(), 1_100u128.encode());
    let mutated = (
        1u32,
        0u32,
        1u32,
        0u32,
        (110u128, 0u128, 0u128, NEW_BALANCES_FLAGS),
    )
        .encode();
    post.insert(account_key.clone(), mutated.clone());
    post_delta.insert(total_key, Some(1_100u128.encode()));
    post_delta.insert(account_key, Some(mutated));
    let non_free = verify_finalize_issuance(
        &BTreeMap::new(),
        &post_delta,
        &|key| pre.get(key).cloned(),
        &|key| post.get(key).cloned(),
        &plan,
    )
    .expect_err("nonce mutation must fail");
    assert!(non_free.contains("非 free 账户字段被改写"));
}

#[test]
fn imported_state_is_partitioned_in_one_shared_pass() {
    let protected = primitives::governance_skeleton::fixed_institutions()[0];
    let governance =
        crate::guard::governance_skeleton::storage_key::account_id(protected.cid_number.as_bytes());
    let ordinary_governance =
        crate::guard::governance_skeleton::storage_key::account_id(b"ORDINARY-CID");
    let composition = crate::guard::national_body_composition::storage_key::composition_keys(
        &primitives::institution_constraints::member_composition_specs()[0],
    )[0]
    .clone();
    let fullnode = crate::guard::fullnode_issuance::storage_key::rewarded_block_count();
    let citizen = crate::guard::citizen_issuance::storage_key::rewarded_count();
    let genesis = crate::guard::genesis_pallet::storage_key::citizen_max();
    let provincialbank = crate::guard::provincialbank_interest::storage_key::last_settled_year();
    let runtime_policy = crate::guard::runtime_policy::storage_key::max_rate();
    let cid = crate::guard::cid_lifecycle::storage_key::citizen_registry_prefix();
    let unrelated = b"unrelated".to_vec();
    let pairs = BTreeMap::from([
        (governance.clone(), vec![1]),
        (ordinary_governance, vec![1]),
        (composition.clone(), vec![1]),
        (fullnode.clone(), vec![2]),
        (citizen.clone(), vec![3]),
        (genesis.clone(), vec![4]),
        (provincialbank.clone(), vec![4]),
        (runtime_policy.clone(), 10u32.encode()),
        (cid.clone(), vec![4]),
        (unrelated, vec![5]),
    ]);
    let state = partition_imported_state(pairs.iter());
    assert_eq!(state.scanned, 10);
    assert_eq!(
        state.governance.keys().cloned().collect::<Vec<_>>(),
        [governance]
    );
    assert_eq!(
        state
            .national_body_composition
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [composition]
    );
    assert_eq!(
        state.fullnode_issuance.keys().cloned().collect::<Vec<_>>(),
        [fullnode]
    );
    assert_eq!(
        state.citizen_issuance.keys().cloned().collect::<Vec<_>>(),
        [citizen]
    );
    assert_eq!(
        state.genesis_pallet.keys().cloned().collect::<Vec<_>>(),
        [genesis]
    );
    assert_eq!(
        state
            .provincialbank_interest
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [provincialbank]
    );
    assert_eq!(
        state.runtime_policy.keys().cloned().collect::<Vec<_>>(),
        [runtime_policy]
    );
    assert_eq!(state.cid.keys().cloned().collect::<Vec<_>>(), [cid]);
}

#[test]
fn real_genesis_complete_state_passes_all_policies_in_one_scan() {
    let storage = full_runtime_genesis_storage();
    let top = storage.top;
    let cid_keys: Vec<Vec<u8>> = top
        .keys()
        .filter(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
        .cloned()
        .collect();
    let reference = crate::guard::cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
        top.get(key).cloned()
    })
    .expect("build CID genesis reference");
    let stats = verify_imported_policy_state(0, top.iter(), &reference)
        .expect("real block zero state must pass every policy");
    assert_eq!(stats.scanned, top.len());
    assert!(stats.governance > 0);
    assert!(stats.fullnode_issuance > 0);
    assert!(stats.genesis_pallet > 0);
    assert!(stats.provincialbank_interest > 0);
    assert!(stats.cid > 0);
}

#[test]
fn complete_state_rejects_missing_fixed_role_and_allows_dynamic_role_key() {
    let storage = full_runtime_genesis_storage();
    let top = storage.top;
    let cid_keys: Vec<Vec<u8>> = top
        .keys()
        .filter(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
        .cloned()
        .collect();
    let reference = crate::guard::cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
        top.get(key).cloned()
    })
    .expect("build CID genesis reference");
    let institution = primitives::governance_skeleton::fixed_institutions()[0];
    let role = primitives::governance_skeleton::fixed_role_specs(institution.code)[0];

    let mut missing = top.clone();
    missing.remove(
        &crate::guard::governance_skeleton::storage_key::institution_role(
            institution.cid_number.as_bytes(),
            role.role_code,
        ),
    );
    let err = verify_imported_policy_state(0, missing.iter(), &reference)
        .expect_err("missing fixed role must fail");
    assert!(err.starts_with("固定治理骨架:RoleMissing"));

    let mut extra = top;
    extra.insert(
        crate::guard::governance_skeleton::storage_key::institution_role(
            institution.cid_number.as_bytes(),
            b"EXTRA_ROLE",
        ),
        Vec::new(),
    );
    verify_imported_policy_state(0, extra.iter(), &reference)
        .expect("受保护机构的动态岗位由 runtime 治理，不得被误判为额外固定岗位");
}

#[test]
fn complete_state_rejects_each_policy_before_inner_import() {
    let storage = full_runtime_genesis_storage();
    let top = storage.top;
    let cid_keys: Vec<Vec<u8>> = top
        .keys()
        .filter(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
        .cloned()
        .collect();
    let reference = crate::guard::cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
        top.get(key).cloned()
    })
    .expect("build CID genesis reference");

    for case in blockchain_harness::ImportedStateBadCaseKind::all() {
        let bad_state = imported_state_bad_case(*case, &top);
        let err = verify_imported_policy_state(0, bad_state.iter(), &reference)
            .expect_err(&format!("{} must fail", case.label()));
        assert!(
            err.starts_with(case.expected_error_prefix()),
            "{} must fail with prefix {}, actual {err}",
            case.label(),
            case.expected_error_prefix()
        );
    }
}

/// 按 harness 定义的坏样本矩阵构造完整导入态。
///
/// 这里故意只放在 node 内部测试中：harness 负责枚举制度坏样本，node 测试负责
/// 使用私有 storage key 精确改写真实创世态，避免生产守卫为了测试而扩大公开接口。
fn imported_state_bad_case(
    case: blockchain_harness::ImportedStateBadCaseKind,
    top: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> BTreeMap<Vec<u8>, Vec<u8>> {
    use codec::Encode;

    let mut bad_state = top.clone();
    match case {
        blockchain_harness::ImportedStateBadCaseKind::MissingGovernanceAdmin => {
            let fixed = primitives::governance_skeleton::fixed_institutions()[0];
            bad_state.remove(&crate::guard::governance_skeleton::storage_key::account_id(
                fixed.cid_number.as_bytes(),
            ));
        }
        blockchain_harness::ImportedStateBadCaseKind::NonZeroFullnodeIssued => {
            bad_state.insert(
                crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
                1u32.encode(),
            );
        }
        blockchain_harness::ImportedStateBadCaseKind::UnknownCitizenIssuanceKey => {
            let mut unknown = crate::guard::citizen_issuance::storage_key::pallet_prefix().to_vec();
            unknown.extend_from_slice(b"UnknownGuardState");
            bad_state.insert(unknown, vec![1]);
        }
        blockchain_harness::ImportedStateBadCaseKind::ChangedGenesisCitizenMax => {
            bad_state.insert(
                crate::guard::genesis_pallet::storage_key::citizen_max(),
                1_443_497_379u64.encode(),
            );
        }
        blockchain_harness::ImportedStateBadCaseKind::MissingProvincialBankStake => {
            let first_bank = &primitives::cid::china::china_ch::CHINA_CH[0];
            bad_state.remove(
                &crate::guard::provincialbank_interest::storage_key::system_account(
                    &first_bank.stake_account,
                ),
            );
        }
        blockchain_harness::ImportedStateBadCaseKind::UnknownProvincialBankStorage => {
            let mut unknown_interest_key =
                crate::guard::provincialbank_interest::storage_key::pallet_prefix().to_vec();
            unknown_interest_key.extend_from_slice(&sp_crypto_hashing::twox_128(b"ShadowInterest"));
            bad_state.insert(unknown_interest_key, 1u32.encode());
        }
    }
    bad_state
}

#[test]
fn cid_policy_accepts_complete_state_import_at_non_genesis_height() {
    assert_eq!(
        crate::guard::cid_lifecycle::check_state_import_height(1),
        Ok(())
    );
}
