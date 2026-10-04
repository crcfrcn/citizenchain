//! 投票判定后由业务模块认领并执行的统一回调。

use frame_support::dispatch::DispatchResult;
use sp_runtime::DispatchError;

use crate::{ProposalCancelDecision, ProposalExecutionOutcome};

pub trait JointVoteResultCallback {
    fn on_joint_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError>;

    fn can_cancel_passed_proposal(
        _proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        Ok(ProposalCancelDecision::Ignored)
    }

    fn on_execution_failed_terminal(_proposal_id: u64) -> DispatchResult {
        Ok(())
    }
}

impl JointVoteResultCallback for () {
    fn on_joint_vote_finalized(
        _vote_proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        Ok(ProposalExecutionOutcome::Ignored)
    }
}

/// 内部投票终态回调。
///
/// 投票引擎在提案进入 `STATUS_PASSED` / `STATUS_REJECTED` 时对所有注册
/// 的业务模块广播此回调，并根据返回的 [`ProposalExecutionOutcome`] 统一推进状态。
///
/// 业务模块应当:
/// - 通过 `ProposalData` 的 `MODULE_TAG` 前缀(或业务独立存储键)认领自己的提案,
///   不属于自己的提案直接返回 `ProposalExecutionOutcome::Ignored` 跳过;
/// - `approved = true` 时执行具体业务动作(转账 / 替换管理员 / 销毁 / ...);
/// - `approved = false` 时可选清理业务独立存储(如 `SweepProposalActions`)。
///
/// 通过提案的回调由异步执行队列调用；返回 `Err` 只回滚本次业务执行尝试，
/// 不撤销已经成立的 PASSED 投票判定，并由引擎按指数退避继续重试。
///
/// 多业务模块通过 tuple 注册(见下方 `impl` for `(A,)`、`(A, B)` ... 等元组类型)。
pub trait InternalVoteResultCallback {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError>;

    fn can_cancel_passed_proposal(
        _proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        Ok(ProposalCancelDecision::Ignored)
    }

    fn on_execution_failed_terminal(_proposal_id: u64) -> DispatchResult {
        Ok(())
    }
}

/// 默认空实现(未挂业务回调时使用 `type X = ()`)。
impl InternalVoteResultCallback for () {
    fn on_internal_vote_finalized(
        _proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        Ok(ProposalExecutionOutcome::Ignored)
    }
}

fn merge_execution_outcome(
    current: ProposalExecutionOutcome,
    next: ProposalExecutionOutcome,
) -> ProposalExecutionOutcome {
    use ProposalExecutionOutcome::*;
    match (current, next) {
        (Ignored, outcome) => outcome,
        (outcome, Ignored) => outcome,
        (FatalFailed, _) | (_, FatalFailed) => FatalFailed,
        (RetryableFailed, _) | (_, RetryableFailed) => RetryableFailed,
        (Executed, Executed) => Executed,
    }
}

fn merge_cancel_decision(
    current: ProposalCancelDecision,
    next: ProposalCancelDecision,
) -> ProposalCancelDecision {
    match (current, next) {
        (ProposalCancelDecision::Allow, _) | (_, ProposalCancelDecision::Allow) => {
            ProposalCancelDecision::Allow
        }
        (ProposalCancelDecision::Ignored, ProposalCancelDecision::Ignored) => {
            ProposalCancelDecision::Ignored
        }
    }
}

// ──── InternalVoteResultCallback 的 tuple 实现(手写,覆盖 1~6 个成员)────
//
// 语义:依次调用每个成员的 `on_internal_vote_finalized`;任一成员返回 `Err`
// 立即短路返回,后续成员不再调用；外层 storage transaction 只回滚本次
// 异步执行尝试，投票判定保持 PASSED 并等待后续重试。
//
// 注:注册 5 个业务模块(multisig /
// public_manage/private_manage / RuntimeAdminAccountQuery / resolution_destroy /
// grandpakey_change),留 6 元组余量。如未来业务模块增加,补对应元组 impl。
impl<A: InternalVoteResultCallback> InternalVoteResultCallback for (A,) {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        A::on_internal_vote_finalized(proposal_id, approved)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        A::can_cancel_passed_proposal(proposal_id)
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)
    }
}

