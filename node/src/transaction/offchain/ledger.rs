//! 清算行本地 L3 存款缓存账本。
//!
// 账本入口显式接收付款、收款和链状态字段，参数拆组会扩大本次清理的业务改动。
#![allow(clippy::too_many_arguments)]
//!
//! - 权威账本在链上 `offchain::DepositBalance` /
//!   `BankTotalDeposits`。本模块只是**缓存**,用于:
//!     1. citizenapp 查询余额时的快速响应(避免每次落到链上 state 查询)
//!     2. 扫码支付时本地验"可用余额 = confirmed - pending_debit"
//!     3. 从链上事件(`Deposited` / `Withdrawn` / `PaymentSettled`)增量同步
//! - 加密持久化采用 blake2_256 XOR + HMAC 方式(节点启动密码派生 key)。

use codec::{Decode, Encode};
use sp_core::sr25519::{Public as Sr25519Public, Signature as Sr25519Signature};
use sp_core::H256;
use sp_io::crypto::sr25519_verify;
use sp_runtime::AccountId32;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// 节点层 blake2b-256 工具函数。
fn blake2_256(data: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

/// L3 用户在本清算行的账户缓存。
#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct L3AccountState {
    /// 从链上 `DepositBalance` 同步的已确认余额(分)。
    pub confirmed: u128,
    /// 本地已接受未上链的扣款(扫码付出方向)。
    pub pending_debit: u128,
    /// 本地已接受未上链的入账(扫码收方向)。
    pub pending_credit: u128,
    /// 从链上 `L3PaymentNonce` 同步的 nonce(重放防护)。
    pub cached_nonce: u64,
}

impl L3AccountState {
    /// 可用余额 = confirmed - pending_debit。
    pub fn available(&self) -> u128 {
        self.confirmed.saturating_sub(self.pending_debit)
    }
}

/// 一笔本地已接受但未上链的扫码支付。
#[derive(Clone, Debug, Encode, Decode)]
pub struct PendingPayment {
    pub tx_id: H256,
    pub payer: AccountId32,
    /// 付款方绑定清算行 CID(机构唯一永久主键)。
    pub payer_bank_cid: Vec<u8>,
    pub recipient: AccountId32,
    /// 收款方绑定清算行 CID。
    pub recipient_bank_cid: Vec<u8>,
    pub amount: u128,
    pub fee: u128,
    pub nonce: u64,
    pub expires_at: u32,
    /// L3 原始签名(sr25519),上链时作为 batch item 的 payer_sig 附带。
    pub payer_sig: [u8; 64],
    /// 节点接受时的 UNIX 秒时间戳(日志用)。
    pub accepted_at: u64,
}

/// L3 支付意图的**节点层镜像结构**。
///
///
/// - 字段顺序与 runtime 侧 `offchain::batch_item::PaymentIntent`
///   **严格一致**,否则 SCALE 编解码得到的 `signing_hash` 会不匹配,导致链上验签
///   失败。
/// - 之所以节点侧再定义一份,是为了不让 node/Cargo.toml 直接依赖 pallet crate
///   (避免循环与门禁耦合),也便于冷启动不同版本节点间兼容。
/// - 签名消息生成规则(唯一原语):
///   `signing_message(OP_SIGN_L3_PAY, SCALE(self))`
///   = `blake2_256(GMB || OP_SIGN_L3_PAY || SCALE(self))`。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct NodePaymentIntent {
    pub tx_id: H256,
    pub payer: AccountId32,
    /// 付款方绑定清算行 CID(SCALE 与 runtime `InstitutionCidNumber` = BoundedVec<u8> 等价)。
    pub payer_bank_cid: Vec<u8>,
    pub recipient: AccountId32,
    /// 收款方绑定清算行 CID。
    pub recipient_bank_cid: Vec<u8>,
    pub amount: u128,
    pub fee: u128,
    pub nonce: u64,
    pub expires_at: u32,
}

impl NodePaymentIntent {
    /// 生成签名消息哈希,与链上 runtime `PaymentIntent::signing_hash()` 严格一致。
    ///
    /// 统一收敛到 `primitives::sign::signing_message`,域头为 `GMB || OP_SIGN_L3_PAY`。
    pub fn signing_hash(&self) -> [u8; 32] {
        primitives::sign::signing_message(primitives::sign::OP_SIGN_L3_PAY, &self.encode())
    }
}

