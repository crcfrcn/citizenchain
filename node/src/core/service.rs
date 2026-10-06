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

use citizenchain::{self, apis::RuntimeApi, opaque::Block};
use codec::{Decode, Encode};
use futures::FutureExt;
use sc_client_api::{Backend, BlockBackend, StorageProvider};
use sc_consensus_pow::{MiningHandle, PowAlgorithm, PowBlockImport};
use sc_network::NetworkBackend as _;
use sc_service::WarpSyncConfig;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_consensus::{NoNetwork, SyncOracle};
use sp_core::{crypto::KeyTypeId, sr25519, Pair as _, U256};
use sp_crypto_hashing::{blake2_256, twox_128};
use sp_keystore::Keystore;
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// CPU 全线程合计哈希率（hashes/sec），以 f64 bits 存入 AtomicU64。
static CPU_HASHRATE: AtomicU64 = AtomicU64::new(0);

// 空块 proposal 防护由 mining worker 的 should_propose 与 CPU/GPU 交易池门控共同实现。

/// 获取当前 CPU 哈希率（hashes/sec）。
pub(crate) fn cpu_hashrate() -> f64 {
    f64::from_bits(CPU_HASHRATE.load(Ordering::Relaxed))
}

pub(crate) type FullClient = sc_service::TFullClient<
    Block,
    RuntimeApi,
    sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;
pub(crate) type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub type Service = sc_service::PartialComponents<
    FullClient,
    FullBackend,
    FullSelectChain,
    sc_consensus::DefaultImportQueue<Block>,
    sc_transaction_pool::TransactionPoolHandle<Block, FullClient>,
    (
        sc_consensus_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
        sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
        Option<Telemetry>,
    ),
>;

// PoW 作者密钥类型：纯 PoW 链使用独立 key type，避免与 Aura 语义混用。
pub(crate) const POW_AUTHOR_KEY_TYPE: KeyTypeId = KeyTypeId(*b"powr");
const POW_MINING_TIMEOUT_SECS: u64 = 2;
const POW_PROPOSAL_BUILD_SECS: u64 = 2;
const GRANDPA_JUSTIFICATION_PERIOD: u32 = 64;

#[derive(Clone)]
pub(crate) struct SimplePow {
    /// 持有 client 引用，直接读取父块 RAW 难度，避免信任可升级 Runtime API。
    client: Arc<FullClient>,
}

impl SimplePow {
    /// 核心服务与真实区块验收共用同一难度读取和 seal 校验实现。
    pub(super) fn new(client: Arc<FullClient>) -> Self {
        Self { client }
    }
}

impl PowAlgorithm<Block> for SimplePow {
    type Difficulty = U256;

    /// 从父块状态读取当前 PoW 难度；缺失、畸形、尾随字节或零值全部 fail-closed。
    fn difficulty(
        &self,
        parent: <Block as BlockT>::Hash,
    ) -> Result<Self::Difficulty, sc_consensus_pow::Error<Block>> {
        let key = [
            twox_128(b"PowDifficulty").as_slice(),
            twox_128(b"CurrentDifficulty").as_slice(),
        ]
        .concat();
        let raw = self
            .client
            .storage(parent, &StorageKey(key))
            .map_err(|e| sc_consensus_pow::Error::BlockProposingError(e.to_string()))?
            .ok_or_else(|| {
                sc_consensus_pow::Error::BlockProposingError(
                    "父块缺少 PowDifficulty::CurrentDifficulty".into(),
                )
            })?;
        let mut input = raw.0.as_slice();
        let difficulty = u64::decode(&mut input).map_err(|_| {
            sc_consensus_pow::Error::BlockProposingError(
                "PowDifficulty::CurrentDifficulty SCALE 解码失败".into(),
            )
        })?;
        if !input.is_empty() || difficulty == 0 {
            return Err(sc_consensus_pow::Error::BlockProposingError(
                "PowDifficulty::CurrentDifficulty 非规范或为零".into(),
            ));
        }
        Ok(U256::from(difficulty))
    }

