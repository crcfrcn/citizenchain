//! 个人多签管理员快照与机构岗位投票人快照。
//!
//! 机构提案创建时按 `VotePlan` 锁定每个岗位主体的有效任职账户，并按 CID
//! 累加岗位票据总数；投票期间不随后续任职变化。`AdminSnapshot` 只供独立个人多签路径
//! 使用，不得用于机构投票资格判定。
//!
//! - `is_admin_in_snapshot`:查个人多签快照判断某账户是否为冻结管理员
//! - `snapshot_admins_len`:个人多签快照中的管理员数量
//! - `snapshot_role_voters`:按完整岗位主体写入任职快照和 CID 岗位票据总数

use frame_support::pallet_prelude::{BoundedVec, DispatchResult};

use crate::pallet::{
    self, AdminSnapshot, Error, InstitutionTicketCountSnapshot, ProposalVotePlans, VoterSnapshot,
};
use crate::types::{AuthorizationSubject, CidNumber, ProposalSubject};
use crate::InternalAdminProvider;

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 把投票人钱包解析为名册规范账户（= 快照键）。
    ///
    /// 机构主体走 `resolve_institution_voter`：运行期该管理员若带公民 CID，只认该 CID 当前
    /// 绑定的钱包（换绑后新钱包解析到同一规范账户，旧钱包得 `None`）；创世期或无 CID 管理员
    /// 按 account_id。个人多签无 CID 身份，账户即身份，恒按账户。
    ///
    /// **必须传入原始投票人钱包**：绝不可对已解析的规范账户二次解析（运行期换绑后
    /// `resolve(旧规范账户)` 为 `None`，二次解析会误杀合法投票人）。
    pub fn resolve_subject_voter(
        subject: &AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> Option<T::AccountId> {
        match subject {
            AuthorizationSubject::Institution(role_subject) => {
                T::InternalAdminProvider::resolve_institution_voter(
                    role_subject.cid_number.as_slice(),
                    who,
                )
            }
            AuthorizationSubject::PersonalMultisig(_) => Some(who.clone()),
        }
    }

    /// 查询某完整岗位主体冻结的投票人名单。
    pub fn is_subject_voter_in_snapshot(
        proposal_id: u64,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> bool {
        let Some(voters) = VoterSnapshot::<T>::get(proposal_id, &subject) else {
            return false;
        };
        // 解析原始钱包 → 规范账户；None = 运行期已非当前管理员（含换绑后的旧钱包）→ 拒。
        let Some(canonical) = Self::resolve_subject_voter(&subject, who) else {
            return false;
        };
        voters.iter().any(|account| account == &canonical)
    }

    /// 查询某个完整岗位主体的冻结选民人数。
    pub fn subject_voters_len(
        proposal_id: u64,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
    ) -> Option<u32> {
        VoterSnapshot::<T>::get(proposal_id, subject).map(|voters| voters.len() as u32)
    }

    /// 查询某机构冻结的岗位票据总数。
    pub fn institution_ticket_count(proposal_id: u64, cid_number: CidNumber) -> Option<u32> {
        InstitutionTicketCountSnapshot::<T>::get(proposal_id, cid_number)
    }

    /// 判断账户是否持有提案任一冻结机构岗位，仅用于重试、取消等非记票权限。
    pub fn is_any_institution_voter_in_snapshot(proposal_id: u64, who: &T::AccountId) -> bool {
        ProposalVotePlans::<T>::get(proposal_id)
            .map(|plan| {
                plan.voter_subjects.iter().any(|subject| {
                    matches!(subject, AuthorizationSubject::Institution(_))
                        && Self::is_subject_voter_in_snapshot(proposal_id, subject.clone(), who)
                })
            })
            .unwrap_or(false)
    }

    /// 冻结一个完整岗位主体的当前有效任职账户，并累加该机构的岗位票据总数。
    pub fn snapshot_role_voters(
        proposal_id: u64,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        voters: sp_std::vec::Vec<T::AccountId>,
    ) -> DispatchResult {
        let institution_cid = match &subject {
            AuthorizationSubject::Institution(role_subject) => role_subject.cid_number.clone(),
            AuthorizationSubject::PersonalMultisig(_) => {
                return Err(Error::<T>::InvalidVotePlan.into())
            }
        };
        frame_support::ensure!(
            !VoterSnapshot::<T>::contains_key(proposal_id, &subject),
            Error::<T>::VotePlanAlreadyBound
        );
        Self::ensure_valid_voter_snapshot(voters.as_slice())?;
        let bounded = BoundedVec::<T::AccountId, T::MaxAdminsPerInstitution>::try_from(voters)
            .map_err(|_| Error::<T>::InvalidInstitution)?;

        let current = InstitutionTicketCountSnapshot::<T>::get(proposal_id, &institution_cid)
            .unwrap_or_default();
        let updated = current
            .checked_add(bounded.len() as u32)
            .ok_or(Error::<T>::InvalidInstitution)?;
        VoterSnapshot::<T>::insert(proposal_id, subject, bounded);
        InstitutionTicketCountSnapshot::<T>::insert(proposal_id, institution_cid, updated);
        Ok(())
    }

    /// 查询账户是否在指定个人多签主体的冻结管理员快照中。
    pub fn is_admin_in_snapshot(
        proposal_id: u64,
        subject: ProposalSubject<T::AccountId>,
        who: &T::AccountId,
    ) -> bool {
        AdminSnapshot::<T>::get(proposal_id, subject)
            .map(|admins| admins.iter().any(|a| a == who))
            .unwrap_or(false)
    }

    /// 查询指定个人多签主体的冻结管理员数量。
    pub fn snapshot_admins_len(
        proposal_id: u64,
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

    fn ensure_valid_voter_snapshot(voters: &[T::AccountId]) -> DispatchResult {
        frame_support::ensure!(!voters.is_empty(), Error::<T>::MissingVoterSnapshot);
        for i in 0..voters.len() {
            for j in i.saturating_add(1)..voters.len() {
                frame_support::ensure!(voters[i] != voters[j], Error::<T>::InvalidInstitution);
            }
        }
        Ok(())
    }

    /// 将个人多签当前或待注册管理员列表写入快照。
    pub fn snapshot_personal_admins(
        proposal_id: u64,
        personal_account_id: T::AccountId,
        pending_account: bool,
    ) -> DispatchResult {
        let admins = if pending_account {
            T::InternalAdminProvider::get_pending_personal_admins(personal_account_id.clone())
        } else {
            T::InternalAdminProvider::get_personal_admins(personal_account_id.clone())
        }
        .ok_or(Error::<T>::InvalidInstitution)?;

        Self::write_admin_snapshot(
            proposal_id,
            ProposalSubject::PersonalAccount(personal_account_id),
            admins,
        )
    }

    fn write_admin_snapshot(
        proposal_id: u64,
        subject: ProposalSubject<T::AccountId>,
        admins: sp_std::vec::Vec<T::AccountId>,
    ) -> DispatchResult {
        Self::ensure_valid_admin_snapshot(admins.as_slice())?;

        match BoundedVec::<T::AccountId, T::MaxAdminsPerInstitution>::try_from(admins) {
            Ok(bounded) => {
                AdminSnapshot::<T>::insert(proposal_id, subject, bounded);
                Ok(())
            }
            Err(_) => {
                frame_support::defensive!(
                    "write_admin_snapshot: personal admin list exceeds MaxAdminsPerInstitution, snapshot not written"
                );
                Err(Error::<T>::InvalidInstitution.into())
            }
        }
    }
}
