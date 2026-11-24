//! 冻结个人管理员及机构岗位投票人快照，提供投票资格和票据查询。

use frame_support::{
    pallet_prelude::{BoundedVec, DispatchResult},
    traits::Get,
    weights::Weight,
};

use crate::pallet::{
    self, AdminSnapshot, Error, InstitutionTicketCountSnapshot, ProposalVotePlans, RoleVoters,
};
use crate::types::{AuthorizationSubject, CidNumber, ProposalSubject};
use crate::{CitizenIdentityReader, InternalVoteProvider};

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 冻结资格及当前人员绑定查询的保守附加预算，非新实测基准；个人票不使用。
    pub fn frozen_vote_weight() -> Weight {
        Weight::from_parts(
            1_000_000_000,
            u64::from(T::MaxSnapshotVoters::get())
                .saturating_mul(40)
                .saturating_add(8192),
        )
        .saturating_add(T::DbWeight::get().reads(4))
    }
    /// 机构建案的人员绑定与岗位快照预算。
    pub fn frozen_snapshot_weight(persons: u32, subjects: u32) -> Weight {
        if subjects == 0 {
            return Weight::zero();
        }
        Weight::from_parts(1_000_000_000, 4000)
            .saturating_mul(subjects.into())
            .saturating_add(
                Weight::from_parts(10_000_000, 40)
                    .saturating_mul(u64::from(persons).saturating_mul(subjects.into())),
            )
            .saturating_add(Weight::from_parts(0, 6000).saturating_mul(persons.into()))
            .saturating_add(
                T::DbWeight::get().reads_writes(
                    (u64::from(persons).saturating_mul(3).saturating_add(3))
                        .saturating_mul(subjects.into()),
                    subjects.into(),
                ),
            )
    }

    /// 解析签名钱包的当前人员 CID，并验证该人员属于指定提案和机构岗位。
    pub fn resolve_snapshot_voter(
        proposal_id: u128,
        subject: &AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> Option<CidNumber> {
        let AuthorizationSubject::Institution(role) = subject else {
            return None;
        };
        let person_cid = T::CitizenIdentityReader::person_cid(who)?;
        let persons = RoleVoters::<T>::get(proposal_id, role)?;
        persons.binary_search(&person_cid).is_ok().then_some(person_cid)
    }

    pub fn is_subject_voter_in_snapshot(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> bool {
        Self::resolve_snapshot_voter(proposal_id, &subject, who).is_some()
    }

    /// 查询指定岗位的冻结人员数量。
    pub fn subject_voters_len(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
    ) -> Option<u32> {
        let AuthorizationSubject::Institution(role) = subject else {
            return None;
        };
        RoleVoters::<T>::decode_len(proposal_id, role).map(|len| len as u32)
    }

    /// 查询某机构冻结的岗位票据总数。
    pub fn institution_ticket_count(proposal_id: u128, cid_number: CidNumber) -> Option<u32> {
        InstitutionTicketCountSnapshot::<T>::get(proposal_id, cid_number)
    }

    /// 判断账户是否持有提案任一冻结机构岗位，仅用于重试、取消等非记票权限。
    pub fn is_any_institution_voter_in_snapshot(proposal_id: u128, who: &T::AccountId) -> bool {
        ProposalVotePlans::<T>::get(proposal_id)
            .map(|plan| {
                plan.voter_subjects.iter().any(|subject| {
                    matches!(subject, AuthorizationSubject::Institution(_))
                        && Self::is_subject_voter_in_snapshot(proposal_id, subject.clone(), who)
                })
            })
            .unwrap_or(false)
    }

    /// 按机构和岗位冻结人员名单，整案拒绝缺绑定、重复或超限的人员。
    pub fn snapshot_role_voters(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        voters: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
    ) -> DispatchResult {
        let AuthorizationSubject::Institution(role) = subject else {
            return Err(Error::<T>::InvalidVotePlan.into());
        };
        frame_support::ensure!(!RoleVoters::<T>::contains_key(proposal_id, &role), Error::<T>::VotePlanAlreadyBound);
        frame_support::ensure!(!voters.is_empty(), Error::<T>::MissingVoterSnapshot);
        frame_support::ensure!(voters.len() <= T::MaxSnapshotVoters::get() as usize, Error::<T>::InvalidInstitution);
        let mut persons = voters.into_iter().map(|raw| {
            let cid = CidNumber::try_from(raw).map_err(|_| Error::<T>::InvalidInstitution)?;
            T::CitizenIdentityReader::person_account(&cid).ok_or(Error::<T>::MissingVoterSnapshot)?;
            Ok::<_, Error<T>>(cid)
        }).collect::<Result<sp_std::vec::Vec<_>, _>>()?;
        persons.sort_unstable();
        frame_support::ensure!(persons.windows(2).all(|pair| pair[0] != pair[1]), Error::<T>::InvalidInstitution);
        let persons = BoundedVec::<CidNumber, T::MaxSnapshotVoters>::try_from(persons)
            .map_err(|_| Error::<T>::InvalidInstitution)?;
        let current = InstitutionTicketCountSnapshot::<T>::get(proposal_id, &role.cid_number).unwrap_or_default();
        let updated = current.checked_add(persons.len() as u32).ok_or(Error::<T>::InvalidInstitution)?;
        InstitutionTicketCountSnapshot::<T>::insert(proposal_id, &role.cid_number, updated);
        RoleVoters::<T>::insert(proposal_id, role, persons);
        Ok(())
    }

    /// 查询账户是否在指定个人多签主体的冻结管理员快照中。
    pub fn is_admin_in_snapshot(
        proposal_id: u128,
        subject: ProposalSubject<T::AccountId>,
        who: &T::AccountId,
    ) -> bool {
        AdminSnapshot::<T>::get(proposal_id, subject)
            .map(|admins| admins.iter().any(|a| a == who))
            .unwrap_or(false)
    }

    /// 查询指定个人多签主体的冻结管理员数量。
    pub fn snapshot_admins_len(
        proposal_id: u128,
        subject: ProposalSubject<T::AccountId>,
    ) -> Option<u32> {
        AdminSnapshot::<T>::get(proposal_id, subject).map(|admins| admins.len() as u32)
    }

    fn ensure_valid_admin_snapshot(admins: &[T::AccountId]) -> DispatchResult {
        // 内部投票一旦创建就只认快照；空快照会导致提案无人可投，
        // 重复管理员会破坏“一管理员一票”的票权语义，所以必须在写快照前拒绝。
        frame_support::ensure!(!admins.is_empty(), Error::<T>::MissingAdminSnapshot);
        for i in 0..admins.len() {
            for j in i.saturating_add(1)..admins.len() {
                frame_support::ensure!(admins[i] != admins[j], Error::<T>::InvalidInstitution);
            }
        }
        Ok(())
    }

    /// 将个人多签当前或待注册管理员列表写入快照。
    pub fn snapshot_personal_admins(
        proposal_id: u128,
        personal_account_id: T::AccountId,
        pending_account: bool,
    ) -> DispatchResult {
        let admins = if pending_account {
            T::InternalVoteProvider::get_pending_personal_admins(personal_account_id.clone())
        } else {
            T::InternalVoteProvider::get_personal_admins(personal_account_id.clone())
        }
        .ok_or(Error::<T>::InvalidInstitution)?;

        Self::write_admin_snapshot(
            proposal_id,
            ProposalSubject::PersonalAccount(personal_account_id),
            admins,
        )
    }

    fn write_admin_snapshot(
        proposal_id: u128,
        subject: ProposalSubject<T::AccountId>,
        admins: sp_std::vec::Vec<T::AccountId>,
    ) -> DispatchResult {
        Self::ensure_valid_admin_snapshot(admins.as_slice())?;

        match BoundedVec::<
            T::AccountId,
            frame_support::traits::ConstU32<{ primitives::multisig::MAX_PERSONAL_ADMINS }>,
        >::try_from(admins) {
            Ok(bounded) => {
                AdminSnapshot::<T>::insert(proposal_id, subject, bounded);
                Ok(())
            }
            Err(_) => {
                frame_support::defensive!(
                    "write_admin_snapshot: personal admin list exceeds MAX_PERSONAL_ADMINS, snapshot not written"
                );
                Err(Error::<T>::InvalidInstitution.into())
            }
        }
    }
}
