//! 清算行批次打包器。
//!
//!
//! - 职责:定时从 `OffchainLedger.pending` 取出待上链 `PendingPayment`,
//!   组装为 `NodeBatchItem` 列表(字段与 runtime `OffchainBatchItem` 对齐),
//!   由 `BatchSigner` 出 batch 签名,`BatchSubmitter` 提交 extrinsic。
//! - 触发条件:
//!     - 笔数 ≥ `PACK_TX_THRESHOLD`(10 万)
//!     - 距上次打包 ≥ `PACK_BLOCK_THRESHOLD` 区块
//! - 签名 + 提交路径通过 **两个 trait** 解耦:
//!     - `BatchSigner`:`KeystoreBatchSigner` 用 `settlement::keystore::SigningKey` 签名,
//!       未接入时用 `NoopBatchSigner`。
//!     - `BatchSubmitter`:`PoolBatchSubmitter` 走 substrate `TransactionPool`,
//!       未接入时用 `NoopBatchSubmitter`。
//! - `pack_and_submit` 失败时调用 `ledger.reject_pending` 回滚本地 pending,
//!   确保 `cached_nonce` 链不断。
//! - 成功提交上链后**不立即 settle**;`ledger.on_payment_settled` 由
//!   `settlement::listener` 收到链上 `PaymentSettled` 事件时调用。

use codec::{Decode, Encode};
use sp_core::H256;
use sp_runtime::AccountId32;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::super::ledger::{OffchainLedger, PendingPayment};

/// 打包触发阈值:笔数上限(与链上 `PACK_TX_THRESHOLD` 对齐)。
pub const PACK_TX_THRESHOLD: usize = 100_000;

/// 打包触发阈值:距上次打包区块数(与链上 `PACK_BLOCK_THRESHOLD` 对齐)。
pub const PACK_BLOCK_THRESHOLD: u64 = 10;

// ---------------- 节点批次项(与 runtime 结构对齐) ----------------

/// `NodeBatchItem`:节点层批次项,字节级对齐 runtime 端
/// `offchain::batch_item::OffchainBatchItem`。
///
/// 再定义一份而不直接引用 pallet 类型:
/// - 避免 `node/Cargo.toml` 直接 dep 到 `offchain-transaction` pallet,
///   让 node/offchain 子树可以独立于 runtime 升级节奏编译。
/// - SCALE 编码只看**字段顺序和宽度**,两边逐字段对齐即可跨 crate 互认。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct NodeBatchItem {
    pub tx_id: H256,
    pub payer: AccountId32,
    /// 付款方绑定清算行 CID(SCALE 与 runtime `InstitutionCidNumber` = BoundedVec<u8> 等价)。
    pub payer_bank_cid: Vec<u8>,
    pub recipient: AccountId32,
    /// 收款方绑定清算行 CID。
    pub recipient_bank_cid: Vec<u8>,
    pub transfer_amount: u128,
    pub fee_amount: u128,
    pub payer_sig: [u8; 64],
    pub payer_nonce: u64,
    pub expires_at: u32,
}

impl From<PendingPayment> for NodeBatchItem {
    fn from(p: PendingPayment) -> Self {
        Self {
            tx_id: p.tx_id,
            payer: p.payer,
            payer_bank_cid: p.payer_bank_cid,
            recipient: p.recipient,
            recipient_bank_cid: p.recipient_bank_cid,
            transfer_amount: p.amount,
            fee_amount: p.fee,
            payer_sig: p.payer_sig,
            payer_nonce: p.nonce,
            expires_at: p.expires_at,
        }
    }
}

// ---------------- 依赖注入 trait ----------------

/// 批次签名提供者。清算行多签管理员对 batch 签名的入口。
///
/// `KeystoreBatchSigner` 用 `settlement::keystore` 里存的清算行管理员私钥;
/// 未接入时用 `NoopBatchSigner` 占位。
pub trait BatchSigner: Send + Sync {
    /// 对 `message = signing_message(OP_SIGN_OFFCHAIN_BATCH, actor_cid_number || institution_account_id || batch_seq || batch.encode())`
    /// 做 sr25519 签名,返回 64 字节签名(见 `batch_signing_message`)。
    fn sign_batch(&self, message: &[u8]) -> Result<[u8; 64], String>;
}

