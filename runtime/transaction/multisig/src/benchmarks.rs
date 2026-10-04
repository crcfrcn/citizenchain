//! 多签资金账户转账模块 Benchmark 定义。
//!
//! 本 benchmark 当前覆盖治理机构路径；个人多签与注册机构账户
//! 通过相同 `propose_transfer` 入口和查询 trait 接入，职责边界不在本文件复刻。
//!
//! 机构岗位选民与个人多签管理员投票一律通过 `InternalVote::cast`(20.0)。
//! 本文件只保留 `propose_transfer` benchmark;手动重试统一走
//! `VotingEngine::retry_passed_proposal`,投票与重试 weight 全部归入
//! votingengine pallet 自身的 benchmark,业务端无需重复覆盖。

#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::traits::Currency;
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;

use crate::{BalanceOf, Call, Config, Pallet, CHINA_CB};

fn decode_account<T: Config>(raw: [u8; 32]) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..]).expect("benchmark account must decode")
}

fn prc_main_account<T: Config>() -> T::AccountId {
    decode_account::<T>(CHINA_CB[1].main_account)
}

fn prc_fee_account<T: Config>() -> T::AccountId {
    decode_account::<T>(CHINA_CB[1].fee_account)
}

fn prc_actor_cid() -> votingengine::types::CidNumber {
    CHINA_CB[1]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRC CID should fit")
}

fn prc_admin<T: Config>(index: usize) -> T::AccountId {
    decode_account::<T>(CHINA_CB[1].admins[index])
}

fn beneficiary_account_id<T: Config>() -> T::AccountId {
    decode_account::<T>([99u8; 32])
}

fn last_proposal_id<T: Config>() -> u64 {
    votingengine::Pallet::<T>::next_proposal_id().saturating_sub(1)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn propose_transfer() {
        let funding_account_id = prc_main_account::<T>();
        let fee_account = prc_fee_account::<T>();
        let actor_cid_number = prc_actor_cid();
        let proposer_account_id = prc_admin::<T>(0);
        let beneficiary_account_id = beneficiary_account_id::<T>();
        let amount: BalanceOf<T> = 111u128.saturated_into();
        let top_up: BalanceOf<T> = 1_000_000u128.saturated_into();

        let _ = T::Currency::deposit_creating(&funding_account_id, top_up);
        // 机构本金账户只承担本金，链上手续费必须由同 CID 的费用账户承担。
        let _ = T::Currency::deposit_creating(&fee_account, top_up);

        #[extrinsic_call]
        propose_transfer(
            RawOrigin::Signed(proposer_account_id.clone()),
            Some(actor_cid_number),
            Some(
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("benchmark role fits"),
            ),
            funding_account_id,
            beneficiary_account_id,
            amount,
            BoundedVec::default(),
        );

        let pid = last_proposal_id::<T>();
        assert!(votingengine::Pallet::<T>::get_proposal_data(pid).is_some());
    }

    // execute_transfer / execute_safety_fund_transfer / execute_sweep_to_main
    // benchmark 已废弃: 三个 wrapper extrinsic 已统一到
    // VotingEngine::retry_passed_proposal,benchmark 由 votingengine 自身覆盖。
}
