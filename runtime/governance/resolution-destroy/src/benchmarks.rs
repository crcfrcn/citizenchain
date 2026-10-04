//! 决议销毁模块 Benchmark 定义。
//!
//! 投票统一走 `votingengine::internal_vote`,本模块只覆盖"发起提案"和
//! "任意人重试执行"两条路径。

#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;

use crate::{BalanceOf, Call, Config, Pallet};
use primitives::cid::china::china_cb::CHINA_CB;

fn decode_account<T: Config>(raw: [u8; 32]) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..]).expect("benchmark account must decode")
}

fn prc_institution<T: Config>() -> T::AccountId {
    decode_account::<T>(CHINA_CB[1].main_account)
}

fn prc_admin<T: Config>(index: usize) -> T::AccountId {
    decode_account::<T>(CHINA_CB[1].admins[index])
}

fn prc_cid_number() -> votingengine::CidNumber {
    CHINA_CB[1]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRC CID fits runtime bound")
}

fn last_proposal_id<T: Config>() -> u64 {
    votingengine::Pallet::<T>::next_proposal_id().saturating_sub(1)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn propose_destroy() {
        let institution = prc_institution::<T>();
        let proposer = prc_admin::<T>(0);
        let amount: BalanceOf<T> = 100u128.saturated_into();

        #[extrinsic_call]
        propose_destroy(
            RawOrigin::Signed(proposer.clone()),
            prc_cid_number(),
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("benchmark role fits"),
            institution,
            amount,
        );

        let proposal_id = last_proposal_id::<T>();
        assert!(votingengine::Pallet::<T>::get_proposal_data(proposal_id).is_some());
    }

    // execute_destroy benchmark 已废弃: 该 wrapper extrinsic 已统一到
    // VotingEngine::retry_passed_proposal,benchmark 由 votingengine 自身覆盖。
}