    fn verify(
        &self,
        _parent: &sp_runtime::generic::BlockId<Block>,
        pre_hash: &<Block as BlockT>::Hash,
        pre_digest: Option<&[u8]>,
        seal: &sp_consensus_pow::Seal,
        difficulty: Self::Difficulty,
    ) -> Result<bool, sc_consensus_pow::Error<Block>> {
        // pre_digest 包含矿工 sr25519 公钥，seal 包含 (nonce, 签名)。
        // 验证：1) PoW 难度满足  2) 签名证明矿工确实拥有该公钥的私钥。
        let Some(pre_digest) = pre_digest else {
            return Ok(false);
        };
        let public = match sr25519::Public::decode(&mut &pre_digest[..]) {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };

        let (nonce, signature): (u64, sr25519::Signature) =
            Decode::decode(&mut &seal[..]).map_err(sc_consensus_pow::Error::<Block>::Codec)?;

        let hash = pow_hash(pre_hash.as_ref(), nonce);
        if !hash_meets_difficulty(&hash, difficulty) {
            return Ok(false);
        }

        // 验证矿工对 pre_hash 的 sr25519 签名，防止冒充他人公钥。
        Ok(sr25519::Pair::verify(
            &signature,
            pre_hash.as_ref(),
            &public,
        ))
    }
}

// RPC 运行态验收复用实际矿工的 PoW 计算，不另建测试算法。
pub(super) fn pow_hash(pre_hash: &[u8], nonce: u64) -> [u8; 32] {
    let mut payload = Vec::with_capacity(pre_hash.len() + std::mem::size_of::<u64>());
    payload.extend_from_slice(pre_hash);
    payload.extend_from_slice(&nonce.to_le_bytes());
    blake2_256(&payload)
}

