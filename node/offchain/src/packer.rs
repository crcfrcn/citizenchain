use crate::ledger::OffchainLedger;
use crate::ledger::PendingPayment;
#[cfg(test)]
use codec::DecodeAll;
use codec::Encode;
use offchain::batch_item::batch_signing_hash;
use offchain::batch_item::OffchainBatchItem;
use sp_core::H256;
use sp_runtime::AccountId32;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 打包触发阈值:笔数上限(与链上 `PACK_TX_THRESHOLD` 对齐)。
pub const PACK_TX_THRESHOLD: usize = 100_000;

/// 打包触发阈值:距上次打包区块数(与链上 `PACK_BLOCK_THRESHOLD` 对齐)。
pub const PACK_BLOCK_THRESHOLD: u64 = 10;

// 直接消费所属 pallet 的公开批次项，不在节点维护 SCALE 字段序镜像。
impl From<PendingPayment> for OffchainBatchItem<AccountId32, u32> {
    fn from(p: PendingPayment) -> Self {
        Self {
            tx_id: p.intent.tx_id,
            payer_account_id: p.intent.payer_account_id,
            payer_bank_cid: p.intent.payer_bank_cid,
            recipient_account_id: p.intent.recipient_account_id,
            recipient_bank_cid: p.intent.recipient_bank_cid,
            transfer_amount: p.intent.amount,
            fee_amount: p.intent.fee,
            payer_sig: p.payer_sig,
            payer_nonce: p.intent.nonce,
            expires_at: p.intent.expires_at,
        }
    }
}

// ---------------- 依赖注入 trait ----------------

/// 批次签名提供者。清算行多签管理员对 batch 签名的入口。
///
/// `KeystoreBatchSigner` 用 `settlement::keystore` 里存的清算行管理员私钥;
/// 未接入时用 `NoopBatchSigner` 占位。
pub(crate) trait BatchSigner: Send + Sync {
    /// 对 `message = signing_message(OP_SIGN_OFFCHAIN_BATCH, actor_cid_number || institution_account_id || batch_seq || batch.encode())`
    /// 做 sr25519 签名,返回 64 字节签名(见 `batch_signing_hash`)。
    fn sign_batch(&self, message: &[u8]) -> Result<[u8; 64], String>;
}

/// Extrinsic 提交器。负责把 `(actor_cid_number, actor_role_code, institution_account_id, batch_seq, batch_bytes, sig)` 转成
/// `offchain::Call::submit_offchain_batch` extrinsic 并提
/// 交到节点的 `TransactionPool`。
///
/// `PoolBatchSubmitter` 接 substrate client + `TransactionPool`;
/// 未接入时用 `NoopBatchSubmitter` 占位。
pub(crate) trait BatchSubmitter: Send + Sync {
    fn submit(
        &self,
        actor_cid_number: Vec<u8>,
        actor_role_code: Vec<u8>,
        institution_account_id: AccountId32,
        batch_seq: u64,
        batch_bytes: Vec<u8>,
        batch_signature: [u8; 64],
    ) -> Result<H256, String>;
}

/// 未接入真实签名器的占位实现。调用即返回 Err,让 `pack_and_submit` 正常回
/// 滚 pending 并打印 warning。
pub(crate) struct NoopBatchSigner;

impl BatchSigner for NoopBatchSigner {
    fn sign_batch(&self, _message: &[u8]) -> Result<[u8; 64], String> {
        Err("BatchSigner 未接入(Step 2b-ii-α 默认占位)".to_string())
    }
}

/// 未接入真实提交器的占位实现。
pub(crate) struct NoopBatchSubmitter;

impl BatchSubmitter for NoopBatchSubmitter {
    fn submit(
        &self,
        _actor_cid_number: Vec<u8>,
        _actor_role_code: Vec<u8>,
        _institution_account_id: AccountId32,
        _batch_seq: u64,
        _batch_bytes: Vec<u8>,
        _batch_signature: [u8; 64],
    ) -> Result<H256, String> {
        Err("BatchSubmitter 未接入(Step 2b-ii-α 默认占位)".to_string())
    }
}

// ---------------- Packer 本体 ----------------

/// 清算行批次打包器。
pub(crate) struct OffchainPacker {
    ledger: Arc<OffchainLedger>,
    actor_cid_number: Vec<u8>,
    actor_role_code: Vec<u8>,
    institution_account_id: AccountId32,
    signer: Arc<dyn BatchSigner>,
    submitter: Arc<dyn BatchSubmitter>,
    /// 距上次打包的区块号(链上当前高度)。
    last_pack_block: Arc<RwLock<u64>>,
    /// 清算行本地 batch_seq 递增计数,启动时从链上 `LastClearingBatchSeq` 续跑。
    batch_seq_counter: Arc<RwLock<u64>>,
}

