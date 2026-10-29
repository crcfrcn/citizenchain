//! 支付意图与签名数据结构。
//!
//!
//! - `PaymentIntent` 是 L3 用私钥签名的原始数据,citizenapp 本地签,链上验。
//! - 本文件**不引入新的 Storage**,仅提供纯结构与签名哈希函数。
//! - 批次单条 item 结构由本文件提供(清算行体系)。

use codec::{Decode, Encode, MaxEncodedLen};
use primitives::sign::{signing_message, OP_SIGN_L3_PAY, OP_SIGN_OFFCHAIN_BATCH};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_std::vec::Vec;

/// L3 扫码支付意图,citizenapp 本地 sr25519 签名的原始数据。
///
/// 字段顺序**必须与 citizenapp 的 Dart 实现逐字段对齐**,否则签名哈希不一致。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PaymentIntent<AccountId, BlockNumber> {
    /// 全局唯一交易 ID(防重放,与链上 ProcessedOffchainTx 联动)。
    pub tx_id: H256,
    /// 付款方 L3 公钥(L3 的 AccountId,同链上地址)。
    pub payer_account_id: AccountId,
    /// 付款方绑定的清算行 **CID**(机构唯一永久主键;账户由 CID 派生查询)。
    pub payer_bank_cid: crate::InstitutionCidNumber,
    /// 收款方 L3 公钥。
    pub recipient_account_id: AccountId,
    /// 收款方绑定的清算行 **CID**。
    pub recipient_bank_cid: crate::InstitutionCidNumber,
    /// 转账金额(分)。
    pub amount: u128,
    /// 本笔手续费(分),按收款方清算行费率计算。
    pub fee: u128,
    /// L3 的单调递增 nonce,与链上 `L3PaymentNonce` 对应。
    pub nonce: u64,
    /// 签名过期高度(链上 `execute` 时校验 `current_block <= expires_at`)。
    pub expires_at: BlockNumber,
}

impl<AccountId: Encode, BlockNumber: Encode> PaymentIntent<AccountId, BlockNumber> {
    /// 生成签名消息哈希(唯一原语 `signing_message`)。
    ///
    /// `message = blake2_256(GMB || OP_SIGN_L3_PAY || SCALE(intent))`。citizenapp 的
    /// Dart 镜像必须按同一原语拼接,靠金标向量逐字节对齐。
    pub fn signing_hash(&self) -> [u8; 32] {
        signing_message(OP_SIGN_L3_PAY, &self.encode())
    }
}

/// 生成清算行批次签名哈希(唯一原语 `signing_message`)。
///
/// `message = blake2_256(GMB || OP_SIGN_OFFCHAIN_BATCH || SCALE(actor_cid_number)
/// || SCALE(actor_role_code) || SCALE(institution_account_id) || batch_seq_le || SCALE(batch))`。
/// scale_payload 内字段拼接顺序必须与 node 打包器逐字节一致。
///
/// node 侧 `AccountId32.as_ref()` 与 SCALE 编码同为 32 字节,这里使用 `Encode`
/// 是为了让 runtime 维持泛型边界。
pub fn batch_signing_hash<AccountId: Encode>(
    actor_cid_number: &[u8],
    actor_role_code: &[u8],
    institution_account_id: &AccountId,
    batch_seq: u64,
    batch_bytes: &[u8],
) -> [u8; 32] {
    let mut scale_payload = Vec::new();
    scale_payload.extend_from_slice(&actor_cid_number.encode());
    scale_payload.extend_from_slice(&actor_role_code.encode());
    scale_payload.extend_from_slice(&institution_account_id.encode());
    scale_payload.extend_from_slice(&batch_seq.to_le_bytes());
    scale_payload.extend_from_slice(batch_bytes);
    signing_message(OP_SIGN_OFFCHAIN_BATCH, &scale_payload)
}

/// 扫码支付清算体系:批次上链的**单条 item 结构**(清算行体系)。
///
/// `submit_offchain_batch` extrinsic 使用本结构。
///
/// 字段顺序必须与 citizenapp Dart 端的 SCALE 编码逐字段对齐。
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Encode,
    Decode,
    frame_support::pallet_prelude::DecodeWithMemTracking,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct OffchainBatchItem<AccountId, BlockNumber> {
    pub tx_id: H256,
    pub payer_account_id: AccountId,
    /// 付款方绑定的清算行 **CID**(机构唯一永久主键)。
    pub payer_bank_cid: crate::InstitutionCidNumber,
    pub recipient_account_id: AccountId,
    /// 收款方绑定的清算行 **CID**。
    pub recipient_bank_cid: crate::InstitutionCidNumber,
    pub transfer_amount: u128,
    pub fee_amount: u128,
    /// L3 sr25519 签名(64 字节)。
    pub payer_sig: [u8; 64],
    pub payer_nonce: u64,
    pub expires_at: BlockNumber,
}

impl<AccountId: Clone + Encode, BlockNumber: Clone + Encode>
    OffchainBatchItem<AccountId, BlockNumber>
{
    /// 还原为 `PaymentIntent`(用于重算签名哈希以验签)。
    pub fn to_intent(&self) -> PaymentIntent<AccountId, BlockNumber> {
        PaymentIntent {
            tx_id: self.tx_id,
            payer_account_id: self.payer_account_id.clone(),
            payer_bank_cid: self.payer_bank_cid.clone(),
            recipient_account_id: self.recipient_account_id.clone(),
            recipient_bank_cid: self.recipient_bank_cid.clone(),
            amount: self.transfer_amount,
            fee: self.fee_amount,
            nonce: self.payer_nonce,
            expires_at: self.expires_at.clone(),
        }
    }
}
