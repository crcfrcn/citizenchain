//! 冻结个人管理员及机构岗位投票人快照，提供投票资格和票据查询。

use frame_support::{
    pallet_prelude::{BoundedVec, DispatchResult},
    traits::Get,
    weights::Weight,
};

use crate::pallet::{
    self, AdminSnapshot, Error, FrozenGovernanceWalletState, FrozenWalletVersions, InstitutionTicketCountSnapshot, ProposalVotePlans, VoterSnapshot,
};
use crate::types::{AuthorizationSubject, CidNumber, ProposalSubject};
use crate::InternalAdminProvider;

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 冻结版本读取的保守附加预算，非新实测基准；个人票不使用。
    pub fn frozen_vote_weight() -> Weight {
        Weight::from_parts(
            1_000_000_000,
            u64::from(T::MaxAdminsPerInstitution::get())
                .saturating_mul(40)
                .saturating_add(8192),
        )
        .saturating_add(T::DbWeight::get().reads(2))
    }
    /// 机构治理建案的批量钱包查询及版本写入附加预算，按实际名单/岗位数估价。
    pub fn frozen_snapshot_weight(wallets: u32, subjects: u32) -> Weight {
        if subjects == 0 {
            return Weight::zero();
        }
        Weight::from_parts(1_000_000_000, 4000)
            .saturating_mul(subjects.into())
            .saturating_add(
                Weight::from_parts(10_000_000, 40)
                    .saturating_mul(u64::from(wallets).saturating_mul(subjects.into())),
            )
            .saturating_add(Weight::from_parts(0, 6000).saturating_mul(wallets.into()))
            .saturating_add(
                T::DbWeight::get().reads_writes(
                    (u64::from(wallets).saturating_mul(2).saturating_add(3))
                        .saturating_mul(subjects.into()),
                    subjects.into(),
                ),
            )
    }

    /// 同样只供受信任的创建/历史事实补齐流程调用，无外部extrinsic，不从当前状态兜底。
    pub fn record_governance_wallet_state(proposal_id: u128, state: [u8; 32]) -> DispatchResult {
        frame_support::ensure!(
            crate::Proposals::<T>::contains_key(proposal_id),
            Error::<T>::ProposalNotFound
        );
        frame_support::ensure!(
            !FrozenGovernanceWalletState::<T>::contains_key(proposal_id),
            Error::<T>::VotePlanAlreadyBound
        );
        FrozenGovernanceWalletState::<T>::insert(proposal_id, state);
        Ok(())
    }

    /// 原签名钱包必须直接属于该提案的冻结身份；不解析现任名单或个人CID。
    /// 票据版本随提案冻结，保持旧票据的SCALE编码和已投状态不变。
    pub fn frozen_voter_identity(
        proposal_id: u128,
        subject: &AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> Option<(T::AccountId, u64)> {
        if !matches!(subject, AuthorizationSubject::Institution(_)) {
            return None;
        }
        let voters = VoterSnapshot::<T>::get(proposal_id, subject)?;
        let versions = FrozenWalletVersions::<T>::get(proposal_id, subject)?;
        if voters.len() != versions.len()
            || !voters
                .iter()
                .zip(versions.iter())
                .all(|(wallet, (recorded, _))| wallet == recorded)
        {
            return None;
        }
        versions.into_iter().find(|(wallet, _)| wallet == who)
    }

    pub fn resolve_snapshot_voter(
        proposal_id: u128,
        subject: &AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> Option<T::AccountId> {
        Self::frozen_voter_identity(proposal_id, subject, who).map(|(wallet, _)| wallet)
    }

    pub fn is_subject_voter_in_snapshot(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        who: &T::AccountId,
    ) -> bool {
        Self::frozen_voter_identity(proposal_id, &subject, who).is_some()
    }

    /// 受信任的一次性资料补齐原语，无外部extrinsic。调用方须提供创建时的已核实版本。
    /// 名单必须与原快照逐项完全一致，只能补写一次，不改名单、分母、票数或投票记录。
    pub fn record_frozen_wallet_versions(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
        records: sp_std::vec::Vec<(T::AccountId, u64)>,
    ) -> DispatchResult {
        frame_support::ensure!(
            matches!(subject, AuthorizationSubject::Institution(_)),
            Error::<T>::InvalidVotePlan
        );
        let voters = VoterSnapshot::<T>::get(proposal_id, &subject)
            .ok_or(Error::<T>::MissingVoterSnapshot)?;
        Self::ensure_valid_voter_snapshot(&voters)?;
        frame_support::ensure!(
            !FrozenWalletVersions::<T>::contains_key(proposal_id, &subject),
            Error::<T>::VotePlanAlreadyBound
        );
        frame_support::ensure!(
            voters.len() == records.len()
                && voters.iter().zip(records.iter()).all(|(w, (r, _))| w == r),
            Error::<T>::InvalidInstitution
        );
        let records = BoundedVec::<_, T::MaxAdminsPerInstitution>::try_from(records)
            .map_err(|_| Error::<T>::InvalidInstitution)?;
        FrozenWalletVersions::<T>::insert(proposal_id, subject, records);
        Ok(())
    }

    /// 查询某个完整岗位主体的冻结选民人数。
    pub fn subject_voters_len(
        proposal_id: u128,
        subject: AuthorizationSubject<CidNumber, crate::types::RoleCode, T::AccountId>,
    ) -> Option<u32> {
        VoterSnapshot::<T>::get(proposal_id, subject).map(|voters| voters.len() as u32)
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

    /// 冻结一个完整岗位主体的当前有效任职账户，并累加该机构的岗位票据总数。
    pub fn snapshot_role_voters(
        proposal_id: u128,
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
        frame_support::ensure!(
            !FrozenWalletVersions::<T>::contains_key(proposal_id, &subject),
            Error::<T>::VotePlanAlreadyBound
        );
        let records =
            T::InternalAdminProvider::institution_wallet_snapshot(&institution_cid, &voters)
                .ok_or(Error::<T>::MissingVoterSnapshot)?;
        frame_support::ensure!(
            records.len() == voters.len(),
            Error::<T>::InvalidInstitution
        );
        let wallets = records
            .iter()
            .map(|(wallet, _)| wallet.clone())
            .collect::<sp_std::vec::Vec<_>>();
        Self::ensure_valid_voter_snapshot(&wallets)?;
        let bounded = BoundedVec::<T::AccountId, T::MaxAdminsPerInstitution>::try_from(wallets)
            .map_err(|_| Error::<T>::InvalidInstitution)?;
        let versions = BoundedVec::<_, T::MaxAdminsPerInstitution>::try_from(records)
            .map_err(|_| Error::<T>::InvalidInstitution)?;

        let current = InstitutionTicketCountSnapshot::<T>::get(proposal_id, &institution_cid)
            .unwrap_or_default();
        let updated = current
            .checked_add(bounded.len() as u32)
            .ok_or(Error::<T>::InvalidInstitution)?;
        FrozenWalletVersions::<T>::insert(proposal_id, &subject, versions);
        VoterSnapshot::<T>::insert(proposal_id, subject, bounded);
        InstitutionTicketCountSnapshot::<T>::insert(proposal_id, institution_cid, updated);
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
        proposal_id: u128,
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
        proposal_id: u128,
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
