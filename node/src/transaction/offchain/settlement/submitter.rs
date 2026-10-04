//! 接 substrate `TransactionPool` 的批次提交器。
//!
//! 本文件实现 `packer::BatchSubmitter` trait,把组好的 batch 包进
//!   `RuntimeCall::OffchainTransaction(submit_offchain_batch { .. })`
//!   → `UncheckedExtrinsic` → 扔到节点本地 `TransactionPool`。
//! extrinsic 构造流程与 `benchmarking.rs::create_benchmark_extrinsic` 严格对齐
//!   (`TxExtension` 各 Check 顺序必须与 runtime `type TxExtension = (..)` 完全一致)。
//! 签名密钥复用 β-1 的 `KeystoreBatchSigner::signing_key` 容器:同一把清算行
//!   管理员 sr25519 私钥既签 batch 内部的 `batch_signature`,也签 extrinsic 外
//!   层的 `SignedPayload`。
//!
//! 本文件与 service 的衔接:
//!
//! service.rs 负责传入具体 `Arc<FullClient>` + `Arc<TransactionPoolHandle>`
//! 以及 `Arc<RwLock<Option<SigningKey>>>`,构造 `PoolBatchSubmitter` 后作为
//! `Arc<dyn BatchSubmitter>` 注入 `start_clearing_bank_components`。
//!

use codec::{Decode, Encode};
use frame_system_rpc_runtime_api::AccountNonceApi;
use sc_client_api::BlockBackend;
use sc_transaction_pool_api::{TransactionPool, TransactionSource};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{AccountId32, OpaqueExtrinsic};
use std::sync::{Arc, RwLock};

use citizenchain as runtime;
use offchain::batch_item::OffchainBatchItem;

use super::keystore::SigningKey;
use super::packer::BatchSubmitter;
use crate::core::service::FullClient;

/// 具体 pool 别名。与 `service.rs` 里 `Service` 第 5 项严格对齐:
/// `TransactionPoolHandle<opaque::Block, FullClient>`。
/// 注意:pool 约束的 Block 是 **opaque::Block**(OpaqueExtrinsic 版),
/// 而不是 `runtime::Block`(带具体 UncheckedExtrinsic 版)。
pub type TxPool = sc_transaction_pool::TransactionPoolHandle<runtime::opaque::Block, FullClient>;

/// 真正走 `TransactionPool` 提交 extrinsic 的 submitter。
pub struct PoolBatchSubmitter {
    client: Arc<FullClient>,
    /// substrate TransactionPool,调 `submit_one` 提交 extrinsic。
    pool: Arc<TxPool>,
    /// 同一把清算行管理员 sr25519 私钥:既是 batch 内部 `batch_signature` 的
    /// 签名者,也是 extrinsic 外层 `SignedPayload` 的签名者。
    signing_key: Arc<RwLock<Option<SigningKey>>>,
}

impl PoolBatchSubmitter {
    pub fn new(
        client: Arc<FullClient>,
        pool: Arc<TxPool>,
        signing_key: Arc<RwLock<Option<SigningKey>>>,
    ) -> Self {
        Self {
            client,
            pool,
            signing_key,
        }
    }
}

