//! 决议发行共享校验逻辑。

use crate::pallet::{AllocationOf, BalanceOf, Config, Error, Pallet};
use codec::Decode;
use frame_support::{dispatch::DispatchResult, ensure, BoundedVec};
use primitives::cid::china::china_cb::CHINA_CB;
use sp_runtime::traits::{CheckedAdd, Zero};
use sp_std::collections::btree_set::BTreeSet;

impl<T: Config> Pallet<T> {
    pub(crate) fn validate_proposal_allocations(
        total_amount: &BalanceOf<T>,
        allocations: &[crate::proposal::RecipientAmount<T::AccountId, BalanceOf<T>>],
    ) -> DispatchResult {
        ensure!(!allocations.is_empty(), Error::<T>::EmptyAllocations);
        Self::ensure_nonzero_total(total_amount)?;
        let expected = crate::pallet::AllowedRecipients::<T>::get();
        ensure!(!expected.is_empty(), Error::<T>::RecipientsNotConfigured);

        // 提案收款人集合必须与链上白名单完全一致，既不能少人，也不能多塞账户。
        let expected_set: BTreeSet<&T::AccountId> = expected.iter().collect();
        ensure!(
            expected_set.len() == expected.len(),
            Error::<T>::DuplicateAllowedRecipient
        );
        ensure!(
            allocations.len() == expected_set.len(),
            Error::<T>::InvalidAllocationCount
        );

        let mut seen: BTreeSet<&T::AccountId> = BTreeSet::new();
        let mut sum = BalanceOf::<T>::zero();
        for item in allocations {
            Self::ensure_nonzero_total(&item.amount)?;
            ensure!(
                seen.insert(&item.recipient_account_id),
                Error::<T>::DuplicateRecipient
            );
            ensure!(
                expected_set.contains(&item.recipient_account_id),
                Error::<T>::InvalidRecipientSet
            );
            sum = sum
                .checked_add(&item.amount)
                .ok_or(Error::<T>::AllocationOverflow)?;
        }

        ensure!(seen == expected_set, Error::<T>::InvalidRecipientSet);
        ensure!(sum == *total_amount, Error::<T>::TotalMismatch);
        Ok(())
    }

    pub(crate) fn ensure_unique_recipients(recipients: &[T::AccountId]) -> DispatchResult {
        let mut seen: BTreeSet<&T::AccountId> = BTreeSet::new();
        for recipient_account_id in recipients {
            ensure!(
                seen.insert(recipient_account_id),
                Error::<T>::DuplicateAllowedRecipient
            );
        }
        Ok(())
    }

    /// 所有收款账户必须是 CHINA_CB 省储委会地址（跳过索引 0 的 NRC）。
    pub(crate) fn ensure_recipients_in_china_cb(
        recipients: &BoundedVec<T::AccountId, T::MaxAllocations>,
    ) -> DispatchResult {
        let valid_set: BTreeSet<T::AccountId> = CHINA_CB
            .iter()
            .skip(1)
            .filter_map(|node| T::AccountId::decode(&mut &node.main_account[..]).ok())
            .collect();
        for recipient_account_id in recipients.iter() {
            ensure!(
                valid_set.contains(recipient_account_id),
                Error::<T>::RecipientNotInChinaCb
            );
        }
        Ok(())
    }

    pub(crate) fn validate_execution_allocations(
        total_amount: &BalanceOf<T>,
        allocations: &AllocationOf<T>,
    ) -> DispatchResult {
        Self::validate_proposal_allocations(total_amount, allocations.as_slice())
    }
}
