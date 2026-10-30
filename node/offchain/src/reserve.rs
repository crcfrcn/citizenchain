use crate::ledger::OffchainLedger;
use codec::Decode;
use codec::Encode;
use nodeclient::types::FullClient;
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::sync::Arc;
use std::time::Duration;

/// 链上 `offchain_transaction` pallet 在 `construct_runtime!` 中注册的实例名。
///
/// Storage key 前缀 `twox_128(PALLET_NAME)` 必须与 runtime 一致,否则读不到值。
const PALLET_NAME: &[u8] = b"OffchainTransaction";

/// `BankTotalDeposits` Storage 名称(pallet 内 `#[pallet::storage]` 项名)。
const STORAGE_BANK_TOTAL_DEPOSITS: &[u8] = b"BankTotalDeposits";

/// 定期对账本地 `Σ confirmed` 与链上 `BankTotalDeposits[my_bank]` 是否一致。
pub(crate) struct ReserveMonitor {
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
#[path = "../tests/reserve.rs"]
mod tests;
