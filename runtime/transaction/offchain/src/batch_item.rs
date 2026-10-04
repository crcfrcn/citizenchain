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

#[cfg(test)]
mod tests {
    use super::*;
    use sp_runtime::AccountId32;

    fn acc(b: u8) -> AccountId32 {
        AccountId32::new([b; 32])
    }

    fn cid(s: &str) -> crate::InstitutionCidNumber {
        s.as_bytes().to_vec().try_into().expect("cid 长度须 <= 32")
    }

    #[test]
    fn signing_hash_is_deterministic() {
        let intent = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::repeat_byte(9),
            payer_account_id: acc(1),
            payer_bank_cid: cid("BANK-A"),
            recipient_account_id: acc(3),
            recipient_bank_cid: cid("BANK-A"),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let h1 = intent.signing_hash();
        let h2 = intent.signing_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn signing_hash_changes_with_any_field() {
        let base = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::repeat_byte(9),
            payer_account_id: acc(1),
            payer_bank_cid: cid("BANK-A"),
            recipient_account_id: acc(3),
            recipient_bank_cid: cid("BANK-A"),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let base_hash = base.signing_hash();
        let assert_changed =
            |field_name: &str, mutate: fn(&mut PaymentIntent<AccountId32, u32>)| {
                let mut changed = base.clone();
                mutate(&mut changed);
                assert_ne!(
                    base_hash,
                    changed.signing_hash(),
                    "{field_name} 必须受签名保护"
                );
            };
        assert_changed("tx_id", |intent| intent.tx_id = H256::repeat_byte(10));
        assert_changed("payer_account_id", |intent| {
            intent.payer_account_id = acc(2)
        });
        assert_changed("payer_bank_cid", |intent| {
            intent.payer_bank_cid = cid("BANK-B")
        });
        assert_changed("recipient_account_id", |intent| {
            intent.recipient_account_id = acc(4)
        });
        assert_changed("recipient_bank_cid", |intent| {
            intent.recipient_bank_cid = cid("BANK-B")
        });
        assert_changed("amount", |intent| intent.amount = 10_001);
        assert_changed("fee", |intent| intent.fee = 6);
        assert_changed("nonce", |intent| intent.nonce = 2);
        assert_changed("expires_at", |intent| intent.expires_at = 101);
    }

    #[test]
    fn signing_hash_is_separated_from_other_operation_tags() {
        let intent = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::repeat_byte(9),
            payer_account_id: acc(1),
            payer_bank_cid: cid("BANK-A"),
            recipient_account_id: acc(3),
            recipient_bank_cid: cid("BANK-A"),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let encoded = intent.encode();
        assert_ne!(
            intent.signing_hash(),
            signing_message(OP_SIGN_OFFCHAIN_BATCH, &encoded),
            "相同 PaymentIntent 载荷在批次操作码下必须生成不同签名消息"
        );
    }

    // ─── golden vectors ────────────────────────────
    //
    // 目的:锁定 `PaymentIntent` 的 SCALE 编码布局 + signing_hash 算法,使
    // citizenapp `payment_intent.dart::NodePaymentIntent` 端的编码/哈希必须逐
    // 字节一致。citizenapp 端 `test/trade/payment_intent_golden_test.dart` 写入
    // **相同的 fixture + 相同的期望 hex**,任一端实现漂移 → 两端 CI 同时红。
    //
    // 布局(变长,payer_bank_cid/recipient_bank_cid = Compact(len)||bytes CID):
    //   tx_id(32) || payer_account_id(32) || payer_bank_cid(变长) || recipient_account_id(32) ||
    //   recipient_bank_cid(变长) || amount(u128 LE,16) || fee(u128 LE,16) ||
    //   nonce(u64 LE,8) || expires_at(u32 LE,4)
    //
    // 签名域 = 4B 域头 `GMB(3B) || OP_SIGN_L3_PAY(0x15)` || encoded
    // → blake2_256 = 32 字节。CID 变长故无固定总长;金标锁 hash 而非长度。

    /// 把 `&[u8]` 转 hex(无 `0x` 前缀,小写),测试断言格式。
    fn hex_lower(bytes: &[u8]) -> sp_std::vec::Vec<u8> {
        let mut out = sp_std::vec::Vec::with_capacity(bytes.len() * 2);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for b in bytes {
            out.push(HEX[(*b >> 4) as usize]);
            out.push(HEX[(*b & 0x0F) as usize]);
        }
        out
    }

    fn assert_hex_eq(label: &str, bytes: &[u8], expected: &str) {
        let got = hex_lower(bytes);
        let got_str = core::str::from_utf8(&got).expect("hex ascii");
        assert_eq!(got_str, expected, "{label} hex mismatch");
    }

    /// Fixture 1:简单同行支付。所有定长字段用重复字节、金额 10_000 分 / 费 5 分
    /// / nonce 1 / expires_at 100。
    #[test]
    fn golden_fixture1_simple_same_bank() {
        // 同行:payer_account_id/recipient_account_id 绑同一清算行 CID。signing_hash 是跨语言字节锁,
        // Dart `payment_intent_golden_test.dart` 用同一 CID fixture + 同一期望 hex。
        let intent = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::zero(),
            payer_account_id: acc(1),
            payer_bank_cid: cid("LN001-NRC0G-944805165-2026"),
            recipient_account_id: acc(3),
            recipient_bank_cid: cid("LN001-NRC0G-944805165-2026"),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        assert_hex_eq(
            "fixture1 signing_hash",
            &intent.signing_hash(),
            "4c0c52528976ee38e101769c27ee57a0e30e18939503271fb12a59b58df886fe",
        );
    }

    /// Fixture 2:跨行 + 大金额 + 大 nonce + 大 expires_at。锁字节序与字段位置。
    #[test]
    fn golden_fixture2_cross_bank_big_values() {
        // 跨行 + 极值:payer_account_id/recipient_account_id 绑不同清算行 CID。
        let intent = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::repeat_byte(0xFF),
            payer_account_id: acc(0x11),
            payer_bank_cid: cid("GD001-SFGF0-201206100-2026"),
            recipient_account_id: acc(0x22),
            recipient_bank_cid: cid("AH001-SFGF0-111111111-2026"),
            amount: u128::MAX,
            fee: u128::MAX,
            nonce: u64::MAX,
            expires_at: u32::MAX,
        };
        assert_hex_eq(
            "fixture2 signing_hash",
            &intent.signing_hash(),
            "38ba8205abb84ec9121b65c3ee618626972710063e8e6c48cec29b1121460e72",
        );
    }

    /// Fixture 3:零金额边界。tx_id 递增字节,其他保守。
    #[test]
    fn golden_fixture3_zero_amount_incrementing_tx() {
        let mut tx_bytes = [0u8; 32];
        for (i, b) in tx_bytes.iter_mut().enumerate() {
            *b = i as u8; // 0x00..0x1F
        }
        let intent = PaymentIntent::<AccountId32, u32> {
            tx_id: H256::from(tx_bytes),
            payer_account_id: acc(0x55),
            payer_bank_cid: cid("BJ001-SFGF0-222222222-2026"),
            recipient_account_id: acc(0x66),
            recipient_bank_cid: cid("BJ001-SFGF0-222222222-2026"),
            amount: 0,
            fee: 0,
            nonce: 0,
            expires_at: 0,
        };
        assert_hex_eq(
            "fixture3 signing_hash",
            &intent.signing_hash(),
            "62405346ffba9e0a4b9d785cf399bfdfcdc1033270dbc8a7b4cbc9ba4e052c9f",
        );
    }
}
