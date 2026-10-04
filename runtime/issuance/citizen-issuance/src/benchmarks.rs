//! 公民发行模块 benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use citizen_identity::OnVotingIdentityRegistered;
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;

use crate::pallet::{
    AccountRewarded, Config, IdentityRewardClaimed, Pallet, PendingRewardCount, RewardedCount,
};

fn decode_account<T: Config>(raw: [u8; 32]) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..]).expect("benchmark account must decode")
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn on_voting_identity_registered() {
        let who = decode_account::<T>([7u8; 32]);
        let cid_number = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "bench-0001",
                p1: "1",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "CTZN",
            },
        )
        .expect("citizen cid should generate");
        let cid_number = citizen_identity::CidNumberBound::try_from(cid_number.into_bytes())
            .expect("benchmark cid number should fit");

        #[block]
        {
            <Pallet<T> as OnVotingIdentityRegistered<T::AccountId>>::on_voting_identity_registered(
                &who,
                &cid_number,
            );
            Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
        }

        assert_eq!(RewardedCount::<T>::get(), 1u64);
        assert_eq!(PendingRewardCount::<T>::get(), 0u32);
        assert!(IdentityRewardClaimed::<T>::contains_key(cid_number));
        assert!(AccountRewarded::<T>::contains_key(&who));
    }
}
