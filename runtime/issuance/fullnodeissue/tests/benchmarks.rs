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
    /// 真实PoW作者、绑定接收方和新账户铸发的finalize最重路径。
    #[benchmark]
    fn on_finalize_reward() {
        use codec::Encode;
        use frame_support::traits::{Currency, FindAuthor, Hooks};
        use sp_runtime::traits::{SaturatedConversion, Zero};
        frame_system::Pallet::<T>::set_block_number(1u32.into());
        let miner: T::AccountId = account("pow-miner", 0, 0);
        let recipient: T::AccountId = account("new-reward", 0, 0);
        let encoded = miner.encode();
        let author = T::FindAuthor::find_author([(*b"pow_", encoded.as_slice())])
            .expect("真实PoW作者必须可解码");
        frame_system::Pallet::<T>::deposit_log(sp_runtime::DigestItem::PreRuntime(
            *b"pow_", encoded,
        ));
        pallet::RewardAccountIdByMiner::<T>::insert(&author, &recipient);
        drop(T::Currency::make_free_balance_be(&recipient, Zero::zero()));
        #[block]
        {
            Pallet::<T>::on_finalize(1u32.into());
        }
        assert_eq!(pallet::LastAuthoredBlockByMiner::<T>::get(&author), Some(1));
        assert_eq!(
            T::Currency::free_balance(&recipient),
            primitives::pow_const::FULLNODE_BLOCK_REWARD.saturated_into()
        );
    }
}