#[derive(Default)]
pub(super) struct LedgerInner {
    /// L3 账户缓存(key = L3 公钥)。
    pub(super) accounts: HashMap<AccountId32, L3AccountState>,
    /// 已接受未上链的支付列表。
    pub(super) pending: Vec<PendingPayment>,
    /// 已接受的 tx_id(快速防重)。
    pub(super) accepted_tx_ids: HashSet<H256>,
}

/// 清算行本地账本。
#[derive(Clone)]
pub struct OffchainLedger {
    /// 共享内部状态。`pub(super)` 暴露给同目录下的 `packer.rs` 单测用于
    /// 直接注入 pending(绕过 `accept_payment` 的 L3 签名校验)。生产代码应
    /// 走 `accept_payment` / `on_deposited` / `on_withdrawn` / `on_payment_settled`
    /// 等公开接口,不要直接写 `inner`。
    pub(super) inner: Arc<RwLock<LedgerInner>>,
    /// 加密持久化文件路径。
    file_path: PathBuf,
}

impl OffchainLedger {
    /// 创建实例。`base_path` 为节点 base-path,实际文件落在 `base_path/offchain_step1/`。
    pub fn new(base_path: &Path) -> Self {
        let dir = base_path.join("offchain_step1");
        Self {
            inner: Arc::new(RwLock::new(LedgerInner::default())),
            file_path: dir.join("ledger.enc"),
        }
    }

    // ---------------- 查询 ----------------

    /// 查某 L3 的完整账户状态。
    pub fn get_state(&self, user: &AccountId32) -> L3AccountState {
        let ledger = self.inner.read().unwrap_or_else(|e| e.into_inner());
        ledger.accounts.get(user).cloned().unwrap_or_default()
    }

    /// 查可用余额(扫码付款用)。
    pub fn available_balance(&self, user: &AccountId32) -> u128 {
        self.get_state(user).available()
    }

    /// 查下一个应提交的 nonce(citizenapp RPC 调用)。
    ///
    /// 跨行收款方节点不会把付款方写入本地 `accounts`,因此这里必须把
    /// 已接受的 cross-bank pending 也纳入计算,避免连续两笔跨行支付都拿到同一个
    /// `chain_nonce + 1`。
    pub fn next_nonce(&self, user: &AccountId32) -> u64 {
        let ledger = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let account_next = ledger
            .accounts
            .get(user)
            .map(|s| s.cached_nonce.saturating_add(1))
            .unwrap_or(1);
        let pending_next = ledger
            .pending
            .iter()
            .filter(|p| &p.payer == user)
            .map(|p| p.nonce.saturating_add(1))
            .max()
            .unwrap_or(1);
        account_next.max(pending_next)
    }

    /// 待上链笔数。
    pub fn pending_count(&self) -> usize {
        let ledger = self.inner.read().unwrap_or_else(|e| e.into_inner());
        ledger.pending.len()
    }

    /// 本地 `Σ accounts[*].confirmed`(分)。
    ///
    /// 用于 `settlement::reserve` 与链上 `BankTotalDeposits[my_bank]`
    /// 主账对账。pending_debit / pending_credit 不计入:扫码支付在 pending 期间
    /// 链上 `DepositBalance` 和本地 `confirmed` 同时保持"未扣"状态,settlement
    /// 上链后由 listener 同步扣减,两边始终保持相等。
    pub fn confirmed_sum_snapshot(&self) -> u128 {
        let ledger = self.inner.read().unwrap_or_else(|e| e.into_inner());
        ledger
            .accounts
            .values()
            .fold(0u128, |acc, s| acc.saturating_add(s.confirmed))
    }

    // ---------------- 链上事件同步 ----------------

    /// 同步 `Deposited` 事件:L3 充值确认。
    pub fn on_deposited(&self, user: &AccountId32, amount: u128) {
        let mut ledger = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let state = ledger.accounts.entry(user.clone()).or_default();
        state.confirmed = state.confirmed.saturating_add(amount);
    }

