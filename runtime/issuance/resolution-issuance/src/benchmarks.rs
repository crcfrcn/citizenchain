//! 决议发行完整模块 Benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::{
    traits::{EnsureOrigin, Get},
    BoundedVec,
};
use primitives::cid::china::china_cb::CHINA_CB;
use sp_runtime::traits::{CheckedAdd, SaturatedConversion, Zero};
use sp_std::{vec, vec::Vec};
use votingengine::CitizenIdentityReader;

use crate::{pallet, AllowedRecipients, Config, Pallet, VotingProposalCount};

fn decode_account<T: pallet::Config>(raw: [u8; 32]) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..]).expect("benchmark account must decode")
}

fn prc_recipients<T: pallet::Config>() -> BoundedVec<T::AccountId, T::MaxAllocations> {
    let recipients: Vec<T::AccountId> = CHINA_CB
        .iter()
        .skip(1)
        .map(|node| decode_account::<T>(node.main_account))
        .collect();
    recipients
        .try_into()
        .expect("benchmark recipients should fit MaxAllocations")
}

fn reason_max<T: pallet::Config>() -> pallet::ReasonOf<T> {
    let len = core::cmp::max(1usize, T::MaxReasonLen::get() as usize);
    vec![b'r'; len].try_into().expect("max reason fits")
}

fn full_allocations<T: pallet::Config>() -> (pallet::AllocationOf<T>, pallet::BalanceOf<T>) {
    let recipients = prc_recipients::<T>();
    let mut allocations: Vec<crate::proposal::RecipientAmount<T::AccountId, pallet::BalanceOf<T>>> =
        Vec::with_capacity(recipients.len());
    let mut total = pallet::BalanceOf::<T>::zero();
    for recipient_account_id in recipients {
        let amount: pallet::BalanceOf<T> = 1_000_000u128.saturated_into();
        total = total
            .checked_add(&amount)
            .expect("benchmark total should fit");
        allocations.push(crate::proposal::RecipientAmount {
            recipient_account_id,
            amount,
        });
    }
    (
        allocations
            .try_into()
            .expect("benchmark allocations should fit MaxAllocations"),
        total,
    )
}

fn nrc_cid_number() -> votingengine::types::CidNumber {
    CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID should fit")
}

#[benchmarks(where T: Config + joint_vote::Config)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn propose_issuance() {
        let origin = T::ProposeOrigin::try_successful_origin()
            .expect("benchmark proposer_account_id origin must be available");
        let recipients = prc_recipients::<T>();
        AllowedRecipients::<T>::put(recipients);
        VotingProposalCount::<T>::put(0u32);
        let actor_cid_number = nrc_cid_number();
        let proposer_role_code: votingengine::types::RoleCode =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role code must fit");
        let scope = votingengine::PopulationScope::Country;
        let citizen: T::AccountId = frame_benchmarking::account("resolution-citizen", 0, 0);
        <T as votingengine::Config>::CitizenIdentityReader::benchmark_seed_identity(
            &citizen, &scope,
        );

        let reason = reason_max::<T>();
        let (allocations, total_amount) = full_allocations::<T>();

        #[block]
        {
            Pallet::<T>::propose_issuance(
                origin,
                actor_cid_number,
                proposer_role_code,
                reason,
                total_amount,
                allocations,
            )
            .expect("benchmark resolution issuance proposal should succeed");
        }

        assert_eq!(VotingProposalCount::<T>::get(), 1u32);
    }
}