impl<A: InternalVoteResultCallback, B: InternalVoteResultCallback> InternalVoteResultCallback
    for (A, B)
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let a = A::on_internal_vote_finalized(proposal_id, approved)?;
        let b = B::on_internal_vote_finalized(proposal_id, approved)?;
        Ok(merge_execution_outcome(a, b))
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(a, b))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)
    }
}

impl<
        A: InternalVoteResultCallback,
        B: InternalVoteResultCallback,
        C: InternalVoteResultCallback,
    > InternalVoteResultCallback for (A, B, C)
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let mut outcome = A::on_internal_vote_finalized(proposal_id, approved)?;
        outcome = merge_execution_outcome(
            outcome,
            B::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            C::on_internal_vote_finalized(proposal_id, approved)?,
        );
        Ok(outcome)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        let c = C::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(merge_cancel_decision(a, b), c))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)?;
        C::on_execution_failed_terminal(proposal_id)
    }
}

impl<
        A: InternalVoteResultCallback,
        B: InternalVoteResultCallback,
        C: InternalVoteResultCallback,
        D: InternalVoteResultCallback,
    > InternalVoteResultCallback for (A, B, C, D)
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let mut outcome = A::on_internal_vote_finalized(proposal_id, approved)?;
        outcome = merge_execution_outcome(
            outcome,
            B::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            C::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            D::on_internal_vote_finalized(proposal_id, approved)?,
        );
        Ok(outcome)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        let c = C::can_cancel_passed_proposal(proposal_id)?;
        let d = D::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(
            merge_cancel_decision(merge_cancel_decision(a, b), c),
            d,
        ))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)?;
        C::on_execution_failed_terminal(proposal_id)?;
        D::on_execution_failed_terminal(proposal_id)
    }
}

impl<
        A: InternalVoteResultCallback,
        B: InternalVoteResultCallback,
        C: InternalVoteResultCallback,
        D: InternalVoteResultCallback,
        E: InternalVoteResultCallback,
    > InternalVoteResultCallback for (A, B, C, D, E)
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let mut outcome = A::on_internal_vote_finalized(proposal_id, approved)?;
        outcome = merge_execution_outcome(
            outcome,
            B::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            C::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            D::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            E::on_internal_vote_finalized(proposal_id, approved)?,
        );
        Ok(outcome)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        let c = C::can_cancel_passed_proposal(proposal_id)?;
        let d = D::can_cancel_passed_proposal(proposal_id)?;
        let e = E::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(
            merge_cancel_decision(merge_cancel_decision(merge_cancel_decision(a, b), c), d),
            e,
        ))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)?;
        C::on_execution_failed_terminal(proposal_id)?;
        D::on_execution_failed_terminal(proposal_id)?;
        E::on_execution_failed_terminal(proposal_id)
    }
}

impl<
        A: InternalVoteResultCallback,
        B: InternalVoteResultCallback,
        C: InternalVoteResultCallback,
        D: InternalVoteResultCallback,
        E: InternalVoteResultCallback,
        F: InternalVoteResultCallback,
    > InternalVoteResultCallback for (A, B, C, D, E, F)
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let mut outcome = A::on_internal_vote_finalized(proposal_id, approved)?;
        outcome = merge_execution_outcome(
            outcome,
            B::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            C::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            D::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            E::on_internal_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            F::on_internal_vote_finalized(proposal_id, approved)?,
        );
        Ok(outcome)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        let c = C::can_cancel_passed_proposal(proposal_id)?;
        let d = D::can_cancel_passed_proposal(proposal_id)?;
        let e = E::can_cancel_passed_proposal(proposal_id)?;
        let f = F::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(
            merge_cancel_decision(
                merge_cancel_decision(merge_cancel_decision(merge_cancel_decision(a, b), c), d),
                e,
            ),
            f,
        ))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)?;
        C::on_execution_failed_terminal(proposal_id)?;
        D::on_execution_failed_terminal(proposal_id)?;
        E::on_execution_failed_terminal(proposal_id)?;
        F::on_execution_failed_terminal(proposal_id)
    }
}

