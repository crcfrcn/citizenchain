//! 扫码支付清算体系节点层。
//!
//! - 本目录统一承载 node 层清算行功能,包括清算行管理命令、本地账本、
//!   对 citizenapp 的 RPC、批次打包器、链上事件监听同步、主账对账。
//! - 清算行结算依赖的机构身份只读(候选搜索、链上机构详情、管理员集合、动态阈值、CID 注册凭证)
//!   收敛在 `institution_read` 子模块;机构创建归 onchina 控制台,节点不承接。
//! - 本 mod 直接挂载扫码收单+RPC+ledger 文件;`settlement` 子目录承载结算 worker。
//! - `service::new_full` 检测到 `--clearing-bank` CLI flag 时,调
//!   `start_clearing_bank_components` 启动本目录下的组件,并 spawn:
//!     - `offchain-clearing-packer`(30 秒 tick)
//!     - `offchain-clearing-event-listener`(订阅 import_notification_stream)
//!     - `offchain-clearing-reserve-monitor`(主账对账)
//!
//!   不加 `--clearing-bank` 的节点仅跑 PoW + GRANDPA,跳过本目录所有启动。
//!
//! 模块边界:
//! - 本 mod 平铺文件:扫码支付收单与清算行节点声明 + RPC + 本地账本。
//! - `settlement`:只管本清算行交易打包上链与结算 worker。

// 清算组件装配函数逐项接收服务句柄和机构配置，保持既有调用边界。
#![allow(clippy::too_many_arguments)]
//! 清算行 offchain 组件启动接线。
//!
// 启动装配函数逐项接收 Substrate 服务句柄和清算行配置，保持现有生命周期边界。
#![allow(clippy::too_many_arguments)]
use crate::keystore::SigningKey;
use crate::ledger::OffchainLedger;
use crate::listener::EventListener;
use crate::packer::BatchSigner;
use crate::packer::BatchSubmitter;
use crate::packer::NoopBatchSigner;
use crate::packer::NoopBatchSubmitter;
use crate::packer::OffchainPacker;
use crate::reserve::ReserveMonitor;
use crate::rpc::OffchainClearingRpcImpl;
use crate::submitter::TxPool;
use codec::Decode;
use codec::Encode;
use primitives::account_derive::AccountKind;
use primitives::core_const::SS58_FORMAT;
use sc_client_api::StorageProvider;
use sc_service::TaskManager;
use sp_blockchain::HeaderBackend;
use sp_core::crypto::Ss58Codec;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

/// 清算行节点一次性启动时组装的组件集合。
///
///
/// - `service.rs` 在检测到节点角色是"清算行"时调 `start_clearing_bank_components`
///   拿回本结构,并把 `rpc_impl` 注册到 JSON-RPC 命名空间,`packer` 交给后台
///   worker,`event_listener` 订阅链上事件。
/// - 完成 **组装 + 持久化恢复**;extrinsic 提交 / libp2p
///   gossip / 链上事件订阅由 listener / submitter 接入。
pub struct OffchainComponents {
    /// 本地 L3 账本句柄。当前 worker 通过 `packer` / `event_listener` / `rpc_impl`
    /// 间接持有它,字段保留给后续清算行运维状态查询使用。
    #[allow(dead_code)]
    pub ledger: Arc<OffchainLedger>,
    pub packer: Arc<OffchainPacker>,
    pub event_listener: Arc<EventListener>,
    /// 本地 `Σ confirmed` 与链上 `BankTotalDeposits`
    /// 主账对账 worker(`run` 需要调用方 spawn 到 `task_manager`)。
    pub reserve_monitor: Arc<ReserveMonitor>,
    pub rpc_impl: Arc<OffchainClearingRpcImpl>,
}