impl OffchainPacker {
    /// 测试专用便捷构造:batch_seq 从 0 起。
    ///
    /// 生产路径必须走 [`Self::new_with_initial_seq`],用链上 `LastClearingBatchSeq` 续跑;
    /// 从 0 起会打断 batch_seq 连续性,故这里用 `cfg(test)` 挡住生产误用。
    #[cfg(test)]
    pub fn new(
        ledger: Arc<OffchainLedger>,
        actor_cid_number: Vec<u8>,
        actor_role_code: Vec<u8>,
        institution_account_id: AccountId32,
        signer: Arc<dyn BatchSigner>,
        submitter: Arc<dyn BatchSubmitter>,
    ) -> Self {
        Self::new_with_initial_seq(
            ledger,
            actor_cid_number,
            actor_role_code,
            institution_account_id,
            signer,
            submitter,
            0,
        )
    }

    /// 构造 packer,并指定链上已成功落账的最新 batch_seq。
    pub fn new_with_initial_seq(
        ledger: Arc<OffchainLedger>,
        actor_cid_number: Vec<u8>,
        actor_role_code: Vec<u8>,
        institution_account_id: AccountId32,
        signer: Arc<dyn BatchSigner>,
        submitter: Arc<dyn BatchSubmitter>,
        initial_batch_seq: u64,
    ) -> Self {
        Self {
            ledger,
            actor_cid_number,
            actor_role_code,
            institution_account_id,
            signer,
            submitter,
            last_pack_block: Arc::new(RwLock::new(0)),
            batch_seq_counter: Arc::new(RwLock::new(initial_batch_seq)),
        }
    }

    /// 是否到了打包时机。
    pub async fn should_pack(&self, current_block: u64) -> bool {
        let count = self.ledger.pending_count();
        if count == 0 {
            return false;
        }
        if count >= PACK_TX_THRESHOLD {
            return true;
        }
        let last = *self.last_pack_block.read().await;
        current_block.saturating_sub(last) >= PACK_BLOCK_THRESHOLD
    }

    /// 打包并提交。调用顺序:
    /// 1. `ledger.take_pending_for_batch(cap)` 拿 pending 切片
    /// 2. 转 链端 `OffchainBatchItem`
    /// 3. `batch_seq` 递增
    /// 4. 组 batch_message → `signer.sign_batch` 得到 batch_signature
    /// 5. 对 batch 做 SCALE 编码 → `submitter.submit` 提交 extrinsic
    /// 6. 失败 → `ledger.reject_pending(tx_id)` 逐个回滚
    /// 7. 成功 → 记录 `last_pack_block`,返回 `tx_hash`
    ///
    /// 注意:成功路径下 pending **不会**立即从 ledger 移除——移除由
    /// `settlement::listener` 收到链上事件时完成。
    pub async fn pack_and_submit(&self, current_block: u64) -> Result<Option<H256>, String> {
        let pending = self.ledger.take_pending_for_batch(PACK_TX_THRESHOLD);
        if pending.is_empty() {
            return Ok(None);
        }

        let batch: Vec<OffchainBatchItem<AccountId32, u32>> =
            pending.into_iter().map(OffchainBatchItem::from).collect();

        let batch_seq = self.next_batch_seq().await;

        let batch_bytes = batch.encode();
        let message = batch_signing_hash(
            &self.actor_cid_number,
            &self.actor_role_code,
            &self.institution_account_id,
            batch_seq,
            &batch_bytes,
        );
        let sig = match self.signer.sign_batch(&message) {
            Ok(s) => s,
            Err(e) => {
                self.rollback(&batch, &format!("sign_batch 失败:{e}"));
                return Err(format!("sign_batch:{e}"));
            }
        };

        match self.submitter.submit(
            self.actor_cid_number.clone(),
            self.actor_role_code.clone(),
            self.institution_account_id.clone(),
            batch_seq,
            batch_bytes,
            sig,
        ) {
            Ok(tx_hash) => {
                let mut last = self.last_pack_block.write().await;
                *last = current_block;
                Ok(Some(tx_hash))
            }
            Err(e) => {
                self.rollback(&batch, &format!("submit 失败:{e}"));
                Err(format!("submit:{e}"))
            }
        }
    }

    /// 本地 batch_seq 单调递增。
    async fn next_batch_seq(&self) -> u64 {
        let mut guard = self.batch_seq_counter.write().await;
        *guard = guard.saturating_add(1);
        *guard
    }

    /// 回滚:把本批次所有 item 重新置回 ledger pending(通过 `reject_pending`)。
    fn rollback(&self, batch: &[OffchainBatchItem<AccountId32, u32>], reason: &str) {
        log::warn!("[Packer] 批次提交失败,回滚 {} 笔:{reason}", batch.len());
        for item in batch {
            if let Err(e) = self.ledger.reject_pending(item.tx_id) {
                log::error!("[Packer] 回滚 tx={:?} 失败:{e}", item.tx_id);
            }
        }
    }
}
#[cfg(test)]
#[path = "../tests/packer.rs"]
mod tests;