/// Extrinsic 提交器。负责把 `(actor_cid_number, actor_role_code, institution_account_id, batch_seq, batch_bytes, sig)` 转成
/// `offchain::Call::submit_offchain_batch` extrinsic 并提
/// 交到节点的 `TransactionPool`。
///
/// `PoolBatchSubmitter` 接 substrate client + `TransactionPool`;
/// 未接入时用 `NoopBatchSubmitter` 占位。
pub trait BatchSubmitter: Send + Sync {
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
pub struct NoopBatchSigner;

impl BatchSigner for NoopBatchSigner {
    fn sign_batch(&self, _message: &[u8]) -> Result<[u8; 64], String> {
        Err("BatchSigner 未接入(Step 2b-ii-α 默认占位)".to_string())
    }
}

/// 未接入真实提交器的占位实现。
pub struct NoopBatchSubmitter;

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
pub struct OffchainPacker {
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
    /// 2. 转 `NodeBatchItem`
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

        let batch: Vec<NodeBatchItem> = pending.into_iter().map(NodeBatchItem::from).collect();

        let batch_seq = self.next_batch_seq().await;

        let batch_bytes = batch.encode();
        let message = batch_signing_message(
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
    fn rollback(&self, batch: &[NodeBatchItem], reason: &str) {
        log::warn!("[Packer] 批次提交失败,回滚 {} 笔:{reason}", batch.len());
        for item in batch {
            if let Err(e) = self.ledger.reject_pending(item.tx_id) {
                log::error!("[Packer] 回滚 tx={:?} 失败:{e}", item.tx_id);
            }
        }
    }
}

// ---------------- 纯函数工具 ----------------

/// 构造清算行批次签名消息(唯一原语):
/// `signing_message(OP_SIGN_OFFCHAIN_BATCH, SCALE(actor_cid_number) || SCALE(actor_role_code)
/// || institution_account_id || batch_seq_le || batch_bytes)`。
///
/// 链上 `submit_offchain_batch` 会校验 batch_signature,必须与本函数产生的消息
/// **逐字节一致**(runtime `batch_signing_hash` 用同一原语 + 同序 scale_payload)。
/// CID 按 SCALE 字节序列编码；账户按 32 字节编码，与 runtime 唯一原语逐字节对齐。
pub fn batch_signing_message(
    actor_cid_number: &[u8],
    actor_role_code: &[u8],
    institution_account_id: &AccountId32,
    batch_seq: u64,
    batch_bytes: &[u8],
) -> [u8; 32] {
    let mut scale_payload = Vec::with_capacity(
        actor_cid_number.len() + actor_role_code.len() + 32 + 8 + batch_bytes.len(),
    );
    scale_payload.extend_from_slice(&actor_cid_number.encode());
    scale_payload.extend_from_slice(&actor_role_code.encode());
    scale_payload.extend_from_slice(institution_account_id.as_ref());
    scale_payload.extend_from_slice(&batch_seq.to_le_bytes());
    scale_payload.extend_from_slice(batch_bytes);
    primitives::sign::signing_message(primitives::sign::OP_SIGN_OFFCHAIN_BATCH, &scale_payload)
}

// ---------------- 单元测试 ----------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::offchain::ledger::PendingPayment;
    use std::fs;
    use std::sync::Mutex;

    fn acc(b: u8) -> AccountId32 {
        AccountId32::new([b; 32])
    }

    fn cid() -> Vec<u8> {
        b"GD001-SCB05-000000001-2026".to_vec()
    }

    fn role_code() -> Vec<u8> {
        b"CLEARING_OPERATOR".to_vec()
    }

    fn mk_ledger() -> Arc<OffchainLedger> {
        let tmp = std::env::temp_dir().join("offchain_packer_test");
        let _ = fs::remove_dir_all(&tmp);
        Arc::new(OffchainLedger::new(&tmp))
    }

