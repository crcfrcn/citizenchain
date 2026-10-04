//! 私权机构自定义命名账户新增流程(call_index=7)。
//!
//! 与关闭账户(`close.rs`)完全对称:机构不再由注册局直接插入账户,而是由本机构
//! `actor_cid_number + proposer_role_code` 对应的有效岗位任职人创建内部投票提案,
//! 通过后再落库。派生与校验在发起时完成并冻结进提案载荷,通过后 finalizer 重校验落库。

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use codec::Encode;
use sp_runtime::{traits::Zero, DispatchResult};
use votingengine::InternalVoteEngine;

use frame_support::ensure;

use crate::institution::types::{AddInstitutionAccountAction, InstitutionAccountInfo};
use crate::pallet::{
    AccountNameOf, AccountRegisteredCid, CidNumberOf, Config, Error, Event,
    InstitutionAccountNamesOf, InstitutionAccounts, InstitutionPendingAdd, Institutions, Pallet,
    ACTION_ADD_ACCOUNT,
};
use crate::traits::{AccountValidator, ProtectedSourceChecker, ReservedAccountGuard};
use crate::{BalanceOf, RegisteredInstitution, RoleCodeOf};

/// 发起"新增私权机构自定义命名账户"提案。
///
/// 授权改为机构自身:`build_institution_vote_plan` 一次校验管理员名册、有效任职与业务权限
/// (与关闭账户同源、复用 `ACTION_INSTITUTION_CLOSE` 账户生命周期能力,不虚构岗位数据)。
/// 保留登记同款派生+校验链;派生好的账户冻结进提案载荷,不立即插入。
pub(crate) fn do_propose_add_institution_account<T: Config>(
    who: T::AccountId,
    cid_number: CidNumberOf<T>,
    account_names: InstitutionAccountNamesOf<T>,
    proposer_role_code: RoleCodeOf,
) -> DispatchResult {
    ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
    ensure!(!account_names.is_empty(), Error::<T>::MissingMainAccount);

    let info = Institutions::<T>::get(&cid_number).ok_or(Error::<T>::InstitutionNotFound)?;

    // 防重放:同一机构同一时刻只允许一笔进行中的新增账户提案。
    ensure!(
        !InstitutionPendingAdd::<T>::contains_key(&cid_number),
        Error::<T>::AddAlreadyPending
    );

    // 派生 + 校验(与登记同链):保留名/重复/占用/非法/保护。
    let mut derived: Vec<(AccountNameOf<T>, T::AccountId)> =
        Vec::with_capacity(account_names.len());
    let mut seen = BTreeSet::<Vec<u8>>::new();
    for account_name in account_names.iter() {
        ensure!(!account_name.is_empty(), Error::<T>::EmptyAccountName);
        ensure!(
            primitives::account_derive::is_registrable_custom_name(account_name.as_slice()),
            Error::<T>::ReservedAccountName
        );
        ensure!(
            seen.insert(account_name.as_slice().to_vec()),
            Error::<T>::DuplicateAccountName
        );
        ensure!(
            !InstitutionAccounts::<T>::contains_key(&cid_number, account_name),
            Error::<T>::CidAlreadyRegistered
        );
        let (account_id, _kind) = Pallet::<T>::derive_institution_account(
            cid_number.as_slice(),
            account_name.as_slice(),
        )?;
        ensure!(
            !AccountRegisteredCid::<T>::contains_key(&account_id),
            Error::<T>::AccountAlreadyExists
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
        derived.push((account_name.clone(), account_id));
    }

    let action = AddInstitutionAccountAction {
        actor_cid_number: cid_number.clone(),
        derived,
        proposer_account_id: who.clone(),
    };
    let mut data = Vec::from(crate::MODULE_TAG);
    data.push(ACTION_ADD_ACCOUNT);
    data.extend_from_slice(&action.encode());
    // 授权与关闭账户同源:复用 `ACTION_INSTITUTION_CLOSE` 账户生命周期业务能力,
    // 让能提案关闭账户的同一岗位也能提案新增账户,不虚构新岗位/新能力。
    let vote_plan = Pallet::<T>::build_institution_vote_plan(
        &who,
        cid_number.as_slice(),
        proposer_role_code.as_slice(),
        entity_primitives::business_action::ACTION_INSTITUTION_CLOSE,
        &data,
    )?;
    let proposal_id = T::InternalVoteEngine::create_institution_proposal_with_data(
        who.clone(),
        info.institution_code,
        cid_number.to_vec(),
        None,
        alloc::vec![cid_number.to_vec()],
        vote_plan,
        data,
    )?;
    InstitutionPendingAdd::<T>::insert(&cid_number, proposal_id);

    Pallet::<T>::deposit_event(Event::<T>::InstitutionAccountAddProposed {
        proposal_id,
        cid_number,
        proposer_account_id: who,
    });
    Ok(())
}

/// 投票通过回调执行体:与关闭账户 finalizer 同样的重校验后落库派生账户。
pub(crate) fn execute_institution_add_account_with_finalizer<T: Config>(
    proposal_id: u64,
    action: &AddInstitutionAccountAction<T::AccountId, CidNumberOf<T>, AccountNameOf<T>>,
) -> DispatchResult {
    let info =
        Institutions::<T>::get(&action.actor_cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
    let institution_code = info.institution_code;

    let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
        .ok_or(Error::<T>::ProposalActionNotFound)?;
    ensure!(
        votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
            && votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG)
            && proposal.kind == votingengine::PROPOSAL_KIND_INTERNAL
            && proposal.stage == votingengine::STAGE_INTERNAL
            && proposal.status == votingengine::STATUS_PASSED
            && proposal.internal_code == Some(institution_code)
            && proposal.actor_cid_number.as_ref().map(|cid| cid.as_slice())
                == Some(action.actor_cid_number.as_slice())
            && InstitutionPendingAdd::<T>::get(&action.actor_cid_number) == Some(proposal_id),
        Error::<T>::ProposalActionNotFound
    );

    // 逐项重校验状态相关约束(发起与执行之间不得被占用),全部通过后再一次性落库。
    for (account_name, account_id) in action.derived.iter() {
        ensure!(!account_name.is_empty(), Error::<T>::EmptyAccountName);
        ensure!(
            primitives::account_derive::is_registrable_custom_name(account_name.as_slice()),
            Error::<T>::ReservedAccountName
        );
        // 派生确定性:同 (cid, 账户名) 必派生同址,复核冻结载荷未被篡改。
        let (derived_account, _kind) = Pallet::<T>::derive_institution_account(
            action.actor_cid_number.as_slice(),
            account_name.as_slice(),
        )?;
        ensure!(
            &derived_account == account_id,
            Error::<T>::ProposalActionNotFound
        );
        ensure!(
            !InstitutionAccounts::<T>::contains_key(&action.actor_cid_number, account_name),
            Error::<T>::CidAlreadyRegistered
        );
        ensure!(
            !AccountRegisteredCid::<T>::contains_key(account_id),
            Error::<T>::AccountAlreadyExists
        );
        ensure!(
            !T::ReservedAccountChecker::is_reserved(account_id),
            Error::<T>::AccountReserved
        );
        ensure!(
            T::AccountValidator::is_valid(account_id),
            Error::<T>::InvalidAccount
        );
        ensure!(
            !T::ProtectedSourceChecker::is_protected(account_id),
            Error::<T>::ProtectedSource
        );
    }

    let now = <frame_system::Pallet<T>>::block_number();
    for (account_name, account_id) in action.derived.iter() {
        InstitutionAccounts::<T>::insert(
            &action.actor_cid_number,
            account_name,
            InstitutionAccountInfo {
                account_id: account_id.clone(),
                initial_balance: BalanceOf::<T>::zero(),
                created_at: now,
            },
        );
        AccountRegisteredCid::<T>::insert(
            account_id,
            RegisteredInstitution {
                cid_number: action.actor_cid_number.clone(),
                account_name: account_name.clone(),
            },
        );
        Pallet::<T>::deposit_event(Event::<T>::InstitutionAccountAdded {
            cid_number: action.actor_cid_number.clone(),
            account_name: account_name.clone(),
            account_id: account_id.clone(),
            submitter: action.proposer_account_id.clone(),
        });
    }

    InstitutionPendingAdd::<T>::remove(&action.actor_cid_number);
    Ok(())
}