/// 立法投票终态业务回调(对称于 `JointVoteResultCallback`)。
/// 核心在立法提案进入 PASSED/REJECTED/EXECUTION_FAILED 时按 kind 广播到业务壳 legislation-yuan。
pub trait LegislationVoteResultCallback {
    fn on_legislation_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError>;

    fn can_cancel_passed_proposal(
        _proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        Ok(ProposalCancelDecision::Ignored)
    }

    fn on_execution_failed_terminal(_proposal_id: u64) -> DispatchResult {
        Ok(())
    }
}

impl LegislationVoteResultCallback for () {
    fn on_legislation_vote_finalized(
        _vote_proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        Ok(ProposalExecutionOutcome::Ignored)
    }
}

// 立法投票模块同时承载法律、任免和预算等业务的代表表决；各业务回调必须先以
// ProposalOwner/MODULE_TAG 认领提案。元组只负责聚合，不在投票引擎中理解业务载荷。
impl<A: LegislationVoteResultCallback> LegislationVoteResultCallback for (A,) {
    fn on_legislation_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        A::on_legislation_vote_finalized(proposal_id, approved)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        A::can_cancel_passed_proposal(proposal_id)
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)
    }
}

impl<A: LegislationVoteResultCallback, B: LegislationVoteResultCallback>
    LegislationVoteResultCallback for (A, B)
{
    fn on_legislation_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let a = A::on_legislation_vote_finalized(proposal_id, approved)?;
        let b = B::on_legislation_vote_finalized(proposal_id, approved)?;
        Ok(merge_execution_outcome(a, b))
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let a = A::can_cancel_passed_proposal(proposal_id)?;
        let b = B::can_cancel_passed_proposal(proposal_id)?;
        Ok(merge_cancel_decision(a, b))
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)
    }
}

impl<
        A: LegislationVoteResultCallback,
        B: LegislationVoteResultCallback,
        C: LegislationVoteResultCallback,
    > LegislationVoteResultCallback for (A, B, C)
{
    fn on_legislation_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        let mut outcome = A::on_legislation_vote_finalized(proposal_id, approved)?;
        outcome = merge_execution_outcome(
            outcome,
            B::on_legislation_vote_finalized(proposal_id, approved)?,
        );
        outcome = merge_execution_outcome(
            outcome,
            C::on_legislation_vote_finalized(proposal_id, approved)?,
        );
        Ok(outcome)
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        let mut decision = A::can_cancel_passed_proposal(proposal_id)?;
        decision = merge_cancel_decision(decision, B::can_cancel_passed_proposal(proposal_id)?);
        decision = merge_cancel_decision(decision, C::can_cancel_passed_proposal(proposal_id)?);
        Ok(decision)
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        A::on_execution_failed_terminal(proposal_id)?;
        B::on_execution_failed_terminal(proposal_id)?;
        C::on_execution_failed_terminal(proposal_id)
    }
}

/// 选举投票终态业务回调。
///
/// 当前 election-vote 自己返回 Executed 表示“当选结果快照已生成”；
/// 后续 admins/法定代表人接入后,这里可改为真正的结果写入器。
pub trait ElectionVoteResultCallback {
    fn on_election_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError>;

    fn can_cancel_passed_proposal(
        _proposal_id: u64,
    ) -> Result<ProposalCancelDecision, DispatchError> {
        Ok(ProposalCancelDecision::Ignored)
    }

    fn on_execution_failed_terminal(_proposal_id: u64) -> DispatchResult {
        Ok(())
    }
}

impl ElectionVoteResultCallback for () {
    fn on_election_vote_finalized(
        _vote_proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        Ok(ProposalExecutionOutcome::Ignored)
    }
}
