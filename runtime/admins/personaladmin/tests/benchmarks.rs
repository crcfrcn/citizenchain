//! 真实管理员集合变更基准：快照和提案执行仍走原内部投票引擎。
#![cfg(feature = "runtime-benchmarks")]
extern crate alloc;
use crate::{
    pallet::{AdminAccounts, AdminsOf},
    Call, Config, Pallet,
};
use adminbase::{Admin, AdminAccount, AdminAccountKind, AdminAccountStatus};
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;

#[benchmarks(where T: internalvote::Config)]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn propose_admin_set_change(n: Linear<2, 64>) -> Result<(), BenchmarkError> {
        frame_system::Pallet::<T>::set_block_number(1u32.into());
        if n > <T as Config>::MaxPersonalAccountAdmins::get() {
            return Err(BenchmarkError::Stop("管理员容量小于基准输入"));
        }
        let owner: T::AccountId = account("personal", 0, 0);
        let make = |label: &'static str, index: u32| Admin {
            account_id: account(label, index, 0),
            cid_number: Default::default(),
            family_name: b"Family".to_vec().try_into().unwrap(),
            given_name: b"Given".to_vec().try_into().unwrap(),
        };
        let old: AdminsOf<T> = (0..n)
            .map(|i| make("old", i))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let next: AdminsOf<T> = (0..n)
            .map(|i| make("next", i))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let caller = old[0].account_id.clone();
        let threshold = n / 2 + 1;
        AdminAccounts::<T>::insert(
            &owner,
            AdminAccount {
                cid_number: Default::default(),
                institution_code: votebase::types::PMUL,
                kind: AdminAccountKind::PersonalMultisig,
                admins: old.clone(),
                creator_account_id: caller.clone(),
                created_at: 1u32.into(),
                updated_at: 1u32.into(),
                status: AdminAccountStatus::Active,
            },
        );
        internalvote::ActivePersonalThresholds::<T>::insert(&owner, threshold);
        let before = votebase::Pallet::<T>::next_proposal_id();
        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            votebase::types::PMUL,
            owner.clone(),
            next,
            threshold,
        );
        assert_eq!(votebase::Pallet::<T>::next_proposal_id(), before + 1);
        assert_eq!(AdminAccounts::<T>::get(&owner).unwrap().admins, old);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<T>::get(&owner),
            Some(threshold)
        );
        Ok(())
    }
    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