/// 启动清算行节点所需的 offchain 组件套件。
///
/// [`base_path`]  节点数据根目录(下挂 `offchain_step1/ledger.enc`)。
/// [`actor_cid_number`] 本清算行机构唯一主键。
/// [`actor_role_code`] 提交批次的机构岗位码。
/// [`institution_account_id`] 本清算行**主账户 ID**（身份锚），用于 packer 批次 signing
///                message 拼接与发 extrinsic;`EventListener` 事件过滤按 CID(actor_cid_number)。
/// [`password`]   节点启动时用于 AES-256-GCM 风格加密 ledger 的对称密钥字符串
///                (目前实现为 blake2b_256(password) XOR 流 + HMAC,见 `ledger.rs`)。
/// [`signer`]     批次签名器。未接入时传 `NoopBatchSigner`;接入后
///                传真实 `KeystoreBatchSigner`(从 `nodeoffchain::keystore` 派生)。
/// [`submitter`]  extrinsic 提交器。未接入时传 `NoopBatchSubmitter`;接入后
///                传真实 `PoolBatchSubmitter`(拼 RuntimeCall + 调
///                `TransactionPool`)。
///
/// 返回值:`Arc` 包裹的组件集合,调用方持有 ownership 决定生命周期。
pub fn start_clearing_bank_components(
    base_path: &Path,
    actor_cid_number: Vec<u8>,
    actor_role_code: Vec<u8>,
    institution_account_id: AccountId32,
    password: &str,
    signer: Arc<dyn BatchSigner>,
    submitter: Arc<dyn BatchSubmitter>,
    client: Arc<nodeclient::types::FullClient>,
) -> Result<OffchainComponents, String> {
    let ledger = Arc::new(crate::ledger::OffchainLedger::new(base_path));
    // 若磁盘有上次加密持久化的 ledger,尝试恢复;首次启动(文件不存在)返回 Ok(0)。
    ledger.load_from_disk(password)?;
    // 节点自身身份 = 清算行 CID(actor_cid_number,来自节点配置);主账户仅用于发 extrinsic / 偿付监控。
    let initial_batch_seq = read_last_clearing_batch_seq(client.as_ref(), &actor_cid_number)
        .map_err(|e| format!("读取 LastClearingBatchSeq 失败:{e}"))?;

    let packer = Arc::new(crate::packer::OffchainPacker::new_with_initial_seq(
        ledger.clone(),
        actor_cid_number.clone(),
        actor_role_code,
        institution_account_id.clone(),
        signer.clone(),
        submitter,
        initial_batch_seq,
    ));
    let event_listener = Arc::new(crate::listener::EventListener::new(
        ledger.clone(),
        actor_cid_number.clone(),
    ));
    let reserve_monitor = Arc::new(crate::reserve::ReserveMonitor::new(
        ledger.clone(),
        institution_account_id.clone(),
    ));
    // RPC 层需要 client 读取 UserBank / L2FeeRateBp,也复用 signer 生成真实 L2 ACK。
    let rpc_impl = Arc::new(crate::rpc::OffchainClearingRpcImpl::new(
        ledger.clone(),
        client,
        actor_cid_number,
        signer,
    ));

    Ok(OffchainComponents {
        ledger,
        packer,
        event_listener,
        reserve_monitor,
        rpc_impl,
    })
}

const PALLET_NAME: &[u8] = b"OffchainTransaction";

/// 构造 `LastClearingBatchSeq[bank]` 的 storage key。
fn last_clearing_batch_seq_key(bank_cid: &[u8]) -> StorageKey {
    // CID 键 = InstitutionCidNumber(BoundedVec<u8>),SCALE = Compact(len) || bytes。
    let encoded = bank_cid.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"LastClearingBatchSeq"));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    k.extend_from_slice(&encoded);
    StorageKey(k)
}

/// 读取链上已成功落账的最新 batch_seq。storage 不存在时按 `ValueQuery` 映射为 0。
fn read_last_clearing_batch_seq(
    client: &nodeclient::types::FullClient,
    bank_cid: &[u8],
) -> Result<u64, String> {
    let best = client.info().best_hash;
    let raw = client
        .storage(best, &last_clearing_batch_seq_key(bank_cid))
        .map_err(|e| format!("storage 读取失败:{e}"))?;
    match raw {
        Some(data) => u64::decode(&mut &data.0[..]).map_err(|e| format!("u64 解码失败:{e}")),
        None => Ok(0),
    }
}

/// 默认启动器:signer / submitter 走 Noop 占位。
///
/// 测试或降级启动时调用此便捷版;上链提交与 `submit_payment` 的 L2 ACK 签名
/// 都会 fail-fast,只读查询路径仍可用。
#[allow(dead_code)]
pub fn start_clearing_bank_components_with_noop(
    base_path: &Path,
    actor_cid_number: Vec<u8>,
    actor_role_code: Vec<u8>,
    institution_account_id: AccountId32,
    password: &str,
    client: Arc<nodeclient::types::FullClient>,
) -> Result<OffchainComponents, String> {
    start_clearing_bank_components(
        base_path,
        actor_cid_number,
        actor_role_code,
        institution_account_id,
        password,
        Arc::new(NoopBatchSigner),
        Arc::new(NoopBatchSubmitter),
        client,
    )
}