    fn seed_pending(ledger: &OffchainLedger, tx_byte: u8, payer: AccountId32) {
        // 直接操纵 ledger.inner 注入一条 pending,避免 accept_payment 的签名校验
        let mut inner = ledger.inner.write().unwrap();
        inner.pending.push(PendingPayment {
            tx_id: H256::repeat_byte(tx_byte),
            payer: payer.clone(),
            payer_bank_cid: b"ZS001-PRB08-233384677-2026".to_vec(),
            recipient: acc(9),
            recipient_bank_cid: b"ZS001-PRB08-233384677-2026".to_vec(),
            amount: 1000,
            fee: 1,
            nonce: 1,
            expires_at: 100,
            payer_sig: [0u8; 64],
            accepted_at: tx_byte as u64,
        });
        inner.accepted_tx_ids.insert(H256::repeat_byte(tx_byte));
        let state = inner.accounts.entry(payer).or_default();
        state.confirmed = 2000;
        state.pending_debit = 1001;
        state.cached_nonce = 1;
    }

    #[allow(clippy::type_complexity)]
    struct MockSubmitter {
        calls: Mutex<Vec<(Vec<u8>, Vec<u8>, AccountId32, u64, Vec<u8>, [u8; 64])>>,
        reply: Mutex<Result<H256, String>>,
    }

    impl BatchSubmitter for MockSubmitter {
        fn submit(
            &self,
            actor_cid_number: Vec<u8>,
            actor_role_code: Vec<u8>,
            institution_account_id: AccountId32,
            batch_seq: u64,
            batch_bytes: Vec<u8>,
            batch_signature: [u8; 64],
        ) -> Result<H256, String> {
            self.calls.lock().unwrap().push((
                actor_cid_number,
                actor_role_code,
                institution_account_id,
                batch_seq,
                batch_bytes,
                batch_signature,
            ));
            let g = self.reply.lock().unwrap();
            g.clone()
        }
    }

    struct MockSigner {
        reply: Result<[u8; 64], String>,
    }

    impl BatchSigner for MockSigner {
        fn sign_batch(&self, _message: &[u8]) -> Result<[u8; 64], String> {
            self.reply.clone()
        }
    }

    #[tokio::test]
    async fn should_pack_is_false_when_empty() {
        let packer = OffchainPacker::new(
            mk_ledger(),
            cid(),
            role_code(),
            acc(0xAA),
            Arc::new(NoopBatchSigner),
            Arc::new(NoopBatchSubmitter),
        );
        assert!(!packer.should_pack(100).await);
    }

    #[tokio::test]
    async fn noop_signer_triggers_rollback() {
        let ledger = mk_ledger();
        seed_pending(&ledger, 1, acc(1));
        let packer = OffchainPacker::new(
            ledger.clone(),
            cid(),
            role_code(),
            acc(0xAA),
            Arc::new(NoopBatchSigner),
            Arc::new(NoopBatchSubmitter),
        );
        let result = packer.pack_and_submit(50).await;
        assert!(result.is_err());
        // rollback 成功后 pending 被清空(因为 reject_pending 会剔除)
        assert_eq!(ledger.pending_count(), 0);
    }

    #[tokio::test]
    async fn happy_path_submits_and_keeps_pending() {
        let ledger = mk_ledger();
        seed_pending(&ledger, 2, acc(2));

        let submitter = Arc::new(MockSubmitter {
            calls: Mutex::new(Vec::new()),
            reply: Mutex::new(Ok(H256::repeat_byte(0xFF))),
        });
        let signer = Arc::new(MockSigner {
            reply: Ok([7u8; 64]),
        });

        let packer = OffchainPacker::new(
            ledger.clone(),
            cid(),
            role_code(),
            acc(0xAA),
            signer.clone(),
            submitter.clone(),
        );

        let result = packer.pack_and_submit(100).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(H256::repeat_byte(0xFF)));