    /// 同步 `Withdrawn` 事件:L3 提现确认。
    pub fn on_withdrawn(&self, user: &AccountId32, amount: u128) {
        let mut ledger = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = ledger.accounts.get_mut(user) {
            state.confirmed = state.confirmed.saturating_sub(amount);
        }
    }

    /// 同步 `PaymentSettled` 事件:把 pending 落地到 confirmed。
    ///
    /// 传入 `(payer_bank_cid, recipient_bank_cid)` 与**本清算行** `my_bank_cid`,只对
    /// 属于本行的一侧动账(避免给不在本行的对手方新建 ghost 账户使
    /// `confirmed_sum_snapshot` 与链上 `BankTotalDeposits[my_bank_cid]` 虚高):
    /// - `payer_bank_cid == my_bank_cid`:扣 payer(pending_debit / confirmed)
    /// - `recipient_bank_cid == my_bank_cid`:加 recipient(pending_credit / confirmed)
    /// - 两者皆同(同行):两侧都动
    /// - 两者皆不同(跨行但 my_bank_cid 是第三方清算行):`listener.handle` 不会
    ///   进来调用本方法(上游已过滤);兜底:不动任何账户
    ///
    /// `tx_id` 仍从本地 pending 列表和 `accepted_tx_ids` 中清除(即使本行不是
    /// payer_bank_cid,也可能是 citizenapp 误路由的 accept_payment,留着会导致 pending
    /// 永远不消)。
    pub fn on_payment_settled(
        &self,
        tx_id: H256,
        payer: &AccountId32,
        payer_bank_cid: &[u8],
        recipient: &AccountId32,
        recipient_bank_cid: &[u8],
        my_bank_cid: &[u8],
        amount: u128,
        fee: u128,
    ) {
        let mut ledger = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let total = amount.saturating_add(fee);

        // 付款方属于本行:扣 pending_debit + confirmed
        if payer_bank_cid == my_bank_cid {
            if let Some(state) = ledger.accounts.get_mut(payer) {
                state.pending_debit = state.pending_debit.saturating_sub(total);
                state.confirmed = state.confirmed.saturating_sub(total);
            }
        }
        // 收款方属于本行:清 pending_credit + 加 confirmed(新建或已有)
        if recipient_bank_cid == my_bank_cid {
            if let Some(state) = ledger.accounts.get_mut(recipient) {
                state.pending_credit = state.pending_credit.saturating_sub(amount);
                state.confirmed = state.confirmed.saturating_add(amount);
            } else {
                let s = L3AccountState {
                    confirmed: amount,
                    ..L3AccountState::default()
                };
                ledger.accounts.insert(recipient.clone(), s);
            }
        }
        // 从 pending 列表移除(不论付/收任一侧 accept 时写入的)
        ledger.pending.retain(|p| p.tx_id != tx_id);
        ledger.accepted_tx_ids.remove(&tx_id);
    }

    // ---------------- 持久化 ----------------

    /// 加密持久化到磁盘。
    ///
    /// 当前恢复路径已接好,周期性落盘会在清算行 graceful shutdown / 运维任务中启用。
    #[allow(dead_code)]
    pub fn save_to_disk(&self, password: &str) -> Result<(), String> {
        let ledger = self.inner.read().map_err(|e| format!("账本锁错误:{e}"))?;
        // 简化持久化:只序列化 accounts 和 pending(accepted_tx_ids 可从 pending 重建)
        let accounts_enc = ledger
            .accounts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
            .encode();
        let pending_enc = ledger.pending.encode();

        let a_len = (accounts_enc.len() as u32).to_le_bytes();
        let p_len = (pending_enc.len() as u32).to_le_bytes();
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&a_len);
        plaintext.extend_from_slice(&accounts_enc);
        plaintext.extend_from_slice(&p_len);
        plaintext.extend_from_slice(&pending_enc);

