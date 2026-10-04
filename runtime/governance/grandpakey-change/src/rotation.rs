//! GRANDPA authority set 更换与生效确认。

use codec::Encode;
use curve25519_dalek::edwards::CompressedEdwardsY;
use frame_support::{ensure, traits::Get};
use sp_consensus_grandpa::{AuthorityId as GrandpaAuthorityId, AuthorityList};
use sp_core::ed25519;
use sp_runtime::traits::Saturating;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

use crate::{
    pallet::{
        Config, CurrentGrandpaKeys, Error, GrandpaKeyOwnerByKey, Pallet, PendingGrandpaKeyChange,
        ReservedGrandpaKeys,
    },
    CidNumber, GrandpaKeyChangeKind, PendingGrandpaKeyChangeState,
};

impl<T: Config> Pallet<T> {
    pub(crate) fn validate_public_key(new_public_key: [u8; 32]) -> Result<(), Error<T>> {
        ensure!(new_public_key != [0u8; 32], Error::<T>::NewKeyIsZero);
        let point = CompressedEdwardsY(new_public_key)
            .decompress()
            .ok_or(Error::<T>::InvalidEd25519Key)?;
        ensure!(!point.is_small_order(), Error::<T>::InvalidEd25519Key);
        Ok(())
    }

    pub(crate) fn current_key_and_next_authorities(
        actor_cid_number: &CidNumber,
        new_public_key: [u8; 32],
        allow_matching_reservation: bool,
    ) -> Result<([u8; 32], AuthorityList), Error<T>> {
        ensure!(
            PendingGrandpaKeyChange::<T>::get().is_none()
                && pallet_grandpa::Pallet::<T>::pending_change().is_none(),
            Error::<T>::GrandpaChangePending
        );
        Self::validate_public_key(new_public_key)?;

        let old_public_key = CurrentGrandpaKeys::<T>::get(actor_cid_number.clone())
            .ok_or(Error::<T>::CurrentGrandpaKeyNotFound)?;
        ensure!(
            old_public_key != new_public_key,
            Error::<T>::NewKeyUnchanged
        );
        ensure!(
            GrandpaKeyOwnerByKey::<T>::get(new_public_key)
                .map(|owner| owner == *actor_cid_number)
                .unwrap_or(true),
            Error::<T>::NewKeyAlreadyUsed
        );
        match ReservedGrandpaKeys::<T>::get(new_public_key) {
            None => {}
            Some(owner) if allow_matching_reservation && owner == *actor_cid_number => {}
            Some(_) => return Err(Error::<T>::NewKeyAlreadyReserved),
        }

        let old_authority = GrandpaAuthorityId::from(ed25519::Public::from_raw(old_public_key));
        let new_authority = GrandpaAuthorityId::from(ed25519::Public::from_raw(new_public_key));
        let mut found = false;
        let next_authorities = pallet_grandpa::Pallet::<T>::grandpa_authorities()
            .into_iter()
            .map(|(authority, weight)| {
                if authority == old_authority {
                    found = true;
                    (new_authority.clone(), weight)
                } else {
                    (authority, weight)
                }
            })
            .collect::<Vec<_>>();
        ensure!(found, Error::<T>::OldAuthorityNotFound);

        let mut unique = BTreeSet::new();
        ensure!(
            next_authorities
                .iter()
                .all(|(authority, _)| unique.insert(authority.encode())),
            Error::<T>::NewKeyAlreadyUsed
        );
        Ok((old_public_key, next_authorities))
    }

    /// 参数逐项绑定已经验证的机构、旧新公钥和治理来源，避免调度阶段重新推断。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn schedule_replacement(
        actor_cid_number: CidNumber,
        initiator_account_id: T::AccountId,
        old_public_key: [u8; 32],
        new_public_key: [u8; 32],
        proof_nonce: u64,
        change_kind: GrandpaKeyChangeKind,
        proposal_id: Option<u64>,
        next_authorities: AuthorityList,
    ) -> Result<
        PendingGrandpaKeyChangeState<T::AccountId, frame_system::pallet_prelude::BlockNumberFor<T>>,
        sp_runtime::DispatchError,
    > {
        let current_set_id = pallet_grandpa::Pallet::<T>::current_set_id();
        let expected_set_id = current_set_id
            .checked_add(1)
            .ok_or(Error::<T>::SetIdOverflow)?;
        let scheduled_at = frame_system::Pallet::<T>::block_number();
        let activate_at = scheduled_at.saturating_add(T::GrandpaChangeDelay::get());

        pallet_grandpa::Pallet::<T>::schedule_change(
            next_authorities,
            T::GrandpaChangeDelay::get(),
            None,
        )?;

        let pending = PendingGrandpaKeyChangeState {
            actor_cid_number: actor_cid_number.clone(),
            initiator_account_id,
            old_public_key,
            new_public_key,
            proof_nonce,
            change_kind,
            proposal_id,
            expected_set_id,
            scheduled_at,
            activate_at,
        };
        PendingGrandpaKeyChange::<T>::put(pending.clone());
        ReservedGrandpaKeys::<T>::insert(new_public_key, actor_cid_number);
        Ok(pending)
    }

    /// 只在 `pallet-grandpa` 已实际切换 authority set 后更新治理映射。
    pub(crate) fn reconcile_pending_change() -> bool {
        let Some(pending) = PendingGrandpaKeyChange::<T>::get() else {
            return false;
        };
        let old_authority =
            GrandpaAuthorityId::from(ed25519::Public::from_raw(pending.old_public_key));
        let new_authority =
            GrandpaAuthorityId::from(ed25519::Public::from_raw(pending.new_public_key));
        let authorities = pallet_grandpa::Pallet::<T>::grandpa_authorities();
        let new_is_active = authorities
            .iter()
            .any(|(authority, _)| authority == &new_authority);
        let old_is_active = authorities
            .iter()
            .any(|(authority, _)| authority == &old_authority);
        if !new_is_active || old_is_active {
            return false;
        }

        CurrentGrandpaKeys::<T>::insert(pending.actor_cid_number.clone(), pending.new_public_key);
        GrandpaKeyOwnerByKey::<T>::remove(pending.old_public_key);
        GrandpaKeyOwnerByKey::<T>::insert(pending.new_public_key, pending.actor_cid_number.clone());
        ReservedGrandpaKeys::<T>::remove(pending.new_public_key);
        PendingGrandpaKeyChange::<T>::kill();
        Self::deposit_event(crate::pallet::Event::<T>::GrandpaKeyActivated {
            actor_cid_number: pending.actor_cid_number,
            old_public_key: pending.old_public_key,
            new_public_key: pending.new_public_key,
            change_kind: pending.change_kind,
            expected_set_id: pending.expected_set_id,
        });
        true
    }
}
