//! 提案业务数据 / Owner / 元数据 / 大对象统一接口。
//!
//! 所有提案在创建后由本入口原子绑定:
//! - `ProposalOwner[id]` = 业务模块 MODULE_TAG(分发自动执行 / 重试 / 取消的归属来源)
//! - `ProposalData[id]` = 业务参数 SCALE 字节(BoundedVec)
//! - `ProposalMeta[id]` = (created_at, passed_at) 辅助元数据
//! - `ProposalObject[id]` / `ProposalObjectMeta[id]` = 大对象(如 runtime wasm)
//!
//! 业务模块只能通过 `register_proposal_data` 在创建阶段写入一次,后续生产路径
//! 不得让 caller 自报 `module_tag` 更新 ProposalData。

use frame_support::ensure;
use frame_support::pallet_prelude::{BoundedVec, DispatchResult};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::Hash as _;
use sp_runtime::DispatchError;

#[cfg(feature = "runtime-benchmarks")]
use crate::pallet::ProposalExecutionRetryStates;
use crate::pallet::{
    self, Error, ProposalData, ProposalDisplayId, ProposalMeta, ProposalObject, ProposalObjectMeta,
    ProposalOwner, ProposalVotePlans, Proposals,
};
#[cfg(feature = "runtime-benchmarks")]
use crate::ExecutionRetryState;
#[cfg(feature = "runtime-benchmarks")]
use crate::STATUS_PASSED;
use crate::{ProposalMetadata, ProposalObjectMetadata};

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 在提案创建事务内一次性绑定业务模块给出的投票计划。
    pub fn bind_vote_plan(
        proposal_id: u64,
        vote_plan: crate::types::VotePlanOf<T::AccountId>,
    ) -> DispatchResult {
        ensure!(
            Proposals::<T>::contains_key(proposal_id),
            Error::<T>::ProposalNotFound
        );
        ensure!(
            !ProposalVotePlans::<T>::contains_key(proposal_id),
            Error::<T>::VotePlanAlreadyBound
        );
        ensure!(
            !ProposalOwner::<T>::contains_key(proposal_id),
            Error::<T>::ProposalDataAlreadyRegistered
        );
        ProposalVotePlans::<T>::insert(proposal_id, vote_plan);
        Ok(())
    }

    fn bounded_module_tag(
        module_tag: &[u8],
    ) -> Result<BoundedVec<u8, T::MaxModuleTagLen>, DispatchError> {
        module_tag
            .to_vec()
            .try_into()
            .map_err(|_| DispatchError::Other("ModuleTagTooLarge"))
    }

    /// 创建提案后原子绑定业务 owner、业务数据和创建区块。
    ///
    /// 业务模块只能通过 `create_*_with_data` 系列入口在创建阶段写入一次。
    /// 后续生产路径不得再让 caller 自报 `module_tag` 更新 ProposalData。
    pub fn register_proposal_data(
        proposal_id: u64,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
        created_at: BlockNumberFor<T>,
    ) -> DispatchResult {
        ensure!(
            Proposals::<T>::contains_key(proposal_id),
            Error::<T>::ProposalNotFound
        );
        ensure!(
            !ProposalOwner::<T>::contains_key(proposal_id)
                && !ProposalData::<T>::contains_key(proposal_id),
            Error::<T>::ProposalDataAlreadyRegistered
        );
        let owner = Self::bounded_module_tag(module_tag)?;
        if let Some(vote_plan) = ProposalVotePlans::<T>::get(proposal_id) {
            ensure!(
                vote_plan.proposal_owner.as_slice() == owner.as_slice(),
                Error::<T>::ProposalOwnerMismatch
            );
        }
        let bounded: BoundedVec<u8, T::MaxProposalDataLen> = data
            .try_into()
            .map_err(|_| DispatchError::Other("ProposalDataTooLarge"))?;
        ProposalOwner::<T>::insert(proposal_id, owner.clone());
        ProposalData::<T>::insert(proposal_id, bounded);
        ProposalMeta::<T>::insert(
            proposal_id,
            ProposalMetadata {
                created_at,
                passed_at: None,
            },
        );

        // 双层 ID：写入 4 张反向索引（institution_code / institution / owner / year）。
        // 索引依赖此时已落地的 Proposals[id](allocate_proposal_id 已写入)与
        // ProposalDisplayId[id](同上)。任一失败,本事务整体回滚。
        let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
        let display =
            ProposalDisplayId::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
        Self::register_proposal_indexes(
            proposal_id,
            proposal.internal_code,
            proposal.actor_cid_number,
            proposal.subject_cid_numbers,
            owner,
            display.year,
        );
        Ok(())
    }

    /// 存储提案业务数据（供 votingengine crate 内部测试与统一登记流程使用）。
    #[cfg(test)]
    pub fn store_proposal_data(proposal_id: u64, data: sp_std::vec::Vec<u8>) -> DispatchResult {
        let bounded: BoundedVec<u8, T::MaxProposalDataLen> = data
            .try_into()
            .map_err(|_| DispatchError::Other("ProposalDataTooLarge"))?;
        ProposalData::<T>::insert(proposal_id, bounded);
        Ok(())
    }

    /// 读取提案业务数据。
    pub fn get_proposal_data(proposal_id: u64) -> Option<sp_std::vec::Vec<u8>> {
        ProposalData::<T>::get(proposal_id).map(|v| v.into_inner())
    }

    /// 判断提案是否由指定业务模块认领。
    ///
    /// 业务 executor 应优先使用 ProposalOwner 判断归属,ProposalData 只承载
    /// 业务参数本体,避免再次把 MODULE_TAG 当作数据前缀依赖。
    pub fn is_proposal_owner(proposal_id: u64, module_tag: &[u8]) -> bool {
        let Some(owner) = ProposalOwner::<T>::get(proposal_id) else {
            return false;
        };
        match Self::bounded_module_tag(module_tag) {
            Ok(expected) => owner == expected,
            Err(_) => false,
        }
    }

    /// Benchmark 专用:把提案切入 PASSED + retry 状态,避免跨 pallet 直接改私有结构。
    #[cfg(feature = "runtime-benchmarks")]
    pub fn force_retryable_passed_for_benchmark(proposal_id: u64) -> DispatchResult {
        Proposals::<T>::try_mutate(proposal_id, |maybe| -> DispatchResult {
            let proposal = maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
            proposal.status = STATUS_PASSED;
            Ok(())
        })?;
        let now = frame_system::Pallet::<T>::block_number();
        ProposalExecutionRetryStates::<T>::insert(
            proposal_id,
            ExecutionRetryState {
                manual_attempts: 0,
                first_auto_failed_at: now,
                retry_deadline: now,
                last_attempt_at: None,
            },
        );
        Ok(())
    }

    /// 存储提案大对象(例如 runtime wasm)。
    pub fn store_proposal_object(
        proposal_id: u64,
        kind: u8,
        data: sp_std::vec::Vec<u8>,
    ) -> DispatchResult {
        let object_len = u32::try_from(data.len())
            .map_err(|_| DispatchError::Other("ProposalObjectTooLarge"))?;
        let object_hash = T::Hashing::hash(&data);
        let bounded: BoundedVec<u8, T::MaxProposalObjectLen> = data
            .try_into()
            .map_err(|_| DispatchError::Other("ProposalObjectTooLarge"))?;
        ProposalObject::<T>::insert(proposal_id, bounded);
        ProposalObjectMeta::<T>::insert(
            proposal_id,
            ProposalObjectMetadata {
                kind,
                object_len,
                object_hash,
            },
        );
        Ok(())
    }

    /// 读取提案大对象原始数据。
    pub fn get_proposal_object(proposal_id: u64) -> Option<sp_std::vec::Vec<u8>> {
        ProposalObject::<T>::get(proposal_id).map(|v| v.into_inner())
    }

    /// 读取提案对象层元数据。
    pub fn get_proposal_object_meta(proposal_id: u64) -> Option<ProposalObjectMetadata<T::Hash>> {
        ProposalObjectMeta::<T>::get(proposal_id)
    }

    /// 删除提案对象层数据与元数据。
    #[cfg(test)]
    pub fn remove_proposal_object(proposal_id: u64) {
        ProposalObject::<T>::remove(proposal_id);
        ProposalObjectMeta::<T>::remove(proposal_id);
    }

    /// 存储提案辅助元数据(创建时间)。
    #[cfg(test)]
    pub fn store_proposal_meta(proposal_id: u64, created_at: BlockNumberFor<T>) {
        ProposalMeta::<T>::insert(
            proposal_id,
            ProposalMetadata {
                created_at,
                passed_at: None,
            },
        );
    }

    /// 标记提案通过时间。
    #[cfg(test)]
    pub fn set_proposal_passed(proposal_id: u64, block: BlockNumberFor<T>) {
        Self::mark_proposal_passed_at(proposal_id, block);
    }

    /// 读取提案辅助元数据。
    pub fn get_proposal_meta(proposal_id: u64) -> Option<ProposalMetadata<BlockNumberFor<T>>> {
        ProposalMeta::<T>::get(proposal_id)
    }
}
