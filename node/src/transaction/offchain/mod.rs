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

pub mod commands;
pub mod endpoint;
pub mod health;
pub mod institution_read;
pub mod ledger;
pub mod rpc;
pub mod settlement;
pub mod signing;
pub mod types;

use codec::{Decode, Encode};
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::path::Path;
use std::sync::Arc;

use self::ledger::OffchainLedger;
use self::rpc::OffchainClearingRpcImpl;
use self::settlement::listener::EventListener;
use self::settlement::packer::{
    BatchSigner, BatchSubmitter, NoopBatchSigner, NoopBatchSubmitter, OffchainPacker,
};
use self::settlement::reserve::ReserveMonitor;

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
///                传真实 `KeystoreBatchSigner`(从 `offchain::settlement::keystore` 派生)。
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
    client: Arc<crate::core::service::FullClient>,
) -> Result<OffchainComponents, String> {
    let ledger = Arc::new(OffchainLedger::new(base_path));
    // 若磁盘有上次加密持久化的 ledger,尝试恢复;首次启动(文件不存在)返回 Ok(0)。
    ledger.load_from_disk(password)?;
    // 节点自身身份 = 清算行 CID(actor_cid_number,来自节点配置);主账户仅用于发 extrinsic / 偿付监控。
    let initial_batch_seq = read_last_clearing_batch_seq(client.as_ref(), &actor_cid_number)
        .map_err(|e| format!("读取 LastClearingBatchSeq 失败:{e}"))?;

    let packer = Arc::new(OffchainPacker::new_with_initial_seq(
        ledger.clone(),
        actor_cid_number.clone(),
        actor_role_code,
        institution_account_id.clone(),
        signer.clone(),
        submitter,
        initial_batch_seq,
    ));
    let event_listener = Arc::new(EventListener::new(ledger.clone(), actor_cid_number.clone()));
    let reserve_monitor = Arc::new(ReserveMonitor::new(
        ledger.clone(),
        institution_account_id.clone(),
    ));
    // RPC 层需要 client 读取 UserBank / L2FeeRateBp,也复用 signer 生成真实 L2 ACK。
    let rpc_impl = Arc::new(OffchainClearingRpcImpl::new(
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
    client: &crate::core::service::FullClient,
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

#[cfg(test)]
// 便捷启动器保持在测试之后，测试可直接覆盖其依赖的私有 storage-key 函数。
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn last_clearing_batch_seq_key_layout_stable() {
        let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        let encoded = bank.encode();
        let key = last_clearing_batch_seq_key(&bank);
        assert_eq!(key.0.len(), 16 + 16 + 16 + encoded.len());
        assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
        assert_eq!(
            &key.0[16..32],
            &sp_io::hashing::twox_128(b"LastClearingBatchSeq")
        );
        assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
        assert_eq!(&key.0[48..], &encoded[..]);
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
    client: Arc<crate::core::service::FullClient>,
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
