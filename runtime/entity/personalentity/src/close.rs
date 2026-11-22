//! 校验个人多签账户并发起关闭的内部投票提案。

extern crate alloc;

use codec::Encode;
use frame_support::{
    ensure,
    traits::{Currency, ReservableCurrency},
};
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
use sp_runtime::{
    traits::{CheckedSub, Zero},
    DispatchResult, SaturatedConversion,
};

use crate::pallet::{Config, Error, Event, Pallet, PendingCloseProposal, PersonalAccounts};
use crate::types::{PersonalCloseAction, PersonalStatus};
use crate::BalanceOf;
use crate::ACTION_CLOSE;
use primitives::multisig::{AccountValidator, ProtectedSourceChecker, ReservedAccountGuard};
use engingcore::InternalVoteEngine;

pub(crate) fn do_propose_close<T: Config>(
    who: T::AccountId,
    account_id: T::AccountId,
    beneficiary_account_id: T::AccountId,
) -> DispatchResult {
    // 仅个人多签可走本入口
    ensure!(
        PersonalAccounts::<T>::contains_key(&account_id),
        Error::<T>::NotPersonalAccount
    );

    ensure!(
        !T::ProtectedSourceChecker::is_protected(&account_id),
        Error::<T>::ProtectedSource
    );
    ensure!(
        T::InstitutionAsset::can_spend(&account_id, InstitutionAssetAction::MultisigCloseExecute,),
        Error::<T>::ProtectedSource
    );
    ensure!(
        beneficiary_account_id != account_id,
        Error::<T>::InvalidBeneficiary
    );
    ensure!(
        !T::ReservedAccountChecker::is_reserved(&beneficiary_account_id),
        Error::<T>::InvalidBeneficiary
    );
    ensure!(
        T::AccountValidator::is_valid(&beneficiary_account_id),
        Error::<T>::InvalidAccount
    );
    ensure!(
        !T::ProtectedSourceChecker::is_protected(&beneficiary_account_id),
        Error::<T>::InvalidBeneficiary
    );

    let account_info =
        PersonalAccounts::<T>::get(&account_id).ok_or(Error::<T>::PersonalNotFound)?;
    ensure!(
        account_info.status == PersonalStatus::Active,
        Error::<T>::PersonalNotActive
    );

    // 个人多签治理账户直接使用个人多签账户地址。
    let institution = account_id.clone();
    let institution_code = engingcore::types::PMUL;
    ensure!(
        Pallet::<T>::is_active_account_admin(institution_code, institution.clone(), &who),
        Error::<T>::PermissionDenied
    );

    ensure!(
        !PendingCloseProposal::<T>::contains_key(&account_id),
        Error::<T>::CloseAlreadyPending
    );

    let all_balance = T::Currency::free_balance(&account_id);
    {
        let balance_u128: u128 = all_balance.saturated_into();
        let fee_u128 = primitives::fee_policy::calculate_onchain_fee(balance_u128);
        let fee: BalanceOf<T> = fee_u128.saturated_into();
        let transfer_amount = all_balance
            .checked_sub(&fee)
            .ok_or(Error::<T>::CloseBalanceBelowMinimum)?;
        let ed = T::Currency::minimum_balance();
        ensure!(transfer_amount >= ed, Error::<T>::CloseBalanceBelowMinimum);
    }
    ensure!(
        T::Currency::reserved_balance(&account_id).is_zero(),
        Error::<T>::ReservedBalanceRemaining
    );

    let action = PersonalCloseAction {
        account_id: account_id.clone(),
        beneficiary_account_id: beneficiary_account_id.clone(),
        proposer_account_id: who.clone(),
    };
    let mut data = alloc::vec::Vec::from(crate::MODULE_TAG);
    data.push(ACTION_CLOSE);
    data.extend_from_slice(&action.encode());
    let proposal_id =
        <T as Config>::InternalVoteEngine::create_personal_lifecycle_proposal_with_data(
            who.clone(),
            institution,
            crate::MODULE_TAG,
            data,
        )?;
    PendingCloseProposal::<T>::insert(&account_id, proposal_id);

    Pallet::<T>::deposit_event(Event::<T>::PersonalCloseProposed {
        proposal_id,
        account_id,
        proposer_account_id: who,
        beneficiary_account_id,
    });

    Ok(())
}