        let key = blake2_256(password.as_bytes());
        let encrypted: Vec<u8> = plaintext
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % 32])
            .collect();
        let tag = blake2_256(&[&key[..], &encrypted].concat());

        let mut data = Vec::new();
        data.extend_from_slice(&encrypted);
        data.extend_from_slice(&tag);

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败:{e}"))?;
        }
        fs::write(&self.file_path, &data).map_err(|e| format!("写入账本失败:{e}"))?;
        Ok(())
    }

    /// 从磁盘解密恢复。
    pub fn load_from_disk(&self, password: &str) -> Result<usize, String> {
        if !self.file_path.exists() {
            return Ok(0);
        }
        let data = fs::read(&self.file_path).map_err(|e| format!("读取账本失败:{e}"))?;
        if data.len() < 32 {
            return Err("账本文件格式错误".to_string());
        }
        let (encrypted, tag) = data.split_at(data.len() - 32);
        let key = blake2_256(password.as_bytes());
        let expected = blake2_256(&[&key[..], encrypted].concat());
        if tag != expected {
            return Err("密码错误或账本损坏".to_string());
        }
        let plaintext: Vec<u8> = encrypted
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % 32])
            .collect();

        if plaintext.len() < 4 {
            return Err("账本数据不完整".to_string());
        }
        let a_len = u32::from_le_bytes(
            plaintext[..4]
                .try_into()
                .map_err(|_| "账本账户区长度字段损坏".to_string())?,
        ) as usize;
        if plaintext.len() < 4 + a_len + 4 {
            return Err("账本数据不完整".to_string());
        }
        let a_data = &plaintext[4..4 + a_len];
        let p_len = u32::from_le_bytes(
            plaintext[4 + a_len..4 + a_len + 4]
                .try_into()
                .map_err(|_| "账本待处理区长度字段损坏".to_string())?,
        ) as usize;
        let p_data = &plaintext[4 + a_len + 4..4 + a_len + 4 + p_len];

        let accounts_vec: Vec<(AccountId32, L3AccountState)> =
            Decode::decode(&mut &a_data[..]).map_err(|e| format!("解码 accounts 失败:{e}"))?;
        let pending: Vec<PendingPayment> =
            Decode::decode(&mut &p_data[..]).map_err(|e| format!("解码 pending 失败:{e}"))?;

        let mut ledger = self.inner.write().map_err(|e| format!("账本锁错误:{e}"))?;
        ledger.accounts = accounts_vec.into_iter().collect();
        ledger.accepted_tx_ids = pending.iter().map(|p| p.tx_id).collect();
        let count = pending.len();
        ledger.pending = pending;
        Ok(count)
    }

    // ---------------- 扫码支付核心业务逻辑 ----------------

    /// 接收 citizenapp 通过 RPC 提交的签名支付意图,执行完整本地校验 + 入账。
    ///
    /// 语义:
    /// 1. sr25519 验签(对 `intent.signing_hash()` 重算,与链上逻辑一致)
    /// 2. tx_id 防重(`accepted_tx_ids` 不命中)
    /// 3. nonce 必须严格等于 `cached_nonce[payer] + 1`
    /// 4. `current_block`(如果提供)必须 `<= intent.expires_at`
    /// 5. 可用余额 `confirmed - pending_debit` 必须 `>= amount + fee`
    /// 6. 成功 → `pending_debit += total`;`cached_nonce = intent.nonce`;
    ///    `pending.push(PendingPayment{..})`;返回 `(tx_id, l2_ack_sig)`。
    ///
    /// [`current_block`] 本地已知最新区块高度。传 `None` 则跳过 `expires_at` 校验。
    ///
    /// [`l2_ack_sig_provider`] 清算行对"我承认这笔意图"的 64 字节签名。
    /// [`accepted_at`] RPC 层生成 ACK 前确定的 UNIX 秒时间戳,本地 pending 与响应共用。
    /// 直接验签入账入口,当前生产路径走链上批次提交。
    #[allow(dead_code)]
    pub fn accept_payment(
        &self,
        intent: NodePaymentIntent,
        payer_sig: [u8; 64],
        current_block: Option<u32>,
        l2_ack_sig_provider: [u8; 64],
        accepted_at: u64,
    ) -> Result<(H256, [u8; 64]), String> {
        let my_bank = intent.payer_bank_cid.clone();
        self.accept_payment_with_chain_state(
            intent,
            payer_sig,
            current_block,
            l2_ack_sig_provider,
            accepted_at,
            &my_bank,
            None,
            None,
        )
    }

    /// 接收支付意图并允许 RPC 层注入链上余额 / nonce。
    ///
    ///
    /// - 本行付款(`intent.payer_bank_cid == my_bank_cid`)继续走本地 `accounts` 缓存,保持
    ///   已有充值/提现/同行扫码的行为。
    /// - 跨行收款(`intent.recipient_bank_cid == my_bank_cid && intent.payer_bank_cid != my_bank_cid`)
    ///   不创建付款方本地账户,而是使用链上 `DepositBalance[payer_bank_cid][payer]`
    ///   与 `L3PaymentNonce[payer]`,再叠加本节点已接受未上链 pending 做早拒。
    /// - 这样收款方清算节点可以主导跨行批次,同时不会污染本地主账对账快照。
    pub fn accept_payment_with_chain_state(
        &self,
        intent: NodePaymentIntent,
        payer_sig: [u8; 64],
        current_block: Option<u32>,
        l2_ack_sig_provider: [u8; 64],
        accepted_at: u64,
        my_bank: &[u8],
        chain_confirmed: Option<u128>,
        chain_nonce: Option<u64>,
    ) -> Result<(H256, [u8; 64]), String> {
        // 1. sr25519 验签
        let msg = intent.signing_hash();
        let pub_bytes: [u8; 32] = intent.payer.clone().into();
        let public = Sr25519Public::from_raw(pub_bytes);
        let signature = Sr25519Signature::from_raw(payer_sig);
        if !sr25519_verify(&signature, &msg, &public) {
            return Err("L3 sr25519 签名验证失败".to_string());
        }

        // 2. expires_at(仅当提供 current_block 时校验)
        if let Some(now_block) = current_block {
            if now_block > intent.expires_at {
                return Err(format!(
                    "支付意图已过期:current={now_block}, expires_at={}",
                    intent.expires_at
                ));
            }
        }

        // 3~6 写锁内一并处理(保证事务性)
        let mut ledger = self.inner.write().map_err(|e| format!("账本锁错误:{e}"))?;

        if ledger.accepted_tx_ids.contains(&intent.tx_id) {
            return Err("tx_id 已被本地接受过,拒绝重复".to_string());
        }

        let total_debit = intent.amount.saturating_add(intent.fee);
        if intent.payer_bank_cid.as_slice() == my_bank {
            let state = ledger.accounts.entry(intent.payer.clone()).or_default();
            let expected_nonce = state.cached_nonce.saturating_add(1);
            if intent.nonce != expected_nonce {
                return Err(format!(
                    "L3 nonce 错位:expected={expected_nonce}, actual={}",
                    intent.nonce
                ));
            }

            let available = state.confirmed.saturating_sub(state.pending_debit);
            if available < total_debit {
                return Err(format!(
                    "清算行存款余额不足:需 {total_debit}, 可用 {available}"
                ));
            }

            // 本行付款才更新本地账户缓存;跨行收款方节点不能生成付款方 ghost 账户。
            state.pending_debit = state.pending_debit.saturating_add(total_debit);
            state.cached_nonce = intent.nonce;
        } else if intent.recipient_bank_cid.as_slice() == my_bank {
            let base_nonce = chain_nonce.unwrap_or(0);
            let local_max_nonce = ledger
                .pending
                .iter()
                .filter(|p| p.payer == intent.payer)
                .map(|p| p.nonce)
                .max()
                .unwrap_or(base_nonce);
            let expected_nonce = base_nonce.max(local_max_nonce).saturating_add(1);
            if intent.nonce != expected_nonce {
                return Err(format!(
                    "L3 nonce 错位:expected={expected_nonce}, actual={}",
                    intent.nonce
                ));
            }

            let local_pending_debit = ledger
                .pending
                .iter()
                .filter(|p| p.payer == intent.payer)
                .fold(0u128, |acc, p| {
                    acc.saturating_add(p.amount.saturating_add(p.fee))
                });
            let confirmed = chain_confirmed.unwrap_or(0);
            let available = confirmed.saturating_sub(local_pending_debit);
            if available < total_debit {
                return Err(format!(
                    "清算行存款余额不足:需 {total_debit}, 可用 {available}"
                ));
            }
        } else {
            return Err("支付意图不属于当前清算行节点".to_string());
        }

        ledger.accepted_tx_ids.insert(intent.tx_id);

        ledger.pending.push(PendingPayment {
            tx_id: intent.tx_id,
            payer: intent.payer.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient: intent.recipient.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            amount: intent.amount,
            fee: intent.fee,
            nonce: intent.nonce,
            expires_at: intent.expires_at,
            payer_sig,
            accepted_at,
        });

        Ok((intent.tx_id, l2_ack_sig_provider))
    }

    /// 回滚本地 pending(packer 提交 extrinsic 失败时调用)。
    ///
    /// 只有当回滚的 tx 正好是 payer 的最后一笔(nonce 为最大)时才回滚 nonce,
    /// 否则仅做 `pending_debit` 和列表剔除,避免破坏其他未提交笔的 nonce 链。
    pub fn reject_pending(&self, tx_id: H256) -> Result<(), String> {
        let mut ledger = self.inner.write().map_err(|e| format!("账本锁错误:{e}"))?;
        let Some(pos) = ledger.pending.iter().position(|p| p.tx_id == tx_id) else {
            return Err("未找到对应 pending".to_string());
        };
        let p = ledger.pending.remove(pos);
        let total = p.amount.saturating_add(p.fee);
        if let Some(state) = ledger.accounts.get_mut(&p.payer) {
            let had_local_debit = total > 0 && state.pending_debit >= total;
            if had_local_debit {
                state.pending_debit = state.pending_debit.saturating_sub(total);
            }
            if had_local_debit && state.cached_nonce == p.nonce {
                state.cached_nonce = state.cached_nonce.saturating_sub(1);
            }
        }
        ledger.accepted_tx_ids.remove(&tx_id);
        Ok(())
    }

    /// 取出至多 `max_items` 笔 pending,用于 packer 组批次。
    /// 按 `accepted_at` 升序,保证上链顺序可预测。
    pub fn take_pending_for_batch(&self, max_items: usize) -> Vec<PendingPayment> {
        let ledger = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let mut items = ledger.pending.to_vec();
        items.sort_by_key(|p| p.accepted_at);
        items.into_iter().take(max_items).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::{sr25519, Pair};

    fn acc(b: u8) -> AccountId32 {
        AccountId32::new([b; 32])
    }

    fn signed_intent(
        pair: &sr25519::Pair,
        tx_id: u8,
        payer_bank_cid: Vec<u8>,
        recipient_bank_cid: Vec<u8>,
        nonce: u64,
    ) -> (NodePaymentIntent, [u8; 64]) {
        let intent = NodePaymentIntent {
            tx_id: H256::repeat_byte(tx_id),
            payer: AccountId32::new(pair.public().0),
            payer_bank_cid,
            recipient: acc(0x22),
            recipient_bank_cid,
            amount: 99,
            fee: 1,
            nonce,
            expires_at: 100,
        };
        let sig = <sr25519::Pair as Pair>::sign(pair, &intent.signing_hash());
        (intent, sig.0)
    }

    #[test]
    fn accept_payment_rejects_signature_from_another_operation_tag() {
        let tmp = std::env::temp_dir().join("offchain_ledger_cross_operation_signature_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let pair = <sr25519::Pair as Pair>::from_seed(&[0x41u8; 32]);
        let payer = AccountId32::new(pair.public().0);
        let bank_cid = b"GD001-PRB0T-239565809-2026".to_vec();
        ledger.on_deposited(&payer, 1_000);
        let (intent, _) = signed_intent(&pair, 0x10, bank_cid.clone(), bank_cid, 1);
        let wrong_message = primitives::sign::signing_message(
            primitives::sign::OP_SIGN_OFFCHAIN_BATCH,
            &intent.encode(),
        );
        let wrong_signature = <sr25519::Pair as Pair>::sign(&pair, &wrong_message).0;

        let error = ledger
            .accept_payment(intent, wrong_signature, Some(1), [7u8; 64], 1_717_000_000)
            .expect_err("其它操作码签名不能重放为 L3 支付授权");
        assert!(error.contains("L3 sr25519 签名验证失败"));
        assert_eq!(ledger.pending_count(), 0);
        assert_eq!(ledger.next_nonce(&payer), 1);
        assert_eq!(ledger.available_balance(&payer), 1_000);
    }

    #[test]
    fn deposited_then_withdrawn_roundtrip() {
        let tmp = std::env::temp_dir().join("offchain_ledger_roundtrip_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);

        ledger.on_deposited(&acc(1), 1000);
        assert_eq!(ledger.available_balance(&acc(1)), 1000);
        ledger.on_withdrawn(&acc(1), 200);
        assert_eq!(ledger.available_balance(&acc(1)), 800);
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = std::env::temp_dir().join("offchain_ledger_saveload_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        ledger.on_deposited(&acc(7), 500);
        ledger.save_to_disk("passwd").unwrap();

        let ledger2 = OffchainLedger::new(&tmp);
        ledger2.load_from_disk("passwd").unwrap();
        assert_eq!(ledger2.available_balance(&acc(7)), 500);
    }

    #[test]
    fn wrong_password_rejected() {
        let tmp = std::env::temp_dir().join("offchain_ledger_password_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        ledger.on_deposited(&acc(9), 123);
        ledger.save_to_disk("right").unwrap();

        let ledger2 = OffchainLedger::new(&tmp);
        assert!(ledger2.load_from_disk("wrong").is_err());
    }

    #[test]
    fn settled_same_bank_moves_pending_to_confirmed() {
        let tmp = std::env::temp_dir().join("offchain_ledger_settled_same_bank_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        ledger.on_deposited(&acc(1), 1000);
        // 模拟本地扣款 pending_debit
        {
            let mut inner = ledger.inner.write().unwrap();
            let st = inner.accounts.entry(acc(1)).or_default();
            st.pending_debit = 100;
            // 模拟 B 的状态不存在,settled 时会创建(同行)
        }
        ledger.on_payment_settled(
            H256::repeat_byte(1),
            &acc(1),
            &my_bank,
            &acc(2),
            &my_bank,
            &my_bank,
            99,
            1,
        );
        // A:confirmed 1000 - 100 = 900,pending_debit 归 0
        assert_eq!(ledger.available_balance(&acc(1)), 900);
        // B:confirmed 99
        assert_eq!(ledger.available_balance(&acc(2)), 99);
    }

    #[test]
    fn settled_cross_bank_payer_side_only_no_ghost_recipient() {
        // 跨行场景:my_bank == payer_bank != recipient_bank。
        // 修复前:recipient 在本地 accounts 会被误建 ghost 账户 confirmed=amount。
        // 修复后:只动 payer,recipient 不出现在本地 accounts。
        let tmp = std::env::temp_dir().join("offchain_ledger_settled_cross_payer_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
        ledger.on_deposited(&acc(1), 1000);
        {
            let mut inner = ledger.inner.write().unwrap();
            let st = inner.accounts.entry(acc(1)).or_default();
            st.pending_debit = 100;
        }
        ledger.on_payment_settled(
            H256::repeat_byte(2),
            &acc(1),
            &my_bank,
            &acc(2),
            &other_bank, // 收款方在别行
            &my_bank,
            99,
            1,
        );
        // A:正常扣减
        assert_eq!(ledger.available_balance(&acc(1)), 900);
        // B 不应有 ghost 账户
        let inner = ledger.inner.read().unwrap();
        assert!(
            !inner.accounts.contains_key(&acc(2)),
            "recipient 不在本行,不应被创建 ghost 账户"
        );
    }

    #[test]
    fn settled_cross_bank_recipient_side_only() {
        // 跨行场景:my_bank == recipient_bank != payer_bank。
        // 只动 recipient,payer 不在本行不动。
        let tmp = std::env::temp_dir().join("offchain_ledger_settled_cross_recipient_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
        // 付款方 A 在 other_bank,但 A 在本行 ledger 里也有历史状态(不该被错扣)
        ledger.on_deposited(&acc(1), 500); // 本行里 A 的旧余额(比如用户曾经绑过我们)
        ledger.on_payment_settled(
            H256::repeat_byte(3),
            &acc(1),
            &other_bank, // 付款方在别行
            &acc(2),
            &my_bank,
            &my_bank,
            99,
            1,
        );
        // A 在本行的 confirmed 不动(非本行 payer)
        assert_eq!(ledger.available_balance(&acc(1)), 500);
        // B 新建 + 加 99
        assert_eq!(ledger.available_balance(&acc(2)), 99);
    }

    #[test]
    fn settled_sum_invariant_same_bank_drops_by_fee() {
        // 同行场景对账不变式:settle 后本地 `Σ confirmed` 变化 = -fee。
        let tmp = std::env::temp_dir().join("offchain_ledger_sum_invariant_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        ledger.on_deposited(&acc(1), 1000);
        {
            let mut inner = ledger.inner.write().unwrap();
            inner.accounts.entry(acc(1)).or_default().pending_debit = 100;
        }
        let before = ledger.confirmed_sum_snapshot();
        ledger.on_payment_settled(
            H256::repeat_byte(4),
            &acc(1),
            &my_bank,
            &acc(2),
            &my_bank,
            &my_bank,
            99, // amount
            1,  // fee
        );
        let after = ledger.confirmed_sum_snapshot();
        assert_eq!(after, before.saturating_sub(1), "Σ confirmed 应减 fee=1");
    }

    #[test]
    fn accept_cross_bank_recipient_uses_chain_balance_without_ghost_payer() {
        // 跨行扫码由收款方清算节点接受 pending:付款方余额与 nonce 来自链上,
        // 本地 ledger 不应该创建付款方账户,否则 reserve 对账会被污染。
        let tmp = std::env::temp_dir().join("offchain_ledger_accept_cross_recipient_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let pair = <sr25519::Pair as Pair>::from_seed(&[0x42u8; 32]);
        let payer = AccountId32::new(pair.public().0);
        let payer_bank: Vec<u8> = b"GD001-PRB0T-239565809-2026".to_vec();
        let recipient_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();

        let (intent1, sig1) =
            signed_intent(&pair, 0x11, payer_bank.clone(), recipient_bank.clone(), 1);
        let (tx1, _) = ledger
            .accept_payment_with_chain_state(
                intent1,
                sig1,
                Some(1),
                [7u8; 64],
                1_717_000_000,
                &recipient_bank,
                Some(1_000),
                Some(0),
            )
            .unwrap();

        assert_eq!(tx1, H256::repeat_byte(0x11));
        assert_eq!(ledger.pending_count(), 1);
        assert_eq!(ledger.next_nonce(&payer), 2);
        let inner = ledger.inner.read().unwrap();
        assert!(
            !inner.accounts.contains_key(&payer),
            "跨行收款方节点不能创建付款方 ghost 账户"
        );
    }

    #[test]
    fn accept_cross_bank_recipient_nonce_counts_local_pending() {
        // 链上 nonce 尚未更新前,第二笔跨行 pending 应接在第一笔后面。
        let tmp = std::env::temp_dir().join("offchain_ledger_accept_cross_nonce_test");
        let _ = fs::remove_dir_all(&tmp);
        let ledger = OffchainLedger::new(&tmp);
        let pair = <sr25519::Pair as Pair>::from_seed(&[0x43u8; 32]);
        let payer_bank: Vec<u8> = b"GD001-PRB0T-239565809-2026".to_vec();
        let recipient_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();

        let (intent1, sig1) =
            signed_intent(&pair, 0x21, payer_bank.clone(), recipient_bank.clone(), 1);
        ledger
            .accept_payment_with_chain_state(
                intent1,
                sig1,
                Some(1),
                [7u8; 64],
                1_717_000_000,
                &recipient_bank,
                Some(1_000),
                Some(0),
            )
            .unwrap();

        let (intent2, sig2) =
            signed_intent(&pair, 0x22, payer_bank.clone(), recipient_bank.clone(), 2);
        ledger
            .accept_payment_with_chain_state(
                intent2,
                sig2,
                Some(1),
                [8u8; 64],
                1_717_000_001,
                &recipient_bank,
                Some(1_000),
                Some(0),
            )
            .unwrap();
        assert_eq!(ledger.pending_count(), 2);

        let (intent_bad, sig_bad) =
            signed_intent(&pair, 0x23, payer_bank, recipient_bank.clone(), 2);
        let err = ledger
            .accept_payment_with_chain_state(
                intent_bad,
                sig_bad,
                Some(1),
                [9u8; 64],
                1_717_000_002,
                &recipient_bank,
                Some(1_000),
                Some(0),
            )
            .unwrap_err();
        assert!(
            err.contains("L3 nonce 错位"),
            "重复 nonce 应被本地 pending 早拒:{err}"
        );
    }
}
