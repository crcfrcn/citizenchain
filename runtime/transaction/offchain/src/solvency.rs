//! 清算行偿付能力自动保护。
//!
//!
//! - 不变式:`L2 清算账户链上 Balances 余额 >= BankTotalDeposits[L2]`
//!   这个不变式保证清算行随时可兑付所有 L3 存款(全额准备金)。
//! - 任何会让 `BankTotalDeposits` 增加的动作执行前,先校验偿付充足;
//!   任何会让清算账户余额减少的动作(手续费扣款 / 提现)同样校验。
//! - 偿付校验嵌在 `settlement::execute_clearing_bank_batch`
//!   和 `deposit::do_withdraw`/`do_deposit` 的路径上,不足时直接 Err
//!   `SolvencyProtected`,链上**自动拒绝**,无需省储行手动干预。

use frame_support::{ensure, traits::Currency};
use sp_runtime::traits::SaturatedConversion;

use crate::{BankTotalDeposits, Config, Error};

/// 校验清算行清算账户在执行**一笔扣减**后仍保持偿付充足。
///
/// [`bank_cid`] 清算行 CID;由其派生清算账户(L2 存款准备金池)
/// [`debit_fen`] 即将从清算账户扣除的分(跨行时是本金+fee;同行时是 fee 部分)
///
/// 语义:
/// - 读清算账户当前链上余额 `onchain`
/// - 读本地总存款快照 `total_deposits`
/// - 要求:`onchain - debit >= total_deposits`(扣款不能跌破总存款)
///
/// 不满足 → `Error::SolvencyProtected`,pallet 拒绝交易。
pub fn ensure_can_debit<T: Config>(
    bank_cid: &crate::InstitutionCidNumber,
    debit_fen: u128,
) -> Result<(), Error<T>> {
    // 身份=CID;资金落点=CID 派生清算账户;账本按 CID 键。
    let bank_clearing = crate::bank_check::clearing_account_of::<T>(bank_cid.as_slice())?;
    let onchain_balance: u128 =
        <T::Currency as Currency<T::AccountId>>::free_balance(&bank_clearing).saturated_into();
    let total_deposits = BankTotalDeposits::<T>::get(bank_cid);

    // 扣减后的主账户余额
    let after_debit = onchain_balance
        .checked_sub(debit_fen)
        .ok_or(Error::<T>::InsufficientBankLiquidity)?;

    // 必须仍然 >= 总存款
    ensure!(after_debit >= total_deposits, Error::<T>::SolvencyProtected);
    Ok(())
}

/// 返回当前清算行偿付率(万分之),用于事件和监控。
/// 例如:102% 返回 10200。总存款为 0 时返回 `u32::MAX`(无限富余)。
pub fn solvency_ratio_bp<T: Config>(bank_cid: &crate::InstitutionCidNumber) -> u32 {
    let Ok(bank_clearing) = crate::bank_check::clearing_account_of::<T>(bank_cid.as_slice()) else {
        return 0;
    };
    let onchain: u128 =
        <T::Currency as Currency<T::AccountId>>::free_balance(&bank_clearing).saturated_into();
    let total = BankTotalDeposits::<T>::get(bank_cid);
    if total == 0 {
        return u32::MAX;
    }
    let ratio = onchain.saturating_mul(10_000) / total;
    ratio.min(u32::MAX as u128) as u32
}
