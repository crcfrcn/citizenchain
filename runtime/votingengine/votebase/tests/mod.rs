//! 核心引擎的独立测试环境；外部业务通过公开trait提供可控测试响应。

use super::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::{ConstU32, ConstU64, UnixTime},
    weights::{constants::RocksDbWeight, Weight},
};
use sp_runtime::{BuildStorage, DispatchError};
use std::cell::Cell;

type Block = frame_system::mocking::MockBlock<Test>;
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;
    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type VotingEngine = super;
}
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type DbWeight = RocksDbWeight;
}
parameter_types! {
    pub static AutoBudget: Weight = Weight::MAX;
    pub static ExecutionBudget: Weight = Weight::MAX;
    pub static CleanupBudget: Weight = Weight::MAX;
}
pub struct Clock;
impl UnixTime for Clock {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_783_036_800)
    }
}
thread_local! {
    static FAIL_CALLBACK: Cell<bool> = const { Cell::new(false) };
    static CALLBACK_COUNT: Cell<u32> = const { Cell::new(0) };
}
pub struct Callback;
impl InternalVoteResultCallback for Callback {
    fn on_internal_vote_finalized(
        _id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        if !approved {
            return Ok(ProposalExecutionOutcome::Ignored);
        }
        CALLBACK_COUNT.with(|count| count.set(count.get() + 1));
        sp_io::storage::set(b"test:execution-effect", b"written");
        if FAIL_CALLBACK.with(Cell::get) {
            Err(DispatchError::Other("test callback failure"))
        } else {
            Ok(ProposalExecutionOutcome::Executed)
        }
    }
}
pub struct Track;
impl ProposalTrackHandler<u64, u64> for Track {
    fn handles(kind: u8) -> bool {
        kind == PROPOSAL_KIND_INTERNAL
    }
    fn finalize_timeout(
        proposal: &Proposal<u64, u64>,
        id: u64,
    ) -> Option<frame_support::dispatch::DispatchResult> {
        if !Self::handles(proposal.kind) {
            return None;
        }
        Some(if System::block_number() <= proposal.end {
            Err(DispatchError::Other("test track still voting"))
        } else {
            VotingEngine::set_status_and_emit(id, STATUS_REJECTED)
        })
    }
    fn cleanup_chunk(kind: u8, _id: u64, _limit: u32) -> Option<traits::CleanupChunkResult> {
        Self::handles(kind).then_some((0, false))
    }
    fn cleanup_terminal(kind: u8, _id: u64) -> Option<()> {
        Self::handles(kind).then_some(())
    }
    fn timeout_weight(_stage: u8) -> Option<Weight> {
        Some(Weight::from_parts(1_000, 32))
    }
    fn cleanup_chunk_weight(kind: u8, _limit: u32) -> Option<Weight> {
        Self::handles(kind).then_some(Weight::from_parts(1_000, 32))
    }
    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        Self::handles(kind).then_some(Weight::from_parts(1_000, 32))
    }
}
impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<2>;
    type MaxAutoFinalizeWeightPerBlock = AutoBudget;
    type MaxExecutionWeightPerBlock = ExecutionBudget;
    type MaxCleanupWeightPerBlock = CleanupBudget;
    type MaxProposalsPerExpiry = ConstU32<2>;
    type MaxInternalProposalMutexBindings = ConstU32<8>;
    type MaxActiveProposals = ConstU32<2>;
    type MaxCleanupStepsPerBlock = ConstU32<2>;
    type MaxCleanupActivationsPerBlock = ConstU32<2>;
    type CleanupKeysPerStep = ConstU32<1>;
    type MaxProposalDataLen = ConstU32<32>;
    type MaxProposalObjectLen = ConstU32<64>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = ConstU64<10>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<4>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<2>;
    type CitizenIdentityReader = ();
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = Callback;
    type InternalAdminProvider = ();
    type MaxAdminsPerInstitution = ConstU32<8>;
    type TimeProvider = Clock;
    type WeightInfo = ();
    type TrackHandlers = (Track, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}
fn ext() -> sp_io::TestExternalities {
    FAIL_CALLBACK.with(|value| value.set(false));
    CALLBACK_COUNT.with(|value| value.set(0));
    AutoBudget::set(Weight::MAX);
    ExecutionBudget::set(Weight::MAX);
    CleanupBudget::set(Weight::MAX);
    let mut ext = sp_io::TestExternalities::new(
        frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("核心测试存储"),
    );
    ext.execute_with(|| System::set_block_number(1));
    ext
}
fn seed(id: u64, end: u64) {
    ProposalDisplayId::<Test>::insert(
        id,
        ProposalDisplayMeta {
            year: 2026,
            seq_in_year: id as u32,
        },
    );
    Proposals::<Test>::insert(
        id,
        Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: STATUS_VOTING,
            internal_code: Some(PMUL),
            actor_cid_number: None,
            execution_account_id: Some(1),
            subject_cid_numbers: Default::default(),
            start: 1,
            end,
        },
    );
}

mod lifecycle;
mod maintenance;
mod types;

mod property;
