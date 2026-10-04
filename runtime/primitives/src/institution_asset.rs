//! 机构账户资金操作白名单(institution_asset)。
//!
//! 只提供 `InstitutionAssetAction` 枚举和 `InstitutionAsset` trait,供实体生命周期
//! (public/private/personal-manage)、`multisig`、`offchain` 复用;不是 pallet,不含
//! storage 或 extrinsic。实际放行/拒绝规则由 runtime 的 `RuntimeInstitutionAsset` 实现。
//!
//! # Safety
//!
//! 默认 `()` 实现为 **fail-open(全放行)**,仅适用于测试。
//! 生产 runtime 必须配置为 `RuntimeInstitutionAsset`,否则资金白名单层将完全失效。
//! runtime 层应有集成测试锁定 stake/main/fee_account/普通账户 的允许矩阵。

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// 机构账户资金动作枚举。
///
/// 这里只描述“内部动钱”的执行动作,不描述提案、投票、管理员变更等纯治理动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum InstitutionAssetAction {
    /// 注册局创建机构时，从本机构明确资金账户划转非零初始余额。
    InstitutionCreateFunding,
    /// 机构多签转账执行:从 `main_account` 向外部收款地址转账,并扣手续费。
    MultisigTransferExecute,
    /// 多签账户关闭执行:把 `main_account` 的余额整体转出。
    MultisigCloseExecute,
    /// 省储行手续费账户归集:从 `fee_account` 划回机构主账户。
    OffchainFeeSweepExecute,
    /// 国家储委会安全基金转账:从 `SAFETY_FUND_ACCOUNT` 向指定收款地址转账。
    NrcSafetyFundTransfer,
    // ========== 清算行(L2)体系动作 ==========
    /// L3 用户向清算行清算账户充值。source 为 L3 自持账户。
    L3DepositIn,
    /// 清算账户向 L3 自持账户提现。source 为清算账户。
    L3WithdrawOut,
    /// 清算账户在扫码清算时扣款(本金 + 手续费)。source 为清算账户。
    L2ClearingDebit,
}

/// 机构账户资金白名单检查器。
///
/// 该接口只解决“哪些内部执行动作可以从哪些制度账户扣钱”。
/// 外部签名权限、提案投票权限、地址注册权限仍由各自模块负责。
pub trait InstitutionAsset<AccountId> {
    fn can_spend(source: &AccountId, action: InstitutionAssetAction) -> bool;
}

impl<AccountId> InstitutionAsset<AccountId> for () {
    fn can_spend(_source: &AccountId, _action: InstitutionAssetAction) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_asset_allows_all_actions() {
        let account = [7u8; 32];
        for action in [
            InstitutionAssetAction::InstitutionCreateFunding,
            InstitutionAssetAction::MultisigTransferExecute,
            InstitutionAssetAction::MultisigCloseExecute,
            InstitutionAssetAction::OffchainFeeSweepExecute,
            InstitutionAssetAction::NrcSafetyFundTransfer,
            InstitutionAssetAction::L3DepositIn,
            InstitutionAssetAction::L3WithdrawOut,
            InstitutionAssetAction::L2ClearingDebit,
        ] {
            assert!(<() as InstitutionAsset<[u8; 32]>>::can_spend(
                &account, action
            ));
        }
    }
}