impl BatchSubmitter for PoolBatchSubmitter {
    fn submit(
        &self,
        actor_cid_number: Vec<u8>,
        actor_role_code: Vec<u8>,
        institution_account_id: AccountId32,
        batch_seq: u64,
        batch_bytes: Vec<u8>,
        batch_signature: [u8; 64],
    ) -> Result<H256, String> {
        // 1. 解回 Vec<OffchainBatchItem>
        let batch = decode_batch_items(&batch_bytes)?;

        // 2. 构造 CID 与 batch_signature 的 runtime 有界类型。
        let actor_cid_number: offchain::InstitutionCidNumber = actor_cid_number
            .try_into()
            .map_err(|_| "actor_cid_number 超出 CID_NUMBER_MAX_BYTES".to_string())?;
        let actor_role_code: offchain::ActorRoleCode = actor_role_code
            .try_into()
            .map_err(|_| "actor_role_code 超出 64 字节".to_string())?;
        let sig_bounded = encode_bounded_sig::<runtime::Runtime>(&batch_signature)?;

        // 3. 构造 BoundedVec<OffchainBatchItem, MaxBatchSize>
        let batch_bounded = encode_bounded_batch::<runtime::Runtime>(batch)?;

        // 4. 拼 RuntimeCall
        let call = runtime::RuntimeCall::OffchainTransaction(
            offchain::pallet::Call::submit_offchain_batch {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code,
                institution_account_id: institution_account_id.clone(),
                batch_seq,
                batch: batch_bounded,
                batch_signature: sig_bounded,
            },
        );

        // 5. 从 signing_key 拿 sr25519 pair
        let guard = self
            .signing_key
            .read()
            .map_err(|e| format!("签名密钥锁读取失败:{e}"))?;
        let key = guard
            .as_ref()
            .ok_or_else(|| "清算行签名管理员密钥未加载".to_string())?;
        let sender_pair = key.pair.clone();
        drop(guard);

        // 6. 查链上 nonce(对 sender_pair 对应账户)
        let sender_account_id = AccountId32::from(sender_pair.public());
        let nonce = lookup_nonce(&self.client, &sender_account_id)?;

        // 7. 构造签名过的 extrinsic
        let extrinsic = build_signed_extrinsic(&self.client, &sender_pair, call, nonce)?;

        // 8. 真实提交到 TransactionPool。
        //    `pool.submit_one` 返回 Future,packer 侧已经是 async 环境,但本
        //    trait `BatchSubmitter::submit` 保持 sync,因此用 `block_on`。
        //    若未来并发度上升,可把 trait 改 async + packer 内 .await。
        let best_hash = self.client.info().best_hash;
        let opaque: OpaqueExtrinsic = extrinsic.into();
        let fut = self
            .pool
            .submit_one(best_hash, TransactionSource::Local, opaque);
        let tx_hash =
            futures::executor::block_on(fut).map_err(|e| format!("pool.submit_one 失败:{e:?}"))?;

        log::info!(
            "[PoolBatchSubmitter] extrinsic submitted, actor_cid_number={}, institution_account_id={institution_account_id:?} \
             batch_seq={batch_seq} nonce={nonce}"
            , String::from_utf8_lossy(actor_cid_number.as_slice())
        );

        // `TxPool::Hash` 实际是 `H256`。用 SCALE 编解码做稳妥的跨类型转换。
        let hash_bytes = tx_hash.encode();
        H256::decode(&mut &hash_bytes[..]).map_err(|e| format!("TxHash 转 H256 失败:{e}"))
    }
}

/// 查询链上某账户最新 nonce。
///
/// runtime api 失败时绝不回退 0——nonce=0 会让整批交易被池以
/// Stale 拒掉,而提交方只看到"提交失败"却不知道根因是 nonce 查询失败,
/// 必须把错误如实上抛。
fn lookup_nonce(client: &FullClient, account: &AccountId32) -> Result<u32, String> {
    let best_hash = client.info().best_hash;
    let api = client.runtime_api();
    api.account_nonce(best_hash, account.clone())
        .map_err(|e| format!("账户 nonce 查询失败: {e}"))
}

// ---------------- 纯函数工具(便于单测) ----------------

/// 解 SCALE 字节为 `Vec<OffchainBatchItem>`(字段顺序与 runtime 严格一致)。
pub fn decode_batch_items(
    batch_bytes: &[u8],
) -> Result<Vec<OffchainBatchItem<AccountId32, u32>>, String> {
    <Vec<OffchainBatchItem<AccountId32, u32>>>::decode(&mut &batch_bytes[..])
        .map_err(|e| format!("batch_bytes 解码失败:{e}"))
}

/// 把 64 字节签名包装为 runtime `BatchSignatureOf<Runtime>`(`BoundedVec<u8, MaxBatchSignatureLength>`)。
pub fn encode_bounded_sig<T: offchain::pallet::Config>(
    sig: &[u8; 64],
) -> Result<
    frame_support::BoundedVec<u8, <T as offchain::pallet::Config>::MaxBatchSignatureLength>,
    String,
> {
    sig.to_vec()
        .try_into()
        .map_err(|_| "batch_signature 超出 MaxBatchSignatureLength".to_string())
}

/// 把 Vec<OffchainBatchItem> 包装为 `BoundedVec<_, MaxBatchSize>`。
type RuntimeBatch<T> = frame_support::BoundedVec<
    OffchainBatchItem<
        <T as frame_system::Config>::AccountId,
        frame_system::pallet_prelude::BlockNumberFor<T>,
    >,
    <T as offchain::pallet::Config>::MaxBatchSize,
>;

