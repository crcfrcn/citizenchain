//! 清算行节点"本地账面 ↔ 链上 `BankTotalDeposits` 主账对账"。
//!
//!
//! - 本模块是清算行节点的**保底监控**。如果 `settlement::listener`
//!   因进程崩溃 / 滞后 / runtime 事件丢失等原因漏同步,本地 `ledger.accounts[*]
//!   .confirmed` 之和会悄悄偏离链上 `BankTotalDeposits[my_bank]`。没有监控就
//!   没人知道偏差存在,扫码支付会进入"余额虚增 / 虚减"的危险状态。
//! - 本步**只做 log-only 主账对账**,不做自动修复 / 告警外推 / 逐用户 diff。
//!   后续增强:
//!     - Prometheus `clearing_bank_reserve_diff` metric
//!     - 偏差超阈值 → 自动停新扫码支付
//!     - `DepositBalance::iter_key_prefix(my_bank)` 逐户定位
//!
//! 不变式(由 runtime pallet 保证):
//! ```text
//! BankTotalDeposits[my_bank] == Σ DepositBalance[my_bank][*]
//! ```
//! listener 正常时:
//! ```text
//! Σ ledger.accounts[*].confirmed == BankTotalDeposits[my_bank]
//! ```
//!
//! 所以节点侧只读 `BankTotalDeposits[my_bank]`(O(1) 单点 map 读)与本地
//! `confirmed_sum_snapshot()` 比较,即可发现 listener 层面的漂移。

use codec::{Decode, Encode};
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::sync::Arc;
use std::time::Duration;

use crate::core::service::FullClient;

use super::super::ledger::OffchainLedger;

/// 链上 `offchain_transaction` pallet 在 `construct_runtime!` 中注册的实例名。
///
/// Storage key 前缀 `twox_128(PALLET_NAME)` 必须与 runtime 一致,否则读不到值。
const PALLET_NAME: &[u8] = b"OffchainTransaction";

/// `BankTotalDeposits` Storage 名称(pallet 内 `#[pallet::storage]` 项名)。
const STORAGE_BANK_TOTAL_DEPOSITS: &[u8] = b"BankTotalDeposits";

/// 定期对账本地 `Σ confirmed` 与链上 `BankTotalDeposits[my_bank]` 是否一致。
pub struct ReserveMonitor {
    ledger: Arc<OffchainLedger>,
    my_bank: AccountId32,
}

impl ReserveMonitor {
    pub fn new(ledger: Arc<OffchainLedger>, my_bank: AccountId32) -> Self {
        Self { ledger, my_bank }
    }

    /// 后台长循环:每 `interval` 触发一次对账。
    ///
    /// 调用方应 `task_manager.spawn_handle().spawn(...)` 启动;`interval` 通过
    /// CLI `--clearing-reserve-monitor-interval-secs` 配置,默认 300 秒。
    pub async fn run(self: Arc<Self>, client: Arc<FullClient>, interval: Duration) {
        log::info!(
            "[ReserveMonitor] 启动主账对账 interval={}s bank={:?}",
            interval.as_secs(),
            self.my_bank,
        );
        let mut ticker = tokio::time::interval(interval);
        // 首 tick 立即返回,跳过它以免节点启动瞬间、listener 还没追完 chain 头就
        // 误报偏差。第二 tick 起才是"已稳定运行一个 interval"的真实对账时点。
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match self.check_once(client.as_ref()) {
                Ok(()) => {}
                Err(e) => log::warn!("[ReserveMonitor] 对账失败:{e}"),
            }
        }
    }

    /// 单次对账;成功返回 `Ok(())`,链上读取 / 解码错误返回 `Err`。
    ///
    /// 偏差本身走 `log::error!`,**不**返回 `Err`,避免 run loop 出现"偏差→
    /// warning"的双重打印。
    pub fn check_once(&self, client: &FullClient) -> Result<(), String> {
        let best_hash = client.info().best_hash;
        let local = self.ledger.confirmed_sum_snapshot();
        let chain = read_bank_total_deposits(client, best_hash, &self.my_bank)?;
        if local == chain {
            log::debug!("[ReserveMonitor] ok local=chain={local} block={best_hash:?}");
        } else {
            let diff = (local as i128) - (chain as i128);
            log::error!(
                "[ReserveMonitor] 对账偏差! local={local} chain={chain} diff={diff} \
                 block={best_hash:?}"
            );
        }
        Ok(())
    }
}

