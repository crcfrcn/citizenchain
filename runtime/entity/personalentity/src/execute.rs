//! 投票通过/否决终态回调时的业务执行体。
//!
//! 涵盖:
//! - `execute_create_with_finalizer`: ACTION_CREATE 通过后入金 + 激活 PersonalAccounts
//! - `execute_close_with_finalizer`: ACTION_CLOSE 通过后转出余额 + 删除 PersonalAccounts
//!   + 关闭 admin account_id + 清 PendingCloseProposal
//! - `cleanup_pending_create`: 创建提案被否决/超时/终态失败时清理 reserve

extern crate alloc;

use frame_support::{
    ensure,
    traits::{Currency, ExistenceRequirement, ReservableCurrency},
};
use primitives::fee_policy::OnchainFeeCharger;
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
use sp_runtime::{
    traits::{CheckedSub, Saturating, Zero},
    DispatchResult, SaturatedConversion,
};

use crate::pallet::{
    Config, Error, Event, Pallet, PendingCloseProposal, PendingPersonalCreate, PersonalAccounts,
};
use crate::types::{PersonalCloseAction, PersonalCreateAction, PersonalStatus};
use crate::BalanceOf;
use votingengine::InternalVoteEngine;

/// 执行创建：unreserve + 划转 + 扣手续费 + 激活 PersonalAccounts。
///
/// 资金模型:提案创建时已 reserve(amount + fee),此处先 unreserve 再划转入金 + 扣手续费。
pub(crate) fn execute_create_with_finalizer<T: Config>(
    proposal_id: u64,
    action: &PersonalCreateAction<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 资金释放前先证明通过的是当前个人多签账户的创建提案，不能只信任
    // ProposalData 中可解码出的 action。
    Pallet::<T>::ensure_lifecycle_proposal(
        proposal_id,
        crate::MODULE_TAG,
        action.account_id.clone(),
        votingengine::STATUS_PASSED,
        true,
    )?;
    let fee = action.fee;
    let reserve_total = action.amount.saturating_add(fee);

    let leftover = T::Currency::unreserve(&action.proposer_account_id, reserve_total);
    ensure!(leftover.is_zero(), Error::<T>::ReserveReleaseFailed);

    let charged_fee = T::OnchainFeeCharger::charge(&action.proposer_account_id, action.amount)
        .map_err(|_| Error::<T>::FeeWithdrawFailed)?;
    ensure!(charged_fee == fee, Error::<T>::FeeWithdrawFailed);

    T::Currency::transfer(
        &action.proposer_account_id,
        &action.account_id,
        action.amount,
        ExistenceRequirement::KeepAlive,
    )
    .map_err(|_| Error::<T>::TransferFailed)?;

    let account_id = action.account_id.clone();
    Pallet::<T>::activate_admin_account(proposal_id, account_id.clone())?;
    PersonalAccounts::<T>::mutate(&action.account_id, |maybe_account| {
        if let Some(account_id) = maybe_account {
            account_id.status = PersonalStatus::Active;
        }
    });
    let institution_code = votingengine::types::PMUL;
    let admins_len = Pallet::<T>::active_account_admins_len(institution_code, account_id.clone())
        .ok_or(Error::<T>::PersonalNotFound)?;
    let threshold = <T as Config>::InternalVoteEngine::configured_personal_threshold(
        proposal_id,
        account_id.clone(),
    )
    .ok_or(Error::<T>::PersonalNotFound)?;
    PendingPersonalCreate::<T>::remove(proposal_id);

    Pallet::<T>::deposit_event(Event::<T>::PersonalCreated {
        proposal_id,
        account_id: action.account_id.clone(),
        creator_account_id: action.proposer_account_id.clone(),
        admins_len,
        threshold,
        amount: action.amount,
        fee,
    });

    Ok(())
}

