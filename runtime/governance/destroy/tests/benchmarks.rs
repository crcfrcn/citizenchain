//! 基准测量决议销毁提案及执行重试入口。

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
    frame_benchmarking::account(CHINA_CB[1].cid_number, index as u32, 0)
}

fn prc_cid_number() -> engingcore::CidNumber {
    CHINA_CB[1]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRC CID fits runtime bound")
}

fn last_proposal_id<T: Config>() -> u128 {
    engingcore::Pallet::<T>::next_proposal_id().saturating_sub(1)
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
        assert!(engingcore::Pallet::<T>::get_proposal_data(proposal_id).is_some());
    }

    // execute_destroy benchmark 已废弃: 该 wrapper extrinsic 已统一到
    // VotingEngine::retry_passed_proposal,benchmark 由 engingcore 自身覆盖。
}