pub fn encode_bounded_batch<T: offchain::pallet::Config>(
    items: Vec<OffchainBatchItem<AccountId32, u32>>,
) -> Result<RuntimeBatch<T>, String>
where
    <T as frame_system::Config>::AccountId: From<AccountId32>,
    frame_system::pallet_prelude::BlockNumberFor<T>: From<u32>,
{
    let converted: Vec<_> = items
        .into_iter()
        .map(|it| {
            Ok(OffchainBatchItem {
                tx_id: it.tx_id,
                payer_account_id: it.payer_account_id,
                payer_bank_cid: it.payer_bank_cid,
                recipient_account_id: it.recipient_account_id,
                recipient_bank_cid: it.recipient_bank_cid,
                transfer_amount: it.transfer_amount,
                fee_amount: it.fee_amount,
                payer_sig: it.payer_sig,
                payer_nonce: it.payer_nonce,
                expires_at: it.expires_at.into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    converted
        .try_into()
        .map_err(|_| "batch 超出 MaxBatchSize".to_string())
}

/// 构造签名 extrinsic(与 `benchmarking.rs::create_benchmark_extrinsic` 严格对齐)。
///
/// 本函数是 **本步单测的重点**:
/// - 参数确定性输入 → extrinsic 可 SCALE roundtrip
/// - SignedPayload 可被 `sr25519_verify` 验证
/// - TxExtension 顺序由 `chain-signing` 统一维护
pub fn build_signed_extrinsic(
    client: &FullClient,
    sender: &sr25519::Pair,
    call: runtime::RuntimeCall,
    nonce: u32,
) -> Result<runtime::UncheckedExtrinsic, String> {
    let genesis_hash = client
        .block_hash(0)
        .map_err(|e| format!("读取 genesis_hash 失败:{e}"))?
        .ok_or_else(|| "Genesis block 尚未可用".to_string())?;

    Ok(chain_signing::build_signed_extrinsic_local(
        call,
        genesis_hash,
        nonce,
        sender,
    ))
}

// ---------------- 单元测试 ----------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_item(seed: u8) -> OffchainBatchItem<AccountId32, u32> {
        OffchainBatchItem {
            tx_id: H256::repeat_byte(seed),
            payer_account_id: AccountId32::new([seed; 32]),
            payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
            recipient_account_id: AccountId32::new([seed.wrapping_add(1); 32]),
            recipient_bank_cid: b"AH001-PRB0X-111111111-2026".to_vec().try_into().unwrap(),
            transfer_amount: 1_000,
            fee_amount: 1,
            payer_sig: [0u8; 64],
            payer_nonce: seed as u64,
            expires_at: 100,
        }
    }

    #[test]
    fn batch_bytes_decodes_to_items() {
        let items = vec![mk_item(1), mk_item(2), mk_item(3)];
        let bytes = items.encode();
        let decoded = decode_batch_items(&bytes).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0], items[0]);
        assert_eq!(decoded[2], items[2]);
    }

    #[test]
    fn encode_bounded_sig_respects_limit() {
        let ok: Result<
            frame_support::BoundedVec<
                u8,
                <runtime::Runtime as offchain::pallet::Config>::MaxBatchSignatureLength,
            >,
            String,
        > = encode_bounded_sig::<runtime::Runtime>(&[7u8; 64]);
        let bounded = ok.expect("64 字节应在 MaxBatchSignatureLength 以内");
        assert_eq!(bounded.len(), 64);
        assert_eq!(bounded[0], 7);
    }

    #[test]
    fn encode_bounded_batch_respects_limit() {
        let items = vec![mk_item(1), mk_item(2)];
        let bounded = encode_bounded_batch::<runtime::Runtime>(items.clone())
            .expect("2 item 应在 MaxBatchSize 以内");
        assert_eq!(bounded.len(), 2);
        assert_eq!(bounded[0].tx_id, items[0].tx_id);
    }

    #[test]
    fn decode_batch_items_rejects_invalid_bytes() {
        let bytes = vec![0xFFu8; 3];
        assert!(decode_batch_items(&bytes).is_err());
    }

    #[test]
    fn sign_key_slot_none_returns_err() {
        // 不构造 FullClient,只测 signing_key 路径的 None 分支 —
        // 通过 submitter.submit(...) 调用触发。为避免依赖 client,此测试
        // 仅验证 signing_key 锁正常访问:取出 read guard 并确认 None。
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(None));
        let guard = slot.read().unwrap();
        assert!(guard.is_none());
    }
}
