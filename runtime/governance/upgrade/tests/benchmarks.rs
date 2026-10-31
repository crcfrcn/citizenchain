//! 运行时升级模块 Benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Get};
use sp_runtime::sp_std::vec;
use votebase::CitizenIdentityReader;

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

fn nrc_cid_number() -> votebase::CidNumber {
    primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound")
}

fn committee_role_code() -> votebase::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("committee role fits runtime bound")
}

fn seed_population<T>()
where
    T: Config + jointvote::Config,
{
    let scope = votebase::PopulationScope::Country;
    let citizen: T::AccountId = account("runtime-upgrade-citizen", 0, 0);
    <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(&citizen, &scope);
}

#[benchmarks(where T: Config + jointvote::Config)]
mod benchmarks {
    use super::*;

    // 固定席位按协议准备最大姓名/CID/引用；新提案ID的快照键为空。
    #[benchmark(pov_mode = MaxEncodedLen {
        PublicAdmins::AdminAccounts: Measured,
        PublicManage::InstitutionRoleAssignments: Measured,
        VotingEngine::VoterSnapshot: Measured
    })]
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
                difficulty::ActiveParams::<T>::get(),
            )
            .expect("benchmark runtime upgrade proposal should succeed");
        }

        let proposal_id = votebase::Pallet::<T>::next_proposal_id().saturating_sub(1);
        assert!(
            votebase::Pallet::<T>::get_proposal_data(proposal_id).is_some(),
            "runtime upgrade benchmark should store proposal data in voting engine"
        );
    }
    #[benchmark]
    fn developer_overhead() {
        let origin = T::DeveloperUpgradeOrigin::try_successful_origin()
            .expect("NRC真实管理员来源必须可准备");
        let code = code_max::<T>();
        #[block]
        {
            Pallet::<T>::developer_direct_upgrade(
                origin,
                nrc_cid_number(),
                committee_role_code(),
                code,
                difficulty::ActiveParams::<T>::get(),
            )
            .expect("开发直升编排必须执行真实授权/PoW暂存/审计");
        }
        assert!(crate::LastRuntimeUpgradeAudit::<T>::get().is_some());
    }
}
