//! 运行时升级模块 Benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Get};
use sp_runtime::sp_std::vec;
use votingengine::CitizenIdentityReader;

use crate::pallet::{CodeOf, Config, ReasonOf};
use crate::Pallet;

const BENCH_MAX_REASON_LEN: u32 = 1024;
const BENCH_MAX_CODE_SIZE: u32 = 5 * 1024 * 1024;

fn reason_max<T: Config>() -> ReasonOf<T> {
    assert_eq!(
        T::MaxReasonLen::get(),
        BENCH_MAX_REASON_LEN,
        "update BENCH_MAX_REASON_LEN when runtime MaxReasonLen changes"
    );
    vec![b'r'; BENCH_MAX_REASON_LEN as usize]
        .try_into()
        .expect("benchmark reason should fit")
}

fn code_max<T: Config>() -> CodeOf<T> {
    assert_eq!(
        T::MaxRuntimeCodeSize::get(),
        BENCH_MAX_CODE_SIZE,
        "update BENCH_MAX_CODE_SIZE when runtime MaxRuntimeCodeSize changes"
    );
    vec![b'c'; BENCH_MAX_CODE_SIZE as usize]
        .try_into()
        .expect("benchmark runtime code should fit")
}

fn nrc_cid_number() -> votingengine::CidNumber {
    primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound")
}

fn committee_role_code() -> votingengine::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("committee role fits runtime bound")
}

fn seed_population<T>()
where
    T: Config + joint_vote::Config,
{
    let scope = votingengine::PopulationScope::Country;
    let citizen: T::AccountId = account("runtime-upgrade-citizen", 0, 0);
    <T as votingengine::Config>::CitizenIdentityReader::benchmark_seed_identity(&citizen, &scope);
}

#[benchmarks(where T: Config + joint_vote::Config)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn propose_runtime_upgrade() {
        let origin = T::ProposeOrigin::try_successful_origin()
            .expect("benchmark proposer_account_id origin must be available");
        let reason = reason_max::<T>();
        let code = code_max::<T>();
        seed_population::<T>();

        #[block]
        {
            Pallet::<T>::propose_runtime_upgrade(
                origin,
                nrc_cid_number(),
                committee_role_code(),
                reason,
                code,
                pow_difficulty::ActiveParams::<T>::get(),
            )
            .expect("benchmark runtime upgrade proposal should succeed");
        }

        let proposal_id = votingengine::Pallet::<T>::next_proposal_id().saturating_sub(1);
        assert!(
            votingengine::Pallet::<T>::get_proposal_data(proposal_id).is_some(),
            "runtime upgrade benchmark should store proposal data in voting engine"
        );
    }
}
