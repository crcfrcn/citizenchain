//! # 节点服务层 (service)
//!
//! 实现 CitizenChain 全节点的双共识架构：
//! - **PoW 共识**：SimplePow 算法，blake2_256(pre_hash ++ nonce)，难度从链上 Runtime API 读取。
//! - **GRANDPA 最终性**：权威节点运行 voter，普通节点运行 observer。
//!
//! 挖矿特性：
//! - CPU 多线程挖矿，各线程 nonce 不重叠（stride = 线程数）。
//! - GPU 挖矿（可选 `gpu-mining` feature），使用 nonce 高半区（bit63=1）。
//! - 空交易池时不挖矿（避免空块），major sync 时暂停出块；peer 连接状态不限制挖矿。
//! - PoW 有效解找到后立即提交；六分钟只是难度调整追踪的长期平均目标。

// Substrate service API 固定返回 sc_service::Error，Node 必须保持框架函数签名。
#![allow(clippy::result_large_err)]
use runtime;

use codec::Decode;
use codec::Encode;
use frame_metadata::RuntimeMetadata;
use frame_metadata::RuntimeMetadataPrefixed;
use frame_metadata::META_RESERVED;
use futures::FutureExt;
use nodeclient::codec::decode_runtime_metadata;
use nodeclient::network::LocalNetworkReader;
use nodeclient::types::Service;
use nodemining::cpu::cpu_hashrate;
use nodemining::cpu::start_cpu_miner;
use nodemining::pow::POW_AUTHOR_KEY_TYPE;
use runtime::apis::RuntimeApi;
use runtime::opaque::Block;
use sc_client_api::Backend;
use sc_client_api::BlockBackend;
use sc_client_api::HeaderBackend;
use sc_client_api::StorageProvider;
use sc_consensus_pow::MiningHandle;
use sc_consensus_pow::PowAlgorithm;
use sc_consensus_pow::PowBlockImport;
use sc_network::NetworkBackend as _;
use sc_service::error::Error as ServiceError;
use sc_service::Configuration;
use sc_service::TaskManager;
use sc_service::WarpSyncConfig;
use sc_telemetry::Telemetry;
use sc_telemetry::TelemetryWorker;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::Core as _;
use sp_api::Metadata as _;
use sp_api::ProvideRuntimeApi as _;
use sp_consensus::NoNetwork;
use sp_consensus::SyncOracle;
use sp_core::crypto::KeyTypeId;
use sp_core::sr25519;
use sp_core::Pair as _;
use sp_core::U256;
use sp_crypto_hashing::blake2_256;
use sp_crypto_hashing::twox_128;
use sp_keystore::Keystore;
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const POW_MINING_TIMEOUT_SECS: u64 = 2;

const POW_PROPOSAL_BUILD_SECS: u64 = 2;

const GRANDPA_JUSTIFICATION_PERIOD: u32 = 64;

/// Ethereum方法只在Runtime实际注册Revive时初始化；创世旧版只提供原生TLS RPC。
fn ethereum_rpc_available(metadata: &[u8], spec_version: u32) -> Result<bool, String> {
    let has_revive = match decode_runtime_metadata(metadata)? {
        RuntimeMetadata::V14(value) => value.pallets.iter().any(|pallet| pallet.name == "Revive"),
        RuntimeMetadata::V15(value) => value.pallets.iter().any(|pallet| pallet.name == "Revive"),
        RuntimeMetadata::V16(value) => value.pallets.iter().any(|pallet| pallet.name == "Revive"),
        _ => unreachable!("元数据版本已校验"),
    };
    if has_revive {
        return Ok(true);
    }
    if spec_version == 0 {
        return Ok(false);
    }
    Err("运行期Runtime缺少必需的Revive，拒绝启动Ethereum RPC".into())
}

/// 返回 (pre_digest 编码字节, 矿工公钥)。
/// pre_digest 中存储的是 sr25519 公钥而非 AccountId，配合 seal 中的签名实现密码学绑定。
fn author_pre_digest(keystore: &sp_keystore::KeystorePtr) -> Option<(Vec<u8>, sr25519::Public)> {
    let keys = keystore.sr25519_public_keys(POW_AUTHOR_KEY_TYPE);
    let author_public = keys.into_iter().next()?;
    Some((author_public.encode(), author_public))
}

