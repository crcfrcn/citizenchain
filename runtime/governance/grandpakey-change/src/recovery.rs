//! 旧 GRANDPA 私钥不可用时的机构内部投票恢复。

use frame_support::ensure;
use sp_std::vec::Vec;
use votingengine::types::{
    AuthorizationSubject, BusinessActionId, RoleSubject, VotePlanOf, VotingEngineKind,
};

use crate::{
    pallet::{Config, Error, Pallet},
    CidNumber, RoleCode,
};

impl<T: Config> Pallet<T> {
    pub(crate) fn build_emergency_vote_plan(
        who: &T::AccountId,
        actor_cid_number: &[u8],
        proposer_role_code: &[u8],
        encoded_action: &[u8],
    ) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
        use entity_primitives::{InstitutionRoleAuthorizationQuery, RolePermissionOperation};

        let action_code = entity_primitives::business_action::ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY;
        let action_id = BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code,
        };
        let proposer = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: proposer_role_code.to_vec(),
        };
        ensure!(
            T::InstitutionRoleAuthorization::is_authorized(
                who,
                &proposer,
                &action_id,
                RolePermissionOperation::Propose,
            ),
            Error::<T>::UnauthorizedAdmin
        );

        let voter_subjects = T::InstitutionRoleAuthorization::role_subjects_with_permission(
            actor_cid_number,
            &action_id,
            RolePermissionOperation::Vote,
        )
        .into_iter()
        .map(|role| {
            Ok(AuthorizationSubject::Institution(RoleSubject {
                cid_number: CidNumber::try_from(role.cid_number)
                    .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                role_code: RoleCode::try_from(role.role_code)
                    .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
            }))
        })
        .collect::<Result<Vec<_>, sp_runtime::DispatchError>>()?;
        ensure!(!voter_subjects.is_empty(), Error::<T>::UnauthorizedAdmin);

        let owner: frame_support::BoundedVec<
            u8,
            frame_support::traits::ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
        > = crate::MODULE_TAG
            .to_vec()
            .try_into()
            .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
        VotePlanOf::<T::AccountId>::try_new(
            BusinessActionId {
                module_tag: owner.clone(),
                action_code,
            },
            owner,
            AuthorizationSubject::Institution(RoleSubject {
                cid_number: CidNumber::try_from(actor_cid_number.to_vec())
                    .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                role_code: RoleCode::try_from(proposer_role_code.to_vec())
                    .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
            }),
            voter_subjects,
            VotingEngineKind::Internal,
            sp_crypto_hashing::blake2_256(encoded_action),
        )
        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan.into())
    }

    pub(crate) fn authorize_routine_rotation(
        who: &T::AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
    ) -> Result<(), Error<T>> {
        use entity_primitives::{InstitutionRoleAuthorizationQuery, RolePermissionOperation};

        let subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        let action_id = entity_primitives::BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code: entity_primitives::business_action::ACTION_GRANDPA_KEY_ROTATION,
        };
        ensure!(
            T::InstitutionRoleAuthorization::is_authorized(
                who,
                &subject,
                &action_id,
                RolePermissionOperation::Propose,
            ),
            Error::<T>::UnauthorizedAdmin
        );
        Ok(())
    }
}