/// 执行关闭：转出余额 + 删除 PersonalAccounts + 关闭 admin account_id。
pub(crate) fn execute_close_with_finalizer<T: Config>(
    proposal_id: u64,
    action: &PersonalCloseAction<T::AccountId>,
) -> DispatchResult {
    Pallet::<T>::ensure_lifecycle_proposal(
        proposal_id,
        crate::MODULE_TAG,
        action.account_id.clone(),
        votingengine::STATUS_PASSED,
        true,
    )?;
    ensure!(
        PendingCloseProposal::<T>::get(&action.account_id) == Some(proposal_id),
        Error::<T>::ProposalActionNotFound
    );
    ensure!(
        T::InstitutionAsset::can_spend(
            &action.account_id,
            InstitutionAssetAction::MultisigCloseExecute,
        ),
        Error::<T>::ProtectedSource
    );
    let account_id = action.account_id.clone();
    let institution_code = votingengine::types::PMUL;
    let admins_len = Pallet::<T>::active_account_admins_len(institution_code, account_id.clone())
        .ok_or(Error::<T>::PersonalNotFound)?;
    let threshold =
        <T as Config>::InternalVoteEngine::active_personal_threshold(account_id.clone())
            .ok_or(Error::<T>::PersonalNotFound)?;
    let all_balance = T::Currency::free_balance(&action.account_id);
    // 注销执行前再次确认没有 reserved 余额，避免提案后新增锁定资金导致销户不彻底。
    ensure!(
        T::Currency::reserved_balance(&action.account_id).is_zero(),
        Error::<T>::ReservedBalanceRemaining
    );

    let balance_u128: u128 = all_balance.saturated_into();
    let fee_u128 = primitives::fee_policy::calculate_onchain_fee(balance_u128);
    let fee: BalanceOf<T> = fee_u128.saturated_into();
    let transfer_amount = all_balance
        .checked_sub(&fee)
        .ok_or(Error::<T>::FeeWithdrawFailed)?;

    let ed = T::Currency::minimum_balance();
    ensure!(transfer_amount >= ed, Error::<T>::CloseTransferBelowED);

    let charged_fee = T::OnchainFeeCharger::charge(&action.account_id, all_balance)
        .map_err(|_| Error::<T>::FeeWithdrawFailed)?;
    ensure!(charged_fee == fee, Error::<T>::FeeWithdrawFailed);

    T::Currency::transfer(
        &action.account_id,
        &action.beneficiary_account_id,
        transfer_amount,
        ExistenceRequirement::AllowDeath,
    )
    .map_err(|_| Error::<T>::TransferFailed)?;

    PersonalAccounts::<T>::remove(&action.account_id);
    Pallet::<T>::close_admin_account(proposal_id, account_id)?;
    PendingCloseProposal::<T>::remove(&action.account_id);

    Pallet::<T>::deposit_event(Event::<T>::PersonalClosed {
        proposal_id,
        account_id: action.account_id.clone(),
        beneficiary_account_id: action.beneficiary_account_id.clone(),
        admins_len,
        threshold,
        amount: transfer_amount,
        fee,
    });

    Ok(())
}

/// 创建提案被否决/超时/终态失败时清理:
/// unreserve(amount + fee) + 删 PersonalAccounts/PendingPersonalCreate +
/// 移除 admin account_id Pending。
///
/// `emit_event = true` 时(否决路径)发 `PersonalCreateRejected`,终态失败路径不发。
pub(crate) fn cleanup_pending_create<T: Config>(
    proposal_id: u64,
    action: &PersonalCreateAction<T::AccountId, BalanceOf<T>>,
    emit_event: bool,
) -> Result<bool, sp_runtime::DispatchError> {
    if !PendingPersonalCreate::<T>::contains_key(proposal_id) {
        return Ok(false);
    }

    Pallet::<T>::remove_pending_admin_account(proposal_id, action.account_id.clone())?;

    let reserve_total = action.amount.saturating_add(action.fee);
    let _ = T::Currency::unreserve(&action.proposer_account_id, reserve_total);

    PersonalAccounts::<T>::remove(&action.account_id);
    PendingPersonalCreate::<T>::remove(proposal_id);

    if emit_event {
        Pallet::<T>::deposit_event(Event::<T>::PersonalCreateRejected {
            proposal_id,
            account_id: action.account_id.clone(),
        });
    }
    Ok(true)
}