fn ensure_powr_key(keystore: &sp_keystore::KeystorePtr) -> Result<(), ServiceError> {
    let keys = keystore.sr25519_public_keys(POW_AUTHOR_KEY_TYPE);
    if !keys.is_empty() {
        return Ok(());
    }
    // 传 None 让 Substrate 生成 BIP39 助记词并写入 keystore 磁盘文件，
    // 节点桌面端后续能读取同一把密钥来签名绑定交易。
    // 注意：传 Some(suri) 只存内存不写磁盘，重启后丢失。
    keystore
        .sr25519_generate_new(POW_AUTHOR_KEY_TYPE, None)
        .map_err(|e| ServiceError::Other(format!("failed to generate powr key: {e}")))?;
    Ok(())
}

type RpcTaskError = Box<dyn std::error::Error + Send + Sync>;

// 只增加阶段上下文，原错误仍作为 source 保留，不以清理错误或固定提示替代。
#[derive(Debug)]
struct RpcStageError {
    stage: &'static str,
    source: RpcTaskError,
}

impl std::fmt::Display for RpcStageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.source)
    }
}

impl std::error::Error for RpcStageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn rpc_stage_error(stage: &'static str, source: impl Into<RpcTaskError>) -> RpcTaskError {
    Box::new(RpcStageError {
        stage,
        source: source.into(),
    })
}

/// 返回当前允许矿工消费的 ready 交易数。
///
/// 只在 major sync 期间暂停挖矿；peer 连接状态不是 PoW 出块权限，调用方不得把
/// `is_offline()` 混入本门控。交易池为空时原样返回 0，继续禁止提交空块。
fn mining_ready_transactions(ready: usize, is_major_syncing: bool) -> usize {
    if is_major_syncing {
        0
    } else {
        ready
    }
}

pub(crate) fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
    #[cfg(any(test, feature = "test-support"))]
    return new_partial_inner(config, None);
    #[cfg(not(any(test, feature = "test-support")))]
    new_partial_inner(config)
}

fn new_partial_inner(
    config: &Configuration,
    #[cfg(any(test, feature = "test-support"))] test_genesis_identity: Option<
        nodeguard::guard::testing::TestGenesisIdentity,
    >,
) -> Result<Service, ServiceError> {
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(&config.executor);
    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<Block, RuntimeApi, _>(
            config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
            vec![Arc::new(sc_consensus_grandpa::GrandpaPruningFilter)],
        )?;
    let client = Arc::new(client);

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager
            .spawn_handle()
            .spawn("telemetry", None, worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = Arc::from(
        sc_transaction_pool::Builder::new(
            task_manager.spawn_essential_handle(),
            client.clone(),
            config.role.is_authority().into(),
        )
        .with_options(config.transaction_pool.clone())
        .with_prometheus(config.prometheus_registry())
        .build(),
    );

    let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
        client.clone(),
        GRANDPA_JUSTIFICATION_PERIOD,
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let algorithm = nodemining::pow::SimplePow::new(client.clone());
    let pow_block_import = PowBlockImport::new(
        grandpa_block_import.clone(),
        client.clone(),
        algorithm.clone(),
        0,
        select_chain.clone(),
        |_, ()| async {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
            Ok((timestamp,))
        },
    );

    // 节点守卫统一承载宪法以外的节点级死规则。创世身份必须通过，其他本机自检沿用警戒状态；
    // 后续区块、完整状态和候选 runtime 导入仍由守卫强拒绝。
    // 测试链只替换声明的创世信任输入，所有身份和导入规则仍完整核验。
    #[cfg(any(test, feature = "test-support"))]
    let node_guard = match test_genesis_identity {
        Some(identity) => nodeguard::guard::NodeGuard::new_for_test(
            pow_block_import,
            client.clone(),
            backend.clone(),
            identity,
        ),
        None => nodeguard::guard::NodeGuard::new(pow_block_import, client.clone(), backend.clone()),
    };
    #[cfg(not(any(test, feature = "test-support")))]
    let node_guard =
        nodeguard::guard::NodeGuard::new(pow_block_import, client.clone(), backend.clone());

    // 宪法守卫保持独立且位于最外层(ADR-027 §7)：先执行整条链最高优先级的
    // 不可修改条款校验，再进入节点守卫及 PoW 导入；runtime 升级无法绕过两层执法。
    let guarded_import = nodeguard::constitution::guard::ConstitutionGuard::new(
        node_guard,
        client.clone(),
        backend.clone(),
    )
    .map_err(ServiceError::Other)?;

    let import_queue = sc_consensus_pow::import_queue(
        Box::new(guarded_import),
        Some(Box::new(grandpa_block_import.clone())),
        algorithm,
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
    )?;

    Ok(sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (grandpa_block_import, grandpa_link, telemetry),
    })
}

