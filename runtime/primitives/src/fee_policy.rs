//! 全链费用协议唯一真源：五类费用路由、费率常量、计算公式与手续费分账。

use sp_runtime::{Debug, Perbill};

/// 链上交易费执行接口。
///
/// 本接口不是第二套费用分类：调用方只能提交已经由业务规则确定的付款账户和
/// 交易金额；具体实现统一使用本文件费率计算、完整扣款并进入链上分账。
pub trait OnchainFeeCharger<AccountId, Balance> {
    /// 从 `payer_account_id` 收取 `transaction_amount` 对应的链上交易费并返回实收金额。
    fn charge(
        payer_account_id: &AccountId,
        transaction_amount: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError>;
}

impl<AccountId, Balance> OnchainFeeCharger<AccountId, Balance> for () {
    fn charge(
        _payer: &AccountId,
        _transaction_amount: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        Err(sp_runtime::DispatchError::Other(
            "OnchainFeeChargerNotConfigured",
        ))
    }
}

/// 链下清算费付款来源。
///
/// 一个批次可包含多个付款公民和多个付款方清算行，不能伪装成单一机构账户付款。
/// 具体每笔 `payer_account_id + fee_amount` 直接以批次 item 为真源，由 offchain 执行器校验并扣账。
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum OffchainFeePayer {
    BatchItemPayers,
}

/// 全链唯一费用路由类型。
///
/// 付款关系直接绑定在三种收费分支中：链上费和投票费携带确切账户，链下批次
/// 携带多付款人来源。类型上不存在可选付款人、默认付款人或“解析失败后改扣
/// 签名者”的回落空间；`Free` 与 `Reject` 也无法携带付款关系。
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum FeeRoute<AccountId, Balance> {
    /// 框架固有、Root 回调或幂等维护调用，不收取交易费。
    Free,
    /// 链上交易费：按交易金额计算，零金额自动落最低 0.1 元。
    Onchain {
        transaction_amount: Balance,
        payer_account_id: AccountId,
    },
    /// 链下交易费：费用由链下清算模块按批次业务规则计算并收取。
    Offchain {
        fee_amount: Balance,
        payer_account_id: OffchainFeePayer,
    },
    /// 实际投票费：管理员或公民投出一票时由签名者固定支付 1 元。
    Vote { payer_account_id: AccountId },
    /// 明确拒绝：未开放、未归类、付款关系或授权关系不成立的调用。
    Reject,
}

/// 交易 tip 协议值：tip 不属于交易费，全链只允许零。
pub const TRANSACTION_TIP: u128 = 0;

// 链上交易费:`max(amount × 0.1%, ONCHAIN_MIN_FEE)`。
pub const ONCHAIN_FEE_RATE: Perbill = Perbill::from_parts(1_000_000);

/// 链上交易单笔最小手续费:10 FEN = 0.1 元。
pub const ONCHAIN_MIN_FEE: u128 = 10;

/// 实际投票统一费用:100 FEN = 1 元/票。
pub const VOTE_FLAT_FEE: u128 = 100;

// 链上交易手续费分账。
/// 铸块全节点分成:80%。
pub const ONCHAIN_FEE_FULLNODE_PERCENT: u32 = 80;

/// 国家储委会账户分成:10%。
pub const ONCHAIN_FEE_NRC_PERCENT: u32 = 10;

/// 安全基金账户分成:10%。
pub const ONCHAIN_FEE_SAFETY_FUND_PERCENT: u32 = 10;

// 链下清算行 L2 手续费。
/// 链下交易单笔最小手续费:1 FEN = 0.01 元。
pub const OFFCHAIN_MIN_FEE: u128 = 1;

// 两类可收费路径的最低费用必须在编译期保持非零，避免被误改为免费。
const _: () = {
    assert!(ONCHAIN_MIN_FEE > 0);
    assert!(OFFCHAIN_MIN_FEE > 0);
};

/// 链下清算行个体费率下限:0.01%。
pub const OFFCHAIN_FEE_RATE_MIN: Perbill = Perbill::from_parts(100_000);

/// 链下清算行个体费率上限:0.1%。
pub const OFFCHAIN_FEE_RATE_MAX: Perbill = Perbill::from_parts(1_000_000);

/// 运营类交易费乘数:1,不额外加价。
pub const OPERATIONAL_FEE_MULTIPLIER: u8 = 1;

/// 按交易金额计算链上手续费，金额和返回值单位均为分。
///
/// 公式：`max(round(amount × ONCHAIN_FEE_RATE), ONCHAIN_MIN_FEE)`。
pub fn calculate_onchain_fee(amount: u128) -> u128 {
    mul_perbill_round(amount, ONCHAIN_FEE_RATE).max(ONCHAIN_MIN_FEE)
}

fn mul_perbill_round(amount: u128, rate: Perbill) -> u128 {
    const PERBILL_DENOMINATOR: u128 = 1_000_000_000;
    let parts = rate.deconstruct() as u128;
    let whole = amount / PERBILL_DENOMINATOR;
    let remainder = amount % PERBILL_DENOMINATOR;

    // 先拆整分量和尾量，避免极大金额直接执行 `amount * parts` 发生溢出失真。
    let whole_component = whole.saturating_mul(parts);
    let remainder_component =
        (remainder * parts).saturating_add(PERBILL_DENOMINATOR / 2) / PERBILL_DENOMINATOR;
    whole_component.saturating_add(remainder_component)
}



#[cfg(test)]
#[path = "../tests/fees.rs"]
mod tests;
