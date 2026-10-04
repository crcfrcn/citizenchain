//! P2P 坏块注入验收。
//!
//! 这里故意只放 `#[cfg(test)]` 服务级测试：恶意节点绕过 `NodeGuard` 直接把
//! “PoW seal 合法、header/root/changes 自洽、但永久规则非法”的块写入本地库，
//! 再通过真实同步网络交给诚实节点。诚实节点必须走生产 `import_queue`
//! 中的 `ConstitutionGuard<NodeGuard<PowBlockImport>>` 并拒绝该块。

use super::*;

use sc_chain_spec::{ChainType, Properties};
use sc_client_api::{StorageProvider, TrieCacheContext};
use sc_consensus::{BlockImport, BlockImportParams, ImportResult, StateAction, StorageChanges};
use sc_network::{
    config::{MultiaddrWithPeerId, NetworkConfiguration},
    service::traits::{NetworkBlock, NetworkPeers, NetworkService, NetworkStateInfo},
};
use sc_service::{
    config::{ExecutorConfiguration, KeystoreConfig, RpcBatchRequestConfig, RpcConfiguration},
    BasePath, BlocksPruning, DatabaseSource, PruningMode, Role, TransactionPoolOptions,
};
use sp_api::{Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_consensus_pow::POW_ENGINE_ID;
use sp_core::{
    crypto::{Ss58AddressFormat, Ss58Codec},
    sr25519,
};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{
    traits::{Block as BlockT, Header as HeaderT},
    Digest, DigestItem, OpaqueExtrinsic,
};
use sp_state_machine::Backend as _;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[derive(Clone, Default)]
struct CountingImport {
    imports: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl BlockImport<Block> for CountingImport {
    type Error = sp_consensus::Error;

    async fn check_block(
        &self,
        _block: sc_consensus::BlockCheckParams<Block>,
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

struct TestNode {
    config: Configuration,
    client: Arc<FullClient>,
    network: Arc<dyn NetworkService>,
    sync: Arc<sc_network_sync::SyncingService<Block>>,
    _task_manager: TaskManager,
}

fn skip_without_wasm_binary(test_name: &str) -> bool {
    if citizenchain::WASM_BINARY.is_some() {
        return false;
    }
    eprintln!("{test_name}: 跳过 P2P 坏块注入验收；当前测试构建未内置 WASM_BINARY");
    true
}

fn test_chain_spec() -> crate::core::chain_spec::ChainSpec {
    let wasm = citizenchain::WASM_BINARY.expect("test requires runtime wasm binary");
    let mut genesis_patch = citizenchain::genesis::genesis_config();
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

    crate::core::chain_spec::ChainSpec::builder(wasm, None)
        .with_name("CitizenChain P2P Bad Block Test")
        .with_id("citizenchain-p2p-bad-block-test")
        .with_chain_type(ChainType::Development)
        .with_protocol_id("citizenchain-p2p-bad-block-test")
        .with_properties(properties)
        .with_genesis_config_patch(genesis_patch)
        .build()
}

fn test_config(node_name: &str, tokio_handle: tokio::runtime::Handle) -> Configuration {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gmb-p2p-bad-block-{node_name}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create unique p2p bad-block temp base");
    let base_path = BasePath::new(root.clone());
    let mut network = NetworkConfiguration::new(
        node_name,
        "citizenchain-p2p-bad-block-test/0.1",
        Default::default(),
        None,
    );
    network.allow_non_globals_in_dht = true;
    network
        .listen_addresses
        .push("/ip4/127.0.0.1/tcp/0".parse().expect("test listen address"));

    Configuration {
        impl_name: "citizenchain-p2p-bad-block-test".into(),
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
        disable_grandpa: true,
        dev_key_seed: None,
        tracing_targets: None,
        tracing_receiver: Default::default(),
        announce_block: true,
        data_path: root,
        base_path,
    }
}

fn remark_extrinsic(genesis_hash: <Block as BlockT>::Hash) -> <Block as BlockT>::Extrinsic {
    let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
        &format!("{genesis_hash:?}"),
        0,
        citizenchain::VERSION.spec_version,
        citizenchain::VERSION.transaction_version,
        b"node-guard-p2p-bad-block",
    )
    .expect("build signed remark extrinsic");
    let raw = hex::decode(hex.trim_start_matches("0x")).expect("decode remark extrinsic hex");
    OpaqueExtrinsic::try_from_encoded_extrinsic(&raw).expect("decode opaque remark extrinsic")
}

fn timestamp_extrinsic(now: u64) -> <Block as BlockT>::Extrinsic {
    let xt = citizenchain::UncheckedExtrinsic::new_bare(citizenchain::RuntimeCall::Timestamp(
        citizenchain::TimestampCall::set { now },
    ));
    xt.into()
}

fn storage_value_key(pallet: &[u8], item: &[u8]) -> Vec<u8> {
    [twox_128(pallet).as_slice(), twox_128(item).as_slice()].concat()
}

fn legal_remark_block_params(client: &Arc<FullClient>) -> BlockImportParams<Block> {
    let parent_hash = client.info().genesis_hash;
    let pow_author =
        sr25519::Pair::from_string("//Alice//pow", None).expect("derive test pow author");
    let mut digest = Digest::default();
    digest.push(DigestItem::PreRuntime(
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
    let mut params = BlockImportParams::new(BlockOrigin::NetworkInitialSync, header);
    params.body = Some(body);
    params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(storage_changes));
    params
}

fn mutate_to_self_consistent_guarded_state(
    params: &mut BlockImportParams<Block>,
    client: &Arc<FullClient>,
    backend: &Arc<FullBackend>,
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

    // GenesisPallet::CitizenMax 是 NodeGuard 钉死的创世事实之一。
    // 这里写入一个自洽但非法的新值，证明 P2P 同步入口不是只靠 state root 拒绝。
    let guarded_key = storage_value_key(b"GenesisPallet", b"CitizenMax");
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
        std::iter::empty::<(
            &sp_storage::ChildInfo,
            std::iter::Empty<(&[u8], Option<&[u8]>)>,
        )>(),
        state_version,
    );
    changes.transaction_storage_root = bad_root;
    changes.transaction = bad_transaction;
    params.header.set_state_root(bad_root);
}

/// 在预计算 delta 中直接篡改 manifest；测试随后移除 body，确保 ConstitutionGuard 走
/// `ApplyChanges(Changes)` 的真实提交前分支，而不是靠执行失败间接拒块。
fn mutate_precomputed_manifest(params: &mut BlockImportParams<Block>, client: &Arc<FullClient>) {
    let parent_hash = *params.header.parent_hash();
    let StateAction::ApplyChanges(StorageChanges::Changes(changes)) = &mut params.state_action
    else {
        panic!("legal remark block must carry precomputed storage changes");
    };
    let key = crate::core::constitution::storage_key::manifest();
    let mut value = client
        .storage(parent_hash, &sp_storage::StorageKey(key.clone()))
        .expect("read parent manifest")
        .expect("parent manifest exists")
        .0;
    value[0] ^= 1;
    if let Some((_, existing)) = changes
        .main_storage_changes
        .iter_mut()
        .find(|(existing_key, _)| existing_key == &key)
    {
        *existing = Some(value);
    } else {
        changes.main_storage_changes.push((key, Some(value)));
    }
    params.body = None;
}

fn seal_with_valid_pow(params: &mut BlockImportParams<Block>, client: &Arc<FullClient>) {
    let parent_hash = *params.header.parent_hash();
    let pre_hash = params.header.hash();
    let difficulty = SimplePow::new(client.clone())
        .difficulty(parent_hash)
        .expect("read parent pow difficulty");
    let nonce = (0u64..)
        .find(|nonce| hash_meets_difficulty(&pow_hash(pre_hash.as_ref(), *nonce), difficulty))
        .expect("difficulty is finite in test genesis");
    let pow_author =
        sr25519::Pair::from_string("//Alice//pow", None).expect("derive test pow author");
    let signature = pow_author.sign(pre_hash.as_ref());
    params
        .post_digests
        .push(DigestItem::Seal(POW_ENGINE_ID, (nonce, signature).encode()));
}

fn import_bad_block_without_node_guard(
    block_import: sc_consensus_grandpa::GrandpaBlockImport<
        FullBackend,
        Block,
        FullClient,
        FullSelectChain,
    >,
    select_chain: FullSelectChain,
    client: &Arc<FullClient>,
    backend: &Arc<FullBackend>,
) -> <Block as BlockT>::Hash {
    let algorithm = SimplePow::new(client.clone());
    let raw_pow_import = PowBlockImport::new(
        block_import,
        client.clone(),
        algorithm,
        0,
        select_chain,
        |_, ()| async {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
            Ok((timestamp,))
        },
    );
    let mut params = legal_remark_block_params(client);
    mutate_to_self_consistent_guarded_state(&mut params, client, backend);
    seal_with_valid_pow(&mut params, client);
    let bad_hash = params.post_hash();
    params.post_hash = Some(bad_hash);
    params.insert_intermediate(
        sc_consensus_pow::INTERMEDIATE_KEY,
        sc_consensus_pow::PowIntermediate::<U256> { difficulty: None },
    );
    let result = futures::executor::block_on(raw_pow_import.import_block(params))
        .expect("raw pow import should accept self-consistent bad block");
    assert!(
        matches!(
            result,
            ImportResult::Imported(_) | ImportResult::AlreadyInChain
        ),
        "恶意节点底层导入必须先接受坏块，实际结果: {result:?}"
    );
    assert_eq!(
        client.info().best_number,
        1,
        "恶意节点必须把坏块作为 best，才能通过 P2P 暴露给诚实节点"
    );
    bad_hash
}

fn start_test_node(
    mut config: Configuration,
    import_bad_before_network: bool,
) -> (TestNode, Option<<Block as BlockT>::Hash>) {
    let tls_cert = crate::core::tls_cert::load_or_generate_tls_cert(config.base_path.path())
        .expect("load or generate test TLS certificate");
    config.network.tls_private_key_der = Some(tls_cert.private_key_der);
    config.network.tls_certificate_chain_der = Some(tls_cert.certificate_chain_der);

    let sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        import_queue,
        transaction_pool,
        select_chain,
        other: (block_import, grandpa_link, _telemetry),
        ..
    } = new_partial(&config).expect("create partial service");

    let bad_hash = import_bad_before_network.then(|| {
        import_bad_block_without_node_guard(block_import, select_chain, &client, &backend)
    });

    let mut net_config = sc_network::config::FullNetworkConfiguration::<
        Block,
        <Block as BlockT>::Hash,
        NetworkBackend,
    >::new(&config.network, config.prometheus_registry().cloned());
    let metrics = NetworkBackend::register_notification_metrics(config.prometheus_registry());
    let peer_store_handle = net_config.peer_store_handle();
    let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
        &client
            .block_hash(0)
            .ok()
            .flatten()
            .expect("genesis block exists"),
        &config.chain_spec,
    );
    let (grandpa_protocol_config, _grandpa_notification_service) =
        sc_consensus_grandpa::grandpa_peers_set_config::<_, NetworkBackend>(
            grandpa_protocol_name,
            metrics.clone(),
            peer_store_handle,
        );
    net_config.add_notification_protocol(grandpa_protocol_config);
    let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::new(),
    ));

    let (network, _system_rpc_tx, _tx_handler_controller, sync) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            net_config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            spawn_essential_handle: task_manager.spawn_essential_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_config: Some(WarpSyncConfig::WithProvider(warp_sync)),
            block_relay: None,
            metrics,
        })
        .expect("build test network");

    (
        TestNode {
            config,
            client,
            network,
            sync,
            _task_manager: task_manager,
        },
        bad_hash,
    )
}