pub(super) fn hash_meets_difficulty(hash: &[u8; 32], difficulty: U256) -> bool {
    if difficulty.is_zero() {
        return false;
    }
    let target = U256::MAX / difficulty;
    U256::from_big_endian(hash) <= target
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

fn start_cpu_miner(
    worker: MiningHandle<Block, SimplePow, ()>,
    num_threads: usize,
    pool_ready: Arc<dyn Fn() -> usize + Send + Sync>,
    keystore: sp_keystore::KeystorePtr,
    author_public: sr25519::Public,
) {
    let stride = (num_threads as u64).max(1);

    for thread_id in 0..num_threads {
        let worker = worker.clone();
        let pool_ready = pool_ready.clone();
        let keystore = keystore.clone();
        thread::spawn(move || {
            // 哈希率采样：仅 thread 0 每 SAMPLE_INTERVAL 次哈希统计一次，乘以线程数得到总哈希率。
            const SAMPLE_INTERVAL: u64 = 100_000;
            let mut sample_count: u64 = 0;
            let mut sample_start = Instant::now();

            loop {
                let Some(metadata) = worker.metadata() else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                // 空块不提交：交易池无待打包交易时不挖矿，避免产生空块。
                if pool_ready() == 0 {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let build_version = worker.version();

                // 共同随机基址（来自 pre_hash 前 8 字节）+ 线程号错位 + stride = 线程数。
                // 每轮 metadata 变化时基址自动更换；同一轮内各线程搜索的 nonce 集合完全不重叠。
                let random_base = {
                    let seed_bytes = metadata.pre_hash.as_ref();
                    let seed = u64::from_le_bytes(seed_bytes[..8].try_into().unwrap_or([0u8; 8]));
                    // CPU 使用低半区 nonce（bit 63 = 0），高半区留给 GPU。
                    seed & 0x7FFFFFFFFFFFFFFF
                };
                let mut nonce = random_base.wrapping_add(thread_id as u64);

                loop {
                    if worker.version() != build_version {
                        break;
                    }

                    // thread 0 负责采样更新全局哈希率。
                    if thread_id == 0 {
                        sample_count += 1;
                        if sample_count >= SAMPLE_INTERVAL {
                            let elapsed = sample_start.elapsed();
                            if elapsed.as_nanos() > 0 {
                                let per_thread = sample_count as f64 / elapsed.as_secs_f64();
                                let total = per_thread * stride as f64;
                                CPU_HASHRATE.store(total.to_bits(), Ordering::Relaxed);
                            }
                            sample_count = 0;
                            sample_start = Instant::now();
                        }
                    }

                    let hash = pow_hash(metadata.pre_hash.as_ref(), nonce);
                    if hash_meets_difficulty(&hash, metadata.difficulty) {
                        // 有效工作量证明找到后立即提交；只防止提交已经过期的工作。
                        if worker.version() != build_version {
                            break; // nonce 已过期，回外层重新获取 metadata
                        }

                        // 签名 pre_hash 证明矿工身份，签名失败则丢弃该 nonce。
                        let signature = match keystore.sr25519_sign(
                            POW_AUTHOR_KEY_TYPE,
                            &author_public,
                            metadata.pre_hash.as_ref(),
                        ) {
                            Ok(Some(sig)) => sig,
                            _ => {
                                log::warn!("PoW: keystore 签名失败，丢弃 nonce");
                                break;
                            }
                        };
                        let seal = (nonce, sr25519::Signature::from(signature)).encode();
                        let _submitted = futures::executor::block_on(worker.submit(seal));
                        break;
                    }

                    nonce = nonce.wrapping_add(stride);
                }
            }
        });
    }
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

pub fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
    #[cfg(test)]
    return new_partial_inner(config, None);
    #[cfg(not(test))]
    new_partial_inner(config)
}

// 正式入口始终使用冻结身份；此独立入口不进入任何正式节点二进制。
#[cfg(test)]
pub(super) fn new_partial_for_test(config: &Configuration) -> Result<Service, ServiceError> {
    let identity = crate::core::node_guard::TestGenesisIdentity::from_chain_spec(
        config.chain_spec.as_ref(),
    ).map_err(ServiceError::Other)?;
    new_partial_inner(config, Some(identity))
}

fn new_partial_inner(
    config: &Configuration,
    #[cfg(test)] test_genesis_identity: Option<crate::core::node_guard::TestGenesisIdentity>,
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

    let algorithm = SimplePow::new(client.clone());
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
    #[cfg(test)]
    let node_guard = match test_genesis_identity {
        Some(identity) => crate::core::node_guard::NodeGuard::new_for_test(
            pow_block_import, client.clone(), backend.clone(), identity,
        ),
        None => crate::core::node_guard::NodeGuard::new(
            pow_block_import, client.clone(), backend.clone(),
        ),
    };
    #[cfg(not(test))]
    let node_guard =
        crate::core::node_guard::NodeGuard::new(pow_block_import, client.clone(), backend.clone());

    // 宪法守卫保持独立且位于最外层(ADR-027 §7)：先执行整条链最高优先级的
    // 不可修改条款校验，再进入节点守卫及 PoW 导入；runtime 升级无法绕过两层执法。
    let guarded_import = crate::core::constitution::ConstitutionGuard::new(
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
type NetworkBackend = sc_network::NetworkWorker<Block, <Block as sp_runtime::traits::Block>::Hash>;

/// Builds a new service for a full client.
pub fn new_full(
    mut config: Configuration,
    mining_threads: usize,
    gpu_device: Option<usize>,
    // 清算行机构 CID(None=本节点不做清算行角色)
    clearing_bank_cid_number: Option<String>,
    // 清算批次提交岗位码；必须与 CID 和签名钱包任职同时匹配
    clearing_bank_role_code: Option<String>,
    // 解锁 `offchain::settlement::keystore` 的密码
    clearing_bank_password: Option<String>,
    // offchain::settlement::reserve 对账周期(秒),None=默认 300,Some(0)=关闭
    clearing_reserve_monitor_interval_secs: Option<u64>,
) -> Result<TaskManager, ServiceError> {
    // 先装载可信 RPC 证书并取走端点，禁止上游 spawn_tasks 同时开启明文监听。
    // 进程内 RpcHandlers 仍正常创建，CLI 和桌面共用本服务入口。
    let rpc_endpoints = config.rpc.addr.take().unwrap_or_default();
    let rpc_tls = if rpc_endpoints.is_empty() {
        None
    } else {
        Some(crate::core::rpc_tls::RpcTls::load()
            .map_err(|error| ServiceError::Other(format!("装载 RPC TLS 失败: {error}")))?)
    };
    // 生成或加载 TLS 自签证书，注入到网络配置中。
    let tls_cert = crate::core::tls_cert::load_or_generate_tls_cert(config.base_path.path())
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
    let metrics = NetworkBackend::register_notification_metrics(config.prometheus_registry());
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
                Some(crate::mining::gpu_miner::gpu_hashrate as fn() -> f64)
            } else {
                None
            }
        }
        #[cfg(not(feature = "gpu-mining"))]
        {
            None
        }
    };

    // 清算行启动细节归入 `transaction::offchain::settlement::bootstrap`,service.rs 只做节点通用接线。
    let clearing_rpc_impl = crate::transaction::offchain::settlement::bootstrap::start_from_cli(
        clearing_bank_cid_number.as_deref(),
        clearing_bank_role_code.as_deref(),
        clearing_bank_password.as_deref(),
        clearing_reserve_monitor_interval_secs,
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
            let deps = crate::core::rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                keystore: keystore.clone(),
                cpu_hashrate_fn: cpu_hashrate as fn() -> f64,
                gpu_hashrate_fn,
                // 清算行 RPC 命名空间(None 时跳过注入)
                offchain_clearing_rpc: clearing_rpc_impl.clone(),
            };
            crate::core::rpc::create_full(deps).map_err(Into::into)
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

    if let Some(rpc_tls) = rpc_tls {
        // 原生后端仅在进程内使用；公开副本显式覆盖为 Safe，防止 Unsafe 扩展泄漏。
        let native = (*rpc_handlers.handle()).clone();
        task_manager.spawn_essential_handle().spawn("ethereum-rpc-tls", Some("rpc"), async move {
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                let (ethereum, synchronization) = crate::core::ethereum_rpc::initialize(native.clone()).await?;
                let mut exposed = native;
                exposed.merge(ethereum)?;
                tokio::try_join!(
                    async { synchronization.await.map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) }) },
                    rpc_tls.run(rpc_endpoints, exposed),
                )?;
                Ok(())
            }.await;
            if let Err(error) = result {
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

    let algorithm = SimplePow::new(client.clone());
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
        crate::core::node_guard::NodeGuard::new(pow_block_import, client.clone(), backend.clone());

    // 宪法守卫在挖矿路径同样保持独立最外层，确保最高规则不会被普通节点守卫
    // 的后续扩展、重排或 runtime 升级稀释。
    let guarded_mining_import = crate::core::constitution::ConstitutionGuard::new(
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
        start_cpu_miner(
            worker.clone(),
            mining_threads,
            pool_ready.clone(),
            keystore.clone(),
            author_public,
        );
    }

    // GPU 矿工（仅在 gpu-mining feature 编译时可用）。
    #[cfg(feature = "gpu-mining")]
    if let Some(device) = gpu_device {
        match crate::mining::gpu_miner::try_start(
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
#[path = "p2p_bad_block_tests.rs"]
mod p2p_bad_block_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_hash_deterministic() {
        let pre_hash = [0u8; 32];
        let h1 = pow_hash(&pre_hash, 42);
        let h2 = pow_hash(&pre_hash, 42);
        assert_eq!(h1, h2);
    }

    #[test]
    fn pow_hash_differs_with_different_nonce() {
        let pre_hash = [1u8; 32];
        assert_ne!(pow_hash(&pre_hash, 0), pow_hash(&pre_hash, 1));
    }

    #[test]
    fn pow_hash_differs_with_different_pre_hash() {
        assert_ne!(pow_hash(&[0u8; 32], 0), pow_hash(&[1u8; 32], 0));
    }

    #[test]
    fn pow_hash_matches_manual_blake2() {
        let pre_hash = [7u8; 32];
        let nonce = 123u64;
        let mut payload = Vec::new();
        payload.extend_from_slice(&pre_hash);
        payload.extend_from_slice(&nonce.to_le_bytes());
        assert_eq!(pow_hash(&pre_hash, nonce), blake2_256(&payload));
    }

    #[test]
    fn hash_meets_difficulty_zero_always_false() {
        assert!(!hash_meets_difficulty(&[0u8; 32], U256::zero()));
    }

    #[test]
    fn hash_meets_difficulty_one_always_true() {
        // difficulty=1 → target=U256::MAX, any hash passes
        assert!(hash_meets_difficulty(&[0xFF; 32], U256::one()));
    }

    #[test]
    fn hash_meets_difficulty_max_only_zero_hash() {
        // difficulty=U256::MAX → target=1, only hash ≤ 1 passes
        assert!(hash_meets_difficulty(&[0u8; 32], U256::MAX));
        let mut h = [0u8; 32];
        h[31] = 2;
        assert!(!hash_meets_difficulty(&h, U256::MAX));
    }

    #[test]
    fn hash_meets_difficulty_boundary() {
        let difficulty = U256::from(2);
        let target = U256::MAX / difficulty;
        // At target: pass
        let at_target: [u8; 32] = target.to_big_endian();
        assert!(hash_meets_difficulty(&at_target, difficulty));
        // Above target: fail
        let above = target + U256::one();
        let above_bytes: [u8; 32] = above.to_big_endian();
        assert!(!hash_meets_difficulty(&above_bytes, difficulty));
    }

    #[test]
    fn mining_ready_transactions_keeps_nonempty_pool_when_not_major_syncing() {
        // peer 状态不再是参数：没有 peer 不能抹掉交易池中的有效交易。
        assert_eq!(mining_ready_transactions(3, false), 3);
    }

    #[test]
    fn mining_ready_transactions_keeps_empty_pool_blocked() {
        assert_eq!(mining_ready_transactions(0, false), 0);
    }

    #[test]
    fn mining_ready_transactions_blocks_during_major_sync() {
        assert_eq!(mining_ready_transactions(3, true), 0);
    }
}