        // pending 没有被 reject,保留等 PaymentSettled 事件清理
        assert_eq!(ledger.pending_count(), 1);
        // submitter 收到正确参数
        let calls = submitter.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, cid());
        assert_eq!(calls[0].1, role_code());
        assert_eq!(calls[0].2, acc(0xAA));
        assert_eq!(calls[0].3, 1);
        assert_eq!(calls[0].5, [7u8; 64]);
    }

    #[tokio::test]
    async fn initial_batch_seq_continues_from_chain_value() {
        let ledger = mk_ledger();
        seed_pending(&ledger, 4, acc(4));
        let submitter = Arc::new(MockSubmitter {
            calls: Mutex::new(Vec::new()),
            reply: Mutex::new(Ok(H256::repeat_byte(0xEE))),
        });
        let signer = Arc::new(MockSigner {
            reply: Ok([8u8; 64]),
        });
        let packer = OffchainPacker::new_with_initial_seq(
            ledger,
            cid(),
            role_code(),
            acc(0xAA),
            signer,
            submitter.clone(),
            41,
        );

        assert_eq!(
            packer.pack_and_submit(100).await.unwrap(),
            Some(H256::repeat_byte(0xEE))
        );
        let calls = submitter.calls.lock().unwrap();
        assert_eq!(calls[0].3, 42);
    }

    #[tokio::test]
    async fn submitter_error_rolls_back() {
        let ledger = mk_ledger();
        seed_pending(&ledger, 3, acc(3));
        let submitter = Arc::new(MockSubmitter {
            calls: Mutex::new(Vec::new()),
            reply: Mutex::new(Err("tx pool 拒绝".to_string())),
        });
        let signer = Arc::new(MockSigner {
            reply: Ok([1u8; 64]),
        });
        let packer = OffchainPacker::new(
            ledger.clone(),
            cid(),
            role_code(),
            acc(0xAA),
            signer,
            submitter,
        );

        let r = packer.pack_and_submit(200).await;
        assert!(r.is_err());
        // 回滚后 pending 被剔除
        assert_eq!(ledger.pending_count(), 0);
    }

    #[test]
    fn batch_signing_message_is_deterministic() {
        let inst = acc(0xAA);
        let bytes = vec![1u8, 2, 3, 4];
        let h1 = batch_signing_message(&cid(), &role_code(), &inst, 42, &bytes);
        let h2 = batch_signing_message(&cid(), &role_code(), &inst, 42, &bytes);
        assert_eq!(h1, h2);

        // 改任意输入都影响哈希
        let h3 = batch_signing_message(&cid(), &role_code(), &inst, 43, &bytes);
        assert_ne!(h1, h3);
        let h4 = batch_signing_message(&cid(), &role_code(), &acc(0xAB), 42, &bytes);
        assert_ne!(h1, h4);
        let h5 = batch_signing_message(
            b"GD001-SCB05-000000002-2026",
            &role_code(),
            &inst,
            42,
            &bytes,
        );
        assert_ne!(h1, h5);
        let h6 = batch_signing_message(&cid(), b"OTHER_ROLE", &inst, 42, &bytes);
        assert_ne!(h1, h6);
    }

    #[test]
    fn node_batch_item_encodes_deterministically() {
        let item = NodeBatchItem {
            tx_id: H256::repeat_byte(5),
            payer: acc(1),
            payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec(),
            recipient: acc(3),
            recipient_bank_cid: b"AH001-PRB0X-111111111-2026".to_vec(),
            transfer_amount: 10_000,
            fee_amount: 5,
            payer_sig: [9u8; 64],
            payer_nonce: 1,
            expires_at: 100,
        };
        let bytes = item.encode();
        // 变长布局:bank 字段为 CID = Compact(len)||bytes(两个 CID 各 26 字节 → 各 27)。
        // 32(tx)+32(payer)+(1+26)+32(recipient)+(1+26)+16+16+64+8+4 = 258。
        assert_eq!(
            bytes.len(),
            32 + 32 + (1 + 26) + 32 + (1 + 26) + 16 + 16 + 64 + 8 + 4
        );

        let decoded: NodeBatchItem = NodeBatchItem::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded, item);
    }
}