async fn wait_until(label: &str, timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if predicate() {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "等待 {label} 超时");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_until_peer_reports_bad_best(honest: &TestNode, bad_hash: <Block as BlockT>::Hash) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let peers = honest
            .sync
            .peers_info()
            .await
            .expect("read honest peer info");
        if peers
            .iter()
            .any(|(_, info)| info.best_number >= 1 && info.best_hash == bad_hash)
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "诚实节点未观察到恶意 peer 的 block#1 坏块状态, peers={}",
            peers.len()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn first_listen_address(node: &TestNode) -> MultiaddrWithPeerId {
    let address = node
        .network
        .listen_addresses()
        .into_iter()
        .next()
        .expect("test node must publish a listen address");
    MultiaddrWithPeerId {
        multiaddr: address,
        peer_id: node.network.local_peer_id(),
    }
}

#[test]
fn constitution_guard_rejects_manifest_delta_and_then_accepts_legal_delta() {
    if skip_without_wasm_binary(
        "constitution_guard_rejects_manifest_delta_and_then_accepts_legal_delta",
    ) {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let config = test_config("constitution-guard-direct", runtime.handle().clone());
    let base_path = config.base_path.path().to_path_buf();
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        ..
    } = new_partial(&config).expect("create partial service");
    let inner = CountingImport::default();
    let imports = inner.imports.clone();
    let guard = crate::core::constitution::ConstitutionGuard::new(inner, client.clone(), backend)
        .expect("create constitution guard from valid block zero");

    let mut malicious = legal_remark_block_params(&client);
    mutate_precomputed_manifest(&mut malicious, &client);
    let rejected = runtime
        .block_on(guard.import_block(malicious))
        .expect("constitution guard rejection result");
    assert_eq!(rejected, ImportResult::KnownBad);
    assert_eq!(imports.load(Ordering::SeqCst), 0);
    assert_eq!(client.info().best_number, 0);

    // 同一个无状态守卫拒绝非法块后，下一份合法预计算 delta 仍能委派，节点无需重启。
    let mut legal = legal_remark_block_params(&client);
    legal.body = None;
    let accepted = runtime
        .block_on(guard.import_block(legal))
        .expect("legal import after manifest rejection");
    assert_eq!(accepted, ImportResult::AlreadyInChain);
    assert_eq!(imports.load(Ordering::SeqCst), 1);
    assert_eq!(client.info().best_number, 0);

    drop(guard);
    drop(client);
    drop(task_manager);
    std::fs::remove_dir_all(base_path).expect("remove constitution guard test temp base");
}

#[test]
fn p2p_sync_rejects_self_consistent_bad_node_guard_block() {
    if skip_without_wasm_binary("p2p_sync_rejects_self_consistent_bad_node_guard_block") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let malicious_config = test_config("p2p-bad-malicious", runtime.handle().clone());
        let honest_config = test_config("p2p-bad-honest", runtime.handle().clone());
        let (malicious, bad_hash) = start_test_node(malicious_config, true);
        let bad_hash = bad_hash.expect("malicious node imported bad block");
        let (honest, _) = start_test_node(honest_config, false);

        wait_until("恶意节点监听地址", Duration::from_secs(10), || {
            !malicious.network.listen_addresses().is_empty()
        })
        .await;
        let malicious_addr = first_listen_address(&malicious);
        honest
            .network
            .add_reserved_peer(malicious_addr)
            .expect("connect honest node to malicious peer");

        wait_until("P2P 已连接", Duration::from_secs(20), || {
            honest.sync.num_connected_peers() > 0 && malicious.sync.num_connected_peers() > 0
        })
        .await;

        malicious.sync.announce_block(bad_hash, None);

        wait_until_peer_reports_bad_best(&honest, bad_hash).await;
        tokio::time::sleep(Duration::from_secs(2)).await;

        assert_eq!(
            honest.client.info().best_number,
            0,
            "诚实节点不得把违反 NodeGuard 永久规则的 P2P 坏块设为 best"
        );
        assert!(
            honest
                .client
                .header(bad_hash)
                .expect("query honest client for bad hash")
                .is_none(),
            "诚实节点数据库不得保存 P2P 坏块 header"
        );
        assert_eq!(
            malicious.client.info().best_number,
            1,
            "恶意节点样本必须保持可服务坏块，避免测试退化为空同步"
        );
        assert!(
            malicious
                .config
                .base_path
                .path()
                .starts_with(std::env::temp_dir()),
            "测试节点必须只使用临时目录"
        );
        assert!(
            honest
                .config
                .base_path
                .path()
                .starts_with(std::env::temp_dir()),
            "测试节点必须只使用临时目录"
        );
        let malicious_base = malicious.config.base_path.path().to_path_buf();
        let honest_base = honest.config.base_path.path().to_path_buf();
        drop(honest);
        drop(malicious);
        std::fs::remove_dir_all(honest_base).expect("remove honest test temp base");
        std::fs::remove_dir_all(malicious_base).expect("remove malicious test temp base");
    });
}