/// 构造 `BankTotalDeposits[bank]` 的 storage key。
///
/// Substrate `StorageMap<_, Blake2_128Concat, AccountId, _>` 布局:
/// ```text
/// twox_128(PALLET_NAME) ++ twox_128(STORAGE_NAME) ++ blake2_128(bank) ++ bank_scale
/// ```
/// `Blake2_128Concat` 哈希器末尾附带原 key,用于运行时迭代反向解码,节点侧
/// 构造时要一并拼上。
fn bank_total_deposits_key(bank: &AccountId32) -> StorageKey {
    let encoded = bank.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(STORAGE_BANK_TOTAL_DEPOSITS));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    k.extend_from_slice(&encoded);
    StorageKey(k)
}

/// 从 `best_hash` 块读 `BankTotalDeposits[bank]` 并 SCALE decode 为 `u128`。
///
/// `ValueQuery` 的 storage 在 key 不存在时,runtime 侧返回 `Default::default()`
/// (`u128` 默认 0),节点侧 `client.storage()` 返回 `Ok(None)`,本函数同样映射为 0。
fn read_bank_total_deposits(
    client: &FullClient,
    block: H256,
    bank: &AccountId32,
) -> Result<u128, String> {
    let raw = client
        .storage(block, &bank_total_deposits_key(bank))
        .map_err(|e| format!("storage 读取失败:{e}"))?;
    match raw {
        Some(data) => u128::decode(&mut &data.0[..]).map_err(|e| format!("u128 解码失败:{e}")),
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mk_ledger() -> Arc<OffchainLedger> {
        let tmp = std::env::temp_dir().join("offchain_reserve_monitor_test");
        let _ = fs::remove_dir_all(&tmp);
        Arc::new(OffchainLedger::new(&tmp))
    }

    fn acc(b: u8) -> AccountId32 {
        AccountId32::new([b; 32])
    }

    #[test]
    fn confirmed_sum_empty_is_zero() {
        let ledger = mk_ledger();
        assert_eq!(ledger.confirmed_sum_snapshot(), 0);
    }

    #[test]
    fn confirmed_sum_adds_across_users() {
        let ledger = mk_ledger();
        ledger.on_deposited(&acc(1), 300);
        ledger.on_deposited(&acc(2), 700);
        ledger.on_deposited(&acc(1), 100); // 累加同 user
        assert_eq!(ledger.confirmed_sum_snapshot(), 1_100);
    }

    #[test]
    fn confirmed_sum_after_withdraw() {
        let ledger = mk_ledger();
        ledger.on_deposited(&acc(1), 1_000);
        ledger.on_withdrawn(&acc(1), 250);
        assert_eq!(ledger.confirmed_sum_snapshot(), 750);
    }

    #[test]
    fn bank_total_deposits_key_layout_stable() {
        // 关键性质:storage key 必须 = twox_128(pallet) ++ twox_128(storage) ++
        // blake2_128(encoded) ++ encoded。长度与前缀不能随实现漂移,否则读链上
        // 会 silently 命中不同 key,对账永远读到 0 而看不出偏差。
        let bank = acc(0xAA);
        let key = bank_total_deposits_key(&bank);
        let encoded = bank.encode();
        let expected_len = 16 + 16 + 16 + encoded.len();
        assert_eq!(key.0.len(), expected_len);
        assert_eq!(
            &key.0[..16],
            &sp_io::hashing::twox_128(b"OffchainTransaction")
        );
        assert_eq!(
            &key.0[16..32],
            &sp_io::hashing::twox_128(b"BankTotalDeposits")
        );
        assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
        assert_eq!(&key.0[48..], &encoded[..]);
    }

    #[test]
    fn bank_total_deposits_key_differs_per_bank() {
        assert_ne!(
            bank_total_deposits_key(&acc(1)).0,
            bank_total_deposits_key(&acc(2)).0,
        );
    }
}