/// 网络后端类型：固定使用 libp2p 承载 WSS，P2P 身份由 Noise 验证。
pub(crate) type NetworkBackend =
    sc_network::NetworkWorker<Block, <Block as sp_runtime::traits::Block>::Hash>;

/// Builds a new service for a full client.
pub(crate) fn new_full(
    config: Configuration,
    mining_threads: usize,
    gpu_device: Option<usize>,
    desktop_rpc_tls: Option<nodedesktop::tls::RpcTls>,
    desktop_rpc_failure: Option<Arc<Mutex<Option<String>>>>,
    bank_cid: Option<String>,
    bank_role: Option<String>,
    clearing_bank_password: Option<String>,
    reserve_interval_secs: Option<u64>,
) -> Result<TaskManager, ServiceError> {
    new_full_with_network_reader(
        config,
        mining_threads,
        gpu_device,
        desktop_rpc_tls,
        desktop_rpc_failure,
        bank_cid,
        bank_role,
        clearing_bank_password,
        reserve_interval_secs,
        None,
    )
}

pub(crate) fn new_full_with_network_reader(
    mut config: Configuration,
    mining_threads: usize,
    gpu_device: Option<usize>,
    desktop_rpc_tls: Option<nodedesktop::tls::RpcTls>,
    desktop_rpc_failure: Option<Arc<Mutex<Option<String>>>>,
    // 清算行机构 CID(None=本节点不做清算行角色)
    bank_cid: Option<String>,
    // 清算批次提交岗位码；必须与 CID 和签名钱包任职同时匹配
    bank_role: Option<String>,
    // 解锁 `nodeoffchain::keystore` 的密码
    clearing_bank_password: Option<String>,
    // nodeoffchain::reserve 对账周期(秒),None=默认 300,Some(0)=关闭
    reserve_interval_secs: Option<u64>,
    local_network: Option<LocalNetworkReader>,
) -> Result<TaskManager, ServiceError> {
    // 先装载可信 RPC 证书并取走端点，禁止上游 spawn_tasks 同时开启明文监听。
    // 进程内 RpcHandlers 仍正常创建，CLI 和桌面共用本服务入口。
    let rpc_endpoints = config.rpc.addr.take().unwrap_or_default();
    // SDK 的显式空端点表示只保留进程内 RPC；None 会重开默认明文回环端口。
    config.rpc.addr = Some(Vec::new());
    let rpc_tls = if rpc_endpoints.is_empty() {
        None
    } else {
        let tls = desktop_rpc_tls.map_or_else(nodedesktop::tls::RpcTls::load, Ok);
        Some(tls.map_err(|error| ServiceError::Other(format!("装载 RPC TLS 失败: {error}")))?)
    };
    // 生成或加载 TLS 自签证书，注入到网络配置中。
    let tls_cert = nodedesktop::certificate::load_or_generate_tls_cert(config.base_path.path())
        .map_err(ServiceError::Other)?;
    config.network.tls_private_key_der = Some(tls_cert.private_key_der);
    config.network.tls_certificate_chain_der = Some(tls_cert.certificate_chain_der);

    let sc_service::PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (block_import, grandpa_link, mut telemetry),
    } = new_partial(&config)?;

    let keystore = keystore_container.keystore();
    let role = config.role;
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let local_grandpa_keys = keystore.ed25519_public_keys(sp_consensus_grandpa::KEY_TYPE);
    let current_authorities = grandpa_link.shared_authority_set().current_authorities();
    let has_local_grandpa_authority = enable_grandpa
        && current_authorities.iter().any(|(id, _)| {
            local_grandpa_keys
                .iter()
                .any(|local| id.encode() == local.encode())
        });

    let mut net_config = sc_network::config::FullNetworkConfiguration::<
        Block,
        <Block as sp_runtime::traits::Block>::Hash,
        NetworkBackend,
    >::new(&config.network, config.prometheus_registry().cloned());
    let metrics =
        crate::service::NetworkBackend::register_notification_metrics(config.prometheus_registry());
    let peer_store_handle = net_config.peer_store_handle();
    let genesis_hash = client
        .block_hash(0)
        .map_err(|err| ServiceError::Other(format!("读取创世区块哈希失败: {err}")))?
        .ok_or_else(|| ServiceError::Other("创世区块不存在".to_string()))?;
    let grandpa_protocol_name =
        sc_consensus_grandpa::protocol_standard_name(&genesis_hash, &config.chain_spec);
    // 所有节点统一注册 GRANDPA 网络协议，保证协议栈一致，避免协议协商不对称导致连接断开。
    // 权威节点启动 grandpa-voter 消费 notification_service；普通节点启动 grandpa-observer 消费。
    let (grandpa_protocol_config, grandpa_notification_service) =
        sc_consensus_grandpa::grandpa_peers_set_config::<_, NetworkBackend>(
            grandpa_protocol_name.clone(),
            metrics.clone(),
            peer_store_handle,
        );
    net_config.add_notification_protocol(grandpa_protocol_config);
    let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::new(),
    ));

    let (network, system_rpc_tx, tx_handler_controller, sync_service) =
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
        })?;

    if config.offchain_worker.enabled {
        let offchain_workers =
            sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
                runtime_api_provider: client.clone(),
                is_validator: config.role.is_authority(),
                keystore: Some(keystore_container.keystore()),
                offchain_db: backend.offchain_storage(),
                transaction_pool: Some(OffchainTransactionPoolFactory::new(
                    transaction_pool.clone(),
                )),
                network_provider: Arc::new(network.clone()),
                enable_http_requests: true,
                custom_extensions: |_| vec![],
            })?;
        task_manager.spawn_handle().spawn(
            "offchain-workers-runner",
            "offchain-worker",
            offchain_workers
                .run(client.clone(), task_manager.spawn_handle())
                .boxed(),
        );
    }

    let prometheus_registry = config.prometheus_registry().cloned();

    // GPU 哈希率函数指针：仅在 gpu-mining feature 且用户启用 GPU 时传入。
    let gpu_hashrate_fn: Option<fn() -> f64> = {
        #[cfg(feature = "gpu-mining")]
        {
            if gpu_device.is_some() {
                Some(nodemining::gpu::gpu_hashrate as fn() -> f64)
            } else {
                None
            }
        }
        #[cfg(not(feature = "gpu-mining"))]
        {
            None
        }
    };

    // 清算行启动细节归入 `nodeoffchain::bootstrap`,service.rs 只做节点通用接线。
    let clearing_rpc_impl = nodeoffchain::bootstrap::start_from_cli(
        bank_cid.as_deref(),
        bank_role.as_deref(),
        clearing_bank_password.as_deref(),
        reserve_interval_secs,
        config.base_path.path(),
        client.clone(),
        transaction_pool.clone(),
        &task_manager,
    );

    let rpc_extensions_builder = {
        let client = client.clone();
        let pool = transaction_pool.clone();
        let keystore = keystore_container.keystore();
        let clearing_rpc_impl = clearing_rpc_impl.clone();

        Box::new(move |_| {
            let deps = crate::rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                keystore: keystore.clone(),
                cpu_hashrate_fn: cpu_hashrate as fn() -> f64,
                gpu_hashrate_fn,
                // 清算行 RPC 命名空间(None 时跳过注入)
                offchain_clearing_rpc: clearing_rpc_impl.clone(),
            };
            crate::rpc::create_full(deps).map_err(Into::into)
        })
    };

    let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        network: Arc::new(network.clone()),
        client: client.clone(),
        keystore: keystore.clone(),
        task_manager: &mut task_manager,
        transaction_pool: transaction_pool.clone(),
        rpc_builder: rpc_extensions_builder,
        backend: backend.clone(),
        system_rpc_tx,
        tx_handler_controller,
        sync_service: sync_service.clone(),
        config,
        telemetry: telemetry.as_mut(),
        tracing_execute_block: None,
    })?;

    if let Some(reader) = local_network {
        reader
            .attach(rpc_handlers.handle(), tokio::runtime::Handle::current())
            .map_err(ServiceError::Other)?;
    }

    if let Some(rpc_tls) = rpc_tls {
        // 原生后端仅在进程内使用；公开副本显式覆盖为 Safe，防止 Unsafe 扩展泄漏。
        let native = (*rpc_handlers.handle()).clone();
        let rpc_client = client.clone();
        task_manager
            .spawn_essential_handle()
            .spawn("ethereum-rpc-tls", Some("rpc"), async move {
                let result: Result<(), RpcTaskError> = async {
                    let finalized = rpc_client.info().finalized_hash;
                    let metadata = rpc_client
                        .runtime_api()
                        .metadata(finalized)
                        .map_err(|error| rpc_stage_error("读取Runtime元数据", error))?;
                    let version = rpc_client
                        .runtime_api()
                        .version(finalized)
                        .map_err(|error| rpc_stage_error("读取Runtime版本", error))?;
                    let with_ethereum = ethereum_rpc_available(
                        metadata.as_slice(),
                        version.spec_version,
                    )
                    .map_err(|error| {
                        rpc_stage_error("核验Runtime的Revive能力", std::io::Error::other(error))
                    })?;
                    let mut exposed = native;
                    if with_ethereum {
                        let (ethereum, synchronization) =
                            crate::ethereum::initialize(exposed.clone())
                                .await
                                .map_err(|error| rpc_stage_error("Ethereum客户端初始化", error))?;
                        exposed
                            .merge(ethereum)
                            .map_err(|error| rpc_stage_error("Ethereum方法装配", error))?;
                        tokio::try_join!(
                            async {
                                synchronization
                                    .await
                                    .map_err(|error| rpc_stage_error("Ethereum同步任务", error))
                            },
                            async {
                                rpc_tls
                                    .run(rpc_endpoints, exposed)
                                    .await
                                    .map_err(|error| rpc_stage_error("TLS监听", error))
                            },
                        )?;
                    } else {
                        // 创世旧Runtime尚无Revive，只提供受同一TLS和Safe规则保护的原生RPC。
                        // 正式升级最终确认后由节点重启重新装配Ethereum方法，初始化失败仍终止节点。
                        log::info!(
                            "创世Runtime尚无Revive，原生TLS RPC先行服务；升级最终确认后需重启节点"
                        );
                        rpc_tls
                            .run(rpc_endpoints, exposed)
                            .await
                            .map_err(|error| rpc_stage_error("原生TLS监听", error))?;
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    if let Some(failure) = desktop_rpc_failure {
                        let mut saved = failure
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        if saved.is_none() {
                            *saved = Some(format!("Ethereum RPC/TLS 必要任务失败: {error}"));
                        }
                    }
                    log::error!("Ethereum RPC/TLS 必要任务失败: {error}");
                }
                // 必要任务退出由 TaskManager 终止节点，禁止继续报告 RPC 可用。
            });
    }

    // 普通全节点不会像 GRANDPA voter 那样把交易池交给最终性组件持有。
    // 这里显式让 TaskManager 持有一个 clone，避免 `new_full` 返回后交易池句柄提前释放，
    // 导致 txpool-background 认为所有视图已关闭并触发 essential task 自退。
    let transaction_pool_keepalive = transaction_pool.clone();
    task_manager.spawn_handle().spawn(
        "transaction-pool-keepalive",
        Some("txpool"),
        futures::future::pending::<()>()
            .map(move |_| drop(transaction_pool_keepalive))
            .boxed(),
    );

    // 本链制度要求"安装全节点软件即可参与挖矿"，不再依赖 authority 角色开关。
    ensure_powr_key(&keystore)?;

    let proposer_factory = sc_basic_authorship::ProposerFactory::new(
        task_manager.spawn_handle(),
        client.clone(),
        transaction_pool.clone(),
        prometheus_registry.as_ref(),
        telemetry.as_ref().map(|x| x.handle()),
    );

    let algorithm = nodemining::pow::SimplePow::new(client.clone());
    let (pre_runtime, author_public) = author_pre_digest(&keystore)
        .ok_or_else(|| ServiceError::Other("powr key missing after generation attempt".into()))?;

    let pow_block_import = PowBlockImport::new(
        block_import,
        client.clone(),
        algorithm.clone(),
        0,
        select_chain.clone(),
        |_, ()| async {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
            Ok((timestamp,))
        },
    );

    // 空块不提交：构造一个闭包，返回交易池中允许打包的 ready 交易数。
    // CPU 和 GPU 矿工在交易池为空时跳过挖矿；major sync 期间暂停挖矿，
    // 但 peer 连接状态不属于 PoW 出块权限，无 peer 且有有效交易时仍允许挖矿。
    let pool_ready: Arc<dyn Fn() -> usize + Send + Sync> = {
        use sc_transaction_pool_api::TransactionPool;
        let pool = transaction_pool.clone();
        let sync_service_for_pool = sync_service.clone();
        Arc::new(move || {
            mining_ready_transactions(
                pool.status().ready,
                sync_service_for_pool.is_major_syncing(),
            )
        })
    };

    // PoW mining worker：在 propose 前检查 pool_ready，交易池为空时跳过 propose。
    // 新最佳块刚导入时，交易池维护可能尚未移除已打包交易；此时即使 ready 暂时非零，
    // 也必须先跳过一轮，等待交易池在新链头上稳定，避免构造只有 timestamp 的空 proposal。
    // runtime 的空块断言仍是最终共识闸门，本地门控只负责减少诚实节点的无效提案。
    let should_propose = {
        let pr = pool_ready.clone();
        use sp_blockchain::HeaderBackend;

        let client = client.clone();
        let mining_enabled =
            mining_threads > 0 || cfg!(feature = "gpu-mining") && gpu_device.is_some();
        let stable_best_hash = Arc::new(Mutex::new(client.info().best_hash));

        move || {
            if !mining_enabled {
                return false;
            }

            let current_best = client.info().best_hash;
            let mut observed_best = stable_best_hash
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *observed_best != current_best {
                *observed_best = current_best;
                return false;
            }

            pr() > 0
        }
    };
    // 本地挖矿导入路径与网络导入路径使用相同的节点守卫；启动自检不杀节点，
    // 挖出的坏块仍必须在委派 PoW/GRANDPA 前被拒绝。
    let mining_node_guard =
        nodeguard::guard::NodeGuard::new(pow_block_import, client.clone(), backend.clone());

    // 宪法守卫在挖矿路径同样保持独立最外层，确保最高规则不会被普通节点守卫
    // 的后续扩展、重排或 runtime 升级稀释。
    let guarded_mining_import = nodeguard::constitution::guard::ConstitutionGuard::new(
        mining_node_guard,
        client.clone(),
        backend.clone(),
    )
    .map_err(ServiceError::Other)?;

    let (worker, worker_task) = sc_consensus_pow::start_mining_worker(
        Box::new(guarded_mining_import),
        client.clone(),
        select_chain,
        algorithm,
        proposer_factory,
        NoNetwork,
        (),
        Some(pre_runtime),
        |_, ()| async {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
            Ok((timestamp,))
        },
        Duration::from_secs(POW_MINING_TIMEOUT_SECS),
        Duration::from_secs(POW_PROPOSAL_BUILD_SECS),
        should_propose,
    );

    task_manager.spawn_essential_handle().spawn(
        "pow-worker",
        Some("block-authoring"),
        worker_task.boxed(),
    );

    if mining_threads > 0 {
        let miners = start_cpu_miner(
            worker.clone(),
            mining_threads,
            pool_ready.clone(),
            keystore.clone(),
            author_public,
        );
        task_manager.keep_alive(miners);
    }

    // GPU 矿工（仅在 gpu-mining feature 编译时可用）。
    #[cfg(feature = "gpu-mining")]
    if let Some(device) = gpu_device {
        match nodemining::gpu::try_start(
            worker.clone(),
            device,
            pool_ready.clone(),
            keystore.clone(),
            author_public,
        ) {
            Ok(()) => log::info!("GPU miner started on device {}", device),
            Err(e) => log::warn!("GPU not available, CPU only: {}", e),
        }
    }

    // 避免 unused 警告（无 gpu-mining feature 时 gpu_device 未使用）。
    #[cfg(not(feature = "gpu-mining"))]
    let _ = gpu_device;

    drop(worker);

    if enable_grandpa {
        if has_local_grandpa_authority {
            // 权威节点启动 grandpa-voter，参与最终性投票。
            let grandpa_config = sc_consensus_grandpa::Config {
                gossip_duration: Duration::from_millis(333),
                justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
                name: Some(name),
                observer_enabled: false,
                keystore: Some(keystore.clone()),
                local_role: role,
                telemetry: telemetry.as_ref().map(|x| x.handle()),
                protocol_name: grandpa_protocol_name,
            };

            let grandpa_params = sc_consensus_grandpa::GrandpaParams {
                config: grandpa_config,
                link: grandpa_link,
                network: network.clone(),
                sync: Arc::new(sync_service),
                notification_service: grandpa_notification_service,
                // (ADR-017 出块即固化)：`()` 是官方无约束投票规则，
                // 允许 GRANDPA 投票到链尾(best)。默认规则集含 BeforeBestBlockBy(2)，
                // 在"跳空块"链上会让链尾两块永不固化(死水期 finalized 卡在 best−2)；
                // 全端 finalized 单一口径(ADR-017)依赖本规则放开。
                voting_rule: (),
                prometheus_registry,
                shared_voter_state: sc_consensus_grandpa::SharedVoterState::empty(),
                telemetry: telemetry.as_ref().map(|x| x.handle()),
                offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
            };

            task_manager.spawn_essential_handle().spawn_blocking(
                "grandpa-voter",
                None,
                sc_consensus_grandpa::run_grandpa_voter(grandpa_params)?,
            );
        } else {
            // 普通节点启动 grandpa-observer，只接收最终性结果不投票，
            // 同时消费 notification_service 避免空接收端导致 EssentialTaskClosed。
            let grandpa_config = sc_consensus_grandpa::Config {
                gossip_duration: Duration::from_millis(333),
                justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
                name: Some(name),
                observer_enabled: false,
                keystore: None,
                local_role: role,
                telemetry: telemetry.as_ref().map(|x| x.handle()),
                protocol_name: grandpa_protocol_name,
            };

            task_manager.spawn_handle().spawn_blocking(
                "grandpa-observer",
                None,
                sc_consensus_grandpa::run_grandpa_observer(
                    grandpa_config,
                    grandpa_link,
                    network.clone(),
                    Arc::new(sync_service),
                    grandpa_notification_service,
                )?,
            );
        }
    }

    Ok(task_manager)
}
#[cfg(test)]
#[path = "tests/p2p.rs"]
mod p2p_bad_block_tests;

#[cfg(test)]
#[path = "tests/service.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
#[path = "tests/support.rs"]
pub mod testing;

#[cfg(test)]
#[path = "tests/performance.rs"]
mod performance_tests;
