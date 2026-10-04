//! 全节点发行模块 Benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use crate::{pallet, Call, Config, Pallet};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn bind_reward_account() {
        let miner_account_id: T::AccountId = frame_benchmarking::account("miner", 0, 0);
        let reward_account_id: T::AccountId = frame_benchmarking::account("reward-account", 0, 0);

        #[extrinsic_call]
        bind_reward_account(RawOrigin::Signed(miner_account_id), reward_account_id);
    }

    #[benchmark]
    fn rebind_reward_account() {
        let miner_account_id: T::AccountId = frame_benchmarking::account("miner", 0, 0);
        let reward_account_id: T::AccountId = frame_benchmarking::account("reward-account", 0, 0);
        let new_reward_account_id: T::AccountId =
            frame_benchmarking::account("new-reward-account", 0, 0);

        // 前置：先绑定一次
        pallet::RewardAccountIdByMiner::<T>::insert(&miner_account_id, &reward_account_id);

        #[extrinsic_call]
        rebind_reward_account(RawOrigin::Signed(miner_account_id), new_reward_account_id);
    }
}
