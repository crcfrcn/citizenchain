//! 机构账户表相关 helper。
//!
//! 此模块负责:
//! - `build_required_protocol_accounts`: 根据 CID 制度约束生成强制协议账户。
//! - `validate_initial_accounts`: 校验 runtime 自动生成的账户列表,派生每个账户
//!   的链上地址,返回固化的 `CreateInstitutionAccountsOf<T>` + 主账户/
//!   费用账户/初始余额合计。

extern crate alloc;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use frame_support::ensure;
use frame_support::traits::Currency;
use sp_runtime::{
    traits::{CheckedAdd, Zero},
    DispatchError,
};

use crate::institution::types::CreateInstitutionAccount;
use crate::pallet::{
    AccountRegisteredCid, CidNumberOf, Config, CreateInstitutionAccountsOf, Error,
    InstitutionAccounts, InstitutionInitialAccountsOf, Pallet,
};
use crate::traits::{AccountValidator, ProtectedSourceChecker, ReservedAccountGuard};
use crate::BalanceOf;

/// 根据 CID 制度约束自动生成全部强制协议账户，初始余额统一为零。
/// 首次登记不得从交易载荷接收账户清单或初始入金。
/// `parent_cid_number` 只对 `UNIN` 非法人组织有意义（父级为 `SFGF` 时才配清算账户），
/// 其余机构码一律传 `None`；父级是否已登记由注册入口另行校验。
pub(crate) fn build_required_protocol_accounts<T: Config>(
    cid_number: &CidNumberOf<T>,
    parent_cid_number: Option<&[u8]>,
) -> Result<InstitutionInitialAccountsOf<T>, DispatchError> {
    let code = primitives::cid::code::institution_code_from_cid_number(
        core::str::from_utf8(cid_number.as_slice()).map_err(|_| Error::<T>::InvalidCidNumber)?,
    )
    .ok_or(Error::<T>::InvalidCidNumber)?;
    let required = primitives::institution_constraints::required_protocol_account_kinds(
        code,
        cid_number.as_slice(),
        parent_cid_number,
    )
    .ok_or(Error::<T>::InvalidCidNumber)?;
    let items: Vec<crate::InstitutionInitialAccountOf<T>> = required
        .iter()
        .map(|kind| {
            let account_name = primitives::account_derive::institution_protocol_account_name(*kind)
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::EmptyAccountName)?;
            Ok(crate::InstitutionInitialAccount {
                account_name,
                amount: BalanceOf::<T>::zero(),
            })
        })
        .collect::<Result<_, DispatchError>>()?;
    items
        .try_into()
        .map_err(|_| Error::<T>::TooManyInstitutionAccounts.into())
}

/// 校验机构初始账户列表合法性,派生地址,返回:
/// - 固化的 `CreateInstitutionAccountsOf<T>`(已派生完地址)
/// - 主账户 AccountId
/// - 费用账户 AccountId
/// - 初始余额合计
///
/// `parent_cid_number` 语义同 `build_required_protocol_accounts`。
type ValidatedInitialAccounts<T> = (
    CreateInstitutionAccountsOf<T>,
    <T as frame_system::Config>::AccountId,
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
);

pub(crate) fn validate_initial_accounts<T: Config>(
    cid_number: &CidNumberOf<T>,
    accounts: &InstitutionInitialAccountsOf<T>,
    parent_cid_number: Option<&[u8]>,
) -> Result<ValidatedInitialAccounts<T>, DispatchError> {
    ensure!(!accounts.is_empty(), Error::<T>::EmptyInstitutionAccounts);

    let mut seen = BTreeSet::new();
    let required_protocol_kinds =
        primitives::institution_constraints::required_protocol_account_kinds(
            primitives::cid::code::institution_code_from_cid_number(
                core::str::from_utf8(cid_number.as_slice())
                    .map_err(|_| Error::<T>::InvalidCidNumber)?,
            )
            .ok_or(Error::<T>::InvalidCidNumber)?,
            cid_number.as_slice(),
            parent_cid_number,
        )
        .ok_or(Error::<T>::InvalidCidNumber)?;
    let mut protocol_kinds = BTreeSet::new();
    let mut main_account: Option<T::AccountId> = None;
    let mut fee_account: Option<T::AccountId> = None;
    let mut initial_total = BalanceOf::<T>::zero();
    let mut built: Vec<CreateInstitutionAccount<_, T::AccountId, BalanceOf<T>>> =
        Vec::with_capacity(accounts.len());

    for item in accounts.iter() {
        ensure!(!item.account_name.is_empty(), Error::<T>::EmptyAccountName);
        ensure!(
            item.amount.is_zero() || item.amount >= T::Currency::minimum_balance(),
            Error::<T>::AccountInitialAmountBelowMinimum
        );
        ensure!(
            seen.insert(item.account_name.to_vec()),
            Error::<T>::DuplicateAccountName
        );

        let (account_id, kind) = Pallet::<T>::derive_institution_account(
            cid_number.as_slice(),
            item.account_name.as_slice(),
        )?;
        ensure!(
            !InstitutionAccounts::<T>::contains_key(cid_number, &item.account_name),
            Error::<T>::CidAlreadyRegistered
        );
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

        if let Some(protocol_kind) = kind.institution_protocol_kind() {
            ensure!(
                required_protocol_kinds.contains(&protocol_kind),
                Error::<T>::ReservedAccountName
            );
            protocol_kinds.insert(protocol_kind);
            if protocol_kind == primitives::account_derive::InstitutionProtocolAccountKind::Main {
                main_account = Some(account_id.clone());
            }
            if protocol_kind == primitives::account_derive::InstitutionProtocolAccountKind::Fee {
                fee_account = Some(account_id.clone());
            }
        }

        initial_total = initial_total
            .checked_add(&item.amount)
            .ok_or(Error::<T>::InitialAmountOverflow)?;
        built.push(CreateInstitutionAccount {
            account_name: item.account_name.clone(),
            account_id,
            amount: item.amount,
        });
    }

    // 私权机构当前统一要求主账户与费用账户；分别报错，禁止把缺费用账户退化成
    // “缺主账户”或静默补齐。其他机构类型的额外协议账户由同一约束表控制。
    ensure!(main_account.is_some(), Error::<T>::MissingMainAccount);
    ensure!(fee_account.is_some(), Error::<T>::MissingFeeAccount);
    ensure!(
        required_protocol_kinds
            .iter()
            .all(|required| protocol_kinds.contains(required)),
        Error::<T>::MissingMainAccount
    );
    let bounded: CreateInstitutionAccountsOf<T> = built
        .try_into()
        .map_err(|_| Error::<T>::TooManyInstitutionAccounts)?;
    Ok((
        bounded,
        main_account.ok_or(Error::<T>::MissingMainAccount)?,
        fee_account.ok_or(Error::<T>::MissingFeeAccount)?,
        initial_total,
    ))
}
