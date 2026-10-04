//! L3 绑定清算行 + 充值 + 提现 + 切换。
//!
//!
//! - 绑定 = 开户,**无预存、无业务开户费**,签名者支付最低链上交易费 0.1 元/次(由
//!   `configs.rs::RuntimeFeeRouter` 统一返回链上金额路由并由 `OnchainChargeAdapter` 扣费)。
//! - 充值 / 提现走链上资金交易路径,由 L3 自持账户 ↔ 清算行**清算账户**,链上费
//!   按金额 0.1% 最低 0.1 元(沿用既有规则)。
//! - 切换清算行无次数 / 时间间隔限制,**前置:旧清算行余额必须清零**。
//! - 本模块所有扣款/入账都必须过 `institution-asset` 的 `can_spend`,
//!   把"清算账户可被扣"这条规则统一落到资金白名单层。

use frame_support::{
    ensure,
    traits::{Currency, ExistenceRequirement::KeepAlive},
};
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
use sp_runtime::{traits::SaturatedConversion, DispatchError, DispatchResult};

use crate::{
    bank_check, BankTotalDeposits, Config, DepositBalance, Error, Event, Pallet, UserBank,
};

type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// `bind_clearing_bank`:L3 绑定清算行主账户,绑定即开户。
///
/// 约束:
/// - L3 未绑定其他清算行
/// - `bank_main_account` 必须满足 `bank_check::ensure_can_be_bound`(K1=S/F + Active + 主账户)
/// - 无预存,`DepositBalance` 初始化为 0
pub fn do_bind_clearing_bank<T: Config>(
    user: T::AccountId,
    bank_cid: crate::InstitutionCidNumber,
) -> DispatchResult {
    // 1. 不允许重复绑定
    ensure!(
        !UserBank::<T>::contains_key(&user),
        Error::<T>::AlreadyHasBank
    );

    // 2. 清算行合法性(按 CID:K1 主体属性 + 资格 + 已声明节点)
    bank_check::ensure_can_be_bound::<T>(bank_cid.as_slice()).map_err(DispatchError::from)?;

    // 3. 落存储(身份主键=CID)
    UserBank::<T>::insert(&user, &bank_cid);
    DepositBalance::<T>::insert(&bank_cid, &user, 0u128);

    Pallet::<T>::deposit_event(Event::<T>::BankBound { user, bank_cid });
    Ok(())
}

/// `deposit`:L3 自持账户 → 清算行主账户充值。
pub fn do_deposit<T: Config>(user: T::AccountId, amount: u128) -> DispatchResult {
    ensure!(amount > 0, Error::<T>::DepositAmountTooSmall);

    let bank_cid = UserBank::<T>::get(&user).ok_or(Error::<T>::NoOpenedBank)?;
    // 资金落点=CID 派生清算账户(L2 存款准备金池)。
    let bank_clearing = bank_check::clearing_account_of::<T>(bank_cid.as_slice())?;

    // 资金白名单:允许 L3 向清算行清算账户转入(DepositIn 动作,源=充值用户)
    ensure!(
        T::InstitutionAsset::can_spend(&user, InstitutionAssetAction::L3DepositIn),
        Error::<T>::DepositForbidden
    );

    let balance: BalanceOf<T> = amount.saturated_into();
    T::Currency::transfer(&user, &bank_clearing, balance, KeepAlive)?;

    DepositBalance::<T>::mutate(&bank_cid, &user, |b| *b = b.saturating_add(amount));
    BankTotalDeposits::<T>::mutate(&bank_cid, |t| *t = t.saturating_add(amount));

    Pallet::<T>::deposit_event(Event::<T>::Deposited {
        user,
        bank_cid,
        amount,
    });
    Ok(())
}

/// `withdraw`:清算行主账户 → L3 自持账户提现。
pub fn do_withdraw<T: Config>(user: T::AccountId, amount: u128) -> DispatchResult {
    ensure!(amount > 0, Error::<T>::WithdrawAmountTooSmall);

    let bank_cid = UserBank::<T>::get(&user).ok_or(Error::<T>::NoOpenedBank)?;
    let current = DepositBalance::<T>::get(&bank_cid, &user);
    ensure!(current >= amount, Error::<T>::InsufficientDepositBalance);

    // 资金落点=CID 派生清算账户(L2 存款准备金池)。
    let bank_clearing = bank_check::clearing_account_of::<T>(bank_cid.as_slice())?;

    // 资金白名单:清算账户可向外转出(L3 提现,源=清算账户)
    ensure!(
        T::InstitutionAsset::can_spend(&bank_clearing, InstitutionAssetAction::L3WithdrawOut),
        Error::<T>::WithdrawForbidden
    );

    let balance: BalanceOf<T> = amount.saturated_into();
    T::Currency::transfer(&bank_clearing, &user, balance, KeepAlive)
        .map_err(|_| Error::<T>::InsufficientBankLiquidity)?;

    DepositBalance::<T>::mutate(&bank_cid, &user, |b| *b = b.saturating_sub(amount));
    BankTotalDeposits::<T>::mutate(&bank_cid, |t| *t = t.saturating_sub(amount));

    Pallet::<T>::deposit_event(Event::<T>::Withdrawn {
        user,
        bank_cid,
        amount,
    });
    Ok(())
}

/// `switch_bank`:切换清算行。
///
/// 前置条件:
/// - L3 当前已绑定清算行
/// - 旧清算行的 `DepositBalance` 必须为 0(否则要求先 withdraw 清零)
/// - 新清算行 != 旧清算行
/// - 新清算行满足 `bank_check::ensure_can_be_bound`
pub fn do_switch_bank<T: Config>(
    user: T::AccountId,
    new_bank_cid: crate::InstitutionCidNumber,
) -> DispatchResult {
    let old_bank_cid = UserBank::<T>::get(&user).ok_or(Error::<T>::NoOpenedBank)?;
    ensure!(
        old_bank_cid != new_bank_cid,
        Error::<T>::NewBankSameAsCurrent
    );
    ensure!(
        DepositBalance::<T>::get(&old_bank_cid, &user) == 0,
        Error::<T>::MustClearBalanceFirst
    );

    bank_check::ensure_can_be_bound::<T>(new_bank_cid.as_slice()).map_err(DispatchError::from)?;

    // 移除旧绑定下的零余额条目(保持 Storage 干净),换到新清算行
    DepositBalance::<T>::remove(&old_bank_cid, &user);
    UserBank::<T>::insert(&user, &new_bank_cid);
    DepositBalance::<T>::insert(&new_bank_cid, &user, 0u128);

    Pallet::<T>::deposit_event(Event::<T>::BankSwitched {
        user,
        old_bank_cid,
        new_bank_cid,
    });
    Ok(())
}