/// 根据 CLI 参数启动清算行 offchain 运行组件。
///
/// 返回值为可注入 JSON-RPC 的 `OffchainClearingRpcImpl`。如果未指定清算行、
/// 地址无效或组件启动失败,返回 `None`,普通 PoW + GRANDPA 节点继续运行。
pub fn start_from_cli(
    bank_cid: Option<&str>,
    bank_role: Option<&str>,
    clearing_bank_password: Option<&str>,
    reserve_interval_secs: Option<u64>,
    base_path: &Path,
    client: Arc<nodeclient::types::FullClient>,
    transaction_pool: Arc<TxPool>,
    task_manager: &TaskManager,
) -> Option<Arc<OffchainClearingRpcImpl>> {
    let raw_cid_number = bank_cid?;

    let actor_cid_number = match primitives::cid::number::validate_cid_number_format(raw_cid_number)
    {
        Ok(cid_number) => cid_number,
        Err(e) => {
            log::warn!("[ClearingBank] --clearing-bank-cid-number 格式无效:{e},清算行组件不启动");
            return None;
        }
    };
    let Some(actor_role_code) = bank_role
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64)
    else {
        log::warn!("[ClearingBank] 缺少有效 --clearing-bank-role-code,清算行组件不启动");
        return None;
    };
    let institution_account_id = AccountId32::new(
        AccountKind::InstitutionMain {
            cid_number: actor_cid_number.as_bytes(),
        }
        .derive(SS58_FORMAT),
    );

    let password = clearing_bank_password.unwrap_or("");
    let signing_key_slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(None));
    let keystore = crate::keystore::OffchainKeystore::new(base_path);

    if keystore.has_signing_key() && !password.is_empty() {
        match keystore.load_signing_key(password) {
            Ok(key) => {
                match signing_key_slot.write() {
                    Ok(mut slot) => *slot = Some(key),
                    Err(err) => *err.into_inner() = Some(key),
                }
                log::info!("[ClearingBank] 签名密钥已解锁");
            }
            Err(e) => {
                log::warn!("[ClearingBank] 签名密钥解锁失败:{e},packer 将拒绝提交 extrinsic");
            }
        }
    } else {
        log::warn!(
            "[ClearingBank] 签名密钥未加载(密码或密钥文件缺失),packer 会在有 pending 时 rollback"
        );
    }

    let signer: Arc<dyn BatchSigner> = Arc::new(crate::signer::KeystoreBatchSigner::new(
        signing_key_slot.clone(),
    ));
    let submitter: Arc<dyn BatchSubmitter> = Arc::new(crate::submitter::PoolBatchSubmitter::new(
        client.clone(),
        transaction_pool,
        signing_key_slot,
    ));

    let components = match start_clearing_bank_components(
        base_path,
        actor_cid_number.as_bytes().to_vec(),
        actor_role_code.as_bytes().to_vec(),
        institution_account_id.clone(),
        password,
        signer,
        submitter,
        client.clone(),
    ) {
        Ok(components) => components,
        Err(e) => {
            log::warn!("[ClearingBank] 组件启动失败:{e}");
            return None;
        }
    };

    spawn_packer_worker(task_manager, client.clone(), components.packer.clone());
    spawn_listener_worker(
        task_manager,
        client.clone(),
        components.event_listener.clone(),
    );
    spawn_reserve_worker(
        task_manager,
        client,
        components.reserve_monitor.clone(),
        reserve_interval_secs,
    );

    log::info!(
        "[ClearingBank] 清算行组件已启动,actor_cid_number={},institution_account_id={}",
        actor_cid_number,
        institution_account_id.to_ss58check()
    );
    Some(components.rpc_impl.clone())
}

fn spawn_packer_worker(
    task_manager: &TaskManager,
    client: Arc<nodeclient::types::FullClient>,
    packer: Arc<crate::packer::OffchainPacker>,
) {
    task_manager
        .spawn_handle()
        .spawn("offchain-clearing-packer", Some("offchain"), async move {
            // HeaderBackend/SaturatedConversion 只在 worker 内使用,放在这里避免污染 service.rs。
            use sp_blockchain::HeaderBackend as _;
            use sp_runtime::traits::SaturatedConversion as _;

            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let info = client.info();
                let current_block: u64 = info.best_number.saturated_into();
                if packer.should_pack(current_block).await {
                    match packer.pack_and_submit(current_block).await {
                        Ok(Some(hash)) => log::info!("[ClearingPacker] batch ok tx=0x{:x}", hash),
                        Ok(None) => {}
                        Err(e) => log::warn!("[ClearingPacker] {e}"),
                    }
                }
            }
        });
}

fn spawn_listener_worker(
    task_manager: &TaskManager,
    client: Arc<nodeclient::types::FullClient>,
    listener: Arc<crate::listener::EventListener>,
) {
    task_manager.spawn_handle().spawn(
        "offchain-clearing-event-listener",
        Some("offchain"),
        async move {
            listener.run(client).await;
        },
    );
}

fn spawn_reserve_worker(
    task_manager: &TaskManager,
    client: Arc<nodeclient::types::FullClient>,
    monitor: Arc<crate::reserve::ReserveMonitor>,
    reserve_interval_secs: Option<u64>,
) {
    let monitor_interval_secs = reserve_interval_secs.unwrap_or(300);
    if monitor_interval_secs == 0 {
        log::warn!(
            "[ClearingBank] reserve monitor 已关闭(interval=0),仅用于排障,生产环境请保留默认 300 秒"
        );
        return;
    }

    task_manager.spawn_handle().spawn(
        "offchain-clearing-reserve-monitor",
        Some("offchain"),
        async move {
            monitor
                .run(client, Duration::from_secs(monitor_interval_secs))
                .await;
        },
    );
}
#[cfg(test)]
// 便捷启动器保持在测试之后，测试可直接覆盖其依赖的私有 storage-key 函数。
#[allow(clippy::items_after_test_module)]
#[path = "../tests/integration.rs"]
mod tests;
