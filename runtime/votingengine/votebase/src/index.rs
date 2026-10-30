//! 提案反向索引。
//!
//! 链上 4 张反向索引让客户端按"分类"O(分类内规模)迭代提案,不用扫全表:
//! - **`ProposalsByCode[institution_code][id]`** — 按 CID 机构码(固定治理档 / PMUL / 公权法人 / 私权法人)反查
//! - **`ProposalsByCid[cid_number][id]`** — 按机构唯一 CID 反查
//!   (如某省储行、某公司、某银行的所有关联提案)
//! - **`ProposalsByOwner[module_tag][id]`** — 按业务模块 MODULE_TAG 反查
//!   (如"只看 runtime 升级提案"、"只看决议销毁提案")
//! - **`ProposalsByYear[year][id]`** — 按创建年份反查(历史归档视图)
//!
//! **写入时机**:`register_proposal_data` 末尾(提案对外业务数据落地后立刻写)。
//! **删除时机**:`cleanup_proposal_indexes`(终态/90 天保留期清理路径)。

use frame_support::pallet_prelude::BoundedVec;

use crate::pallet::{
    self, ProposalDisplayId, ProposalOwner, Proposals, ProposalsByCid, ProposalsByCode,
    ProposalsByOwner, ProposalsByYear,
};
use crate::types::{InstitutionCode, ProposalSubjectCidNumbers};
impl<T: pallet::Config> pallet::Pallet<T> {
    /// 写入四张反向索引。
    ///
    /// 由 `register_proposal_data` 在创建阶段同事务调用,保证:
    /// - 任一提案写入 `ProposalData` 后,4 张反向索引立即可查
    /// - 不写 `ProposalData` 的占位提案(仅创建阶段失败回滚的)不会污染索引
    pub fn register_proposal_indexes(
        proposal_id: u64,
        institution_code: Option<InstitutionCode>,
        actor_cid_number: Option<crate::types::CidNumber>,
        subject_cid_numbers: ProposalSubjectCidNumbers,
        module_tag: BoundedVec<u8, T::MaxModuleTagLen>,
        year: u16,
    ) {
        if let Some(institution_code) = institution_code {
            ProposalsByCode::<T>::insert(institution_code, proposal_id, ());
        }
        if let Some(cid_number) = actor_cid_number {
            ProposalsByCid::<T>::insert(cid_number, proposal_id, ());
        }
        for cid_number in subject_cid_numbers {
            ProposalsByCid::<T>::insert(cid_number, proposal_id, ());
        }
        ProposalsByOwner::<T>::insert(module_tag, proposal_id, ());
        ProposalsByYear::<T>::insert(year, proposal_id, ());
    }

    /// 释放该提案在 4 张反向索引中的所有条目 + ProposalDisplayId。
    ///
    /// 由清理路径(`cleanup` / 90 天保留期)调用。需要从 `Proposals` /
    /// `ProposalOwner` / `ProposalDisplayId` 反查 (institution_code, subject CID, owner, year),
    /// 顺序无关紧要,所有 4 张索引 + 展示号表一次清干净。
    ///
    /// **必须在 `Proposals[id]` / `ProposalOwner[id]` / `ProposalDisplayId[id]`
    /// 自身被删除之前调用**(否则反查不到分类键无法清索引)。
    pub fn cleanup_proposal_indexes(proposal_id: u64) {
        if let Some(proposal) = Proposals::<T>::get(proposal_id) {
            if let Some(institution_code) = proposal.internal_code {
                ProposalsByCode::<T>::remove(institution_code, proposal_id);
            }
            if let Some(cid_number) = proposal.actor_cid_number {
                ProposalsByCid::<T>::remove(cid_number, proposal_id);
            }
            for cid_number in proposal.subject_cid_numbers {
                ProposalsByCid::<T>::remove(cid_number, proposal_id);
            }
        }
        if let Some(owner) = ProposalOwner::<T>::get(proposal_id) {
            ProposalsByOwner::<T>::remove(owner, proposal_id);
        }
        if let Some(meta) = ProposalDisplayId::<T>::get(proposal_id) {
            ProposalsByYear::<T>::remove(meta.year, proposal_id);
        }
        ProposalDisplayId::<T>::remove(proposal_id);
    }
}
