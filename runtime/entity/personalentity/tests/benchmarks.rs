//! 个人账户创建和关闭真实提案基准；准备阶段只写公开的测试/基准状态。
#![cfg(feature = "runtime-benchmarks")]
extern crate alloc;
use crate::{
    pallet::{
        AccountNameOf, AdminsOf, PendingCloseProposal, PendingPersonalCreate, PersonalAccounts,
    },
    types::{PersonalAccount, PersonalStatus},
    Call, Config, Pallet,
};
use adminbase::{Admin, AdminAccount, AdminAccountKind, AdminAccountStatus};
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;

fn administrators<T: Config>(n: u32) -> AdminsOf<T> {
    (0..n)
        .map(|i| Admin {
            account_id: account("admin", i, 0),
            cid_number: Default::default(),
            family_name: b"Family".to_vec().try_into().unwrap(),
            given_name: b"Given".to_vec().try_into().unwrap(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

#[benchmarks(where T: internalvote::Config + personaladmin::Config)]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn propose_create(n: Linear<2, 64>) -> Result<(), BenchmarkError> {
        frame_system::Pallet::<T>::set_block_number(1u32.into());
        if n > <T as Config>::MaxPersonalAccountAdmins::get() {
            return Err(BenchmarkError::Stop("管理员容量小于基准输入"));
        }
        let admins = administrators::<T>(n);
        let caller = admins[0].account_id.clone();
        let name: AccountNameOf<T> = b"benchmark-create".to_vec().try_into().unwrap();
        let owner = Pallet::<T>::derive_personal_account(&caller, name.as_slice())?;
        let _ = T::Currency::make_free_balance_be(&caller, 1_000_000_000u128.saturated_into());
        let amount = <T as Config>::MinCreateAmount::get();
        let id = votebase::Pallet::<T>::next_proposal_id();
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), name, admins, n / 2 + 1, amount);
        assert!(PendingPersonalCreate::<T>::contains_key(id));
        assert_eq!(
            PersonalAccounts::<T>::get(owner).unwrap().status,
            PersonalStatus::Pending
        );
        Ok(())
    }
    #[benchmark]
    fn propose_close() -> Result<(), BenchmarkError> {
        frame_system::Pallet::<T>::set_block_number(1u32.into());
        let admins = administrators::<T>(3);
        let caller = admins[0].account_id.clone();
        let name: AccountNameOf<T> = b"benchmark-close".to_vec().try_into().unwrap();
        let owner = Pallet::<T>::derive_personal_account(&caller, name.as_slice())?;
        let beneficiary: T::AccountId = account("beneficiary", 0, 0);
        PersonalAccounts::<T>::insert(
            &owner,
            PersonalAccount {
                creator_account_id: caller.clone(),
                account_name: name,
                created_at: 1u32.into(),
                status: PersonalStatus::Active,
            },
        );
        let records: personaladmin::pallet::AdminsOf<T> = admins
            .into_inner()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("管理员名册容量不匹配"))?;
        personaladmin::AdminAccounts::<T>::insert(
            &owner,
            AdminAccount {
                cid_number: Default::default(),
                institution_code: votebase::types::PMUL,
                kind: AdminAccountKind::PersonalMultisig,
                admins: records,
                creator_account_id: caller.clone(),
                created_at: 1u32.into(),
                updated_at: 1u32.into(),
                status: AdminAccountStatus::Active,
            },
        );
        internalvote::ActivePersonalThresholds::<T>::insert(&owner, 2);
        let _ = T::Currency::make_free_balance_be(&owner, 1_000_000_000u128.saturated_into());
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), owner.clone(), beneficiary);
        assert!(PendingCloseProposal::<T>::contains_key(owner));
        Ok(())
    }
    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
