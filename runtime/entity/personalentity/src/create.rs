//! 校验名称、资金和管理员集合，派生个人多签账户并发起创建提案。

extern crate alloc;

use codec::Encode;
use frame_support::{
    ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::{Get, ReservableCurrency},
};
use sp_runtime::DispatchResult;

use crate::pallet::{
    AccountNameOf, AdminsOf, Config, Error, Event, Pallet, PendingPersonalCreate, PersonalAccounts,
};
use crate::types::{PersonalAccount, PersonalCreateAction, PersonalStatus};
use crate::BalanceOf;
use crate::ACTION_CREATE;
use primitives::multisig::{AccountValidator, ProtectedSourceChecker, ReservedAccountGuard};
use engingcore::InternalVoteEngine;

pub(crate) fn do_propose_create<T: Config>(
    who: T::AccountId,
    account_name: AccountNameOf<T>,
    admins: AdminsOf<T>,
    regular_threshold: u32,
    amount: BalanceOf<T>,
) -> DispatchResult {
    let admins = Pallet::<T>::normalize_admins(admins);
    ensure!(
        !T::ProtectedSourceChecker::is_protected(&who),
        Error::<T>::ProtectedSource
    );
    ensure!(!account_name.is_empty(), Error::<T>::EmptyPersonalName);
    ensure!(
        amount >= T::MinCreateAmount::get(),
        Error::<T>::CreateAmountBelowMinimum
    );
    Pallet::<T>::ensure_admin_config(&who, &admins, regular_threshold)?;
    let admins_len = admins.len() as u32;

    let (reserve_total, fee) = Pallet::<T>::ensure_proposer_can_afford(&who, amount)?;

    let account_id = Pallet::<T>::derive_personal_account(&who, account_name.as_slice())?;
    ensure!(
        !PersonalAccounts::<T>::contains_key(&account_id),
        Error::<T>::PersonalAlreadyExists
    );
    ensure!(
        !T::ReservedAccountChecker::is_reserved(&account_id),
        Error::<T>::AccountReserved
    );
    ensure!(
        T::AccountValidator::is_valid(&account_id),
        Error::<T>::InvalidAccount
    );
    ensure!(
        !T::ProtectedSourceChecker::is_protected(&account_id),
        Error::<T>::ProtectedSource
    );

    let now = <frame_system::Pallet<T>>::block_number();
    let institution = account_id.clone();
    let action = PersonalCreateAction {
        account_id: account_id.clone(),
        proposer_account_id: who.clone(),
        amount,
        fee,
    };
    let mut data = alloc::vec::Vec::from(crate::MODULE_TAG);
    data.push(ACTION_CREATE);
    data.extend_from_slice(&action.encode());

    let proposal_id = with_transaction(|| {
        if T::Currency::reserve(&who, reserve_total).is_err() {
            return TransactionOutcome::Rollback(Err(Error::<T>::ReserveFailed.into()));
        }
        PersonalAccounts::<T>::insert(
            &account_id,
            PersonalAccount {
                creator_account_id: who.clone(),
                account_name: account_name.clone(),
                created_at: now,
                status: PersonalStatus::Pending,
            },
        );
        // regular_threshold 是账户激活后的动态阈值配置；
        // 本次注册投票的全员通过阈值由投票引擎根据管理员快照生成。
        let proposal_id = match <T as Config>::InternalVoteEngine::create_personal_account_create_proposal_with_data(
            who.clone(),
            institution.clone(),
            admins
                .iter()
                .map(|admin| admin.account_id.clone())
                .collect(),
            regular_threshold,
            crate::MODULE_TAG,
            data,
        ) {
            Ok(proposal_id) => proposal_id,
            Err(err) => return TransactionOutcome::Rollback(Err(err)),
        };
        PendingPersonalCreate::<T>::insert(proposal_id, &action);
        if let Err(err) = Pallet::<T>::create_pending_admin_account_for_proposal(
            proposal_id,
            institution.clone(),
            adminbase::AdminAccountKind::PersonalMultisig,
            &admins,
            &who,
        ) {
            return TransactionOutcome::Rollback(Err(err));
        }
        TransactionOutcome::Commit(Ok(proposal_id))
    })?;

    let expires_at = engingcore::Pallet::<T>::proposals(proposal_id)
        .map(|p| p.end)
        .ok_or(Error::<T>::VoteEngineError)?;

    Pallet::<T>::deposit_event(Event::<T>::PersonalCreateProposed {
        proposal_id,
        account_id,
        proposer_account_id: who,
        account_name,
        admins,
        admins_len,
        threshold: regular_threshold,
        amount,
        fee,
        expires_at,
    });

    Ok(())
}
