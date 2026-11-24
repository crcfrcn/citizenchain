//! GRANDPA 验证密钥正常更换与紧急恢复发起入口基准。

#![cfg(feature = "runtime-benchmarks")]

use ed25519_dalek::{Signer, SigningKey};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_consensus_grandpa::AuthorityId as GrandpaAuthorityId;
use sp_core::ed25519;
use sp_runtime::traits::{SaturatedConversion, Saturating};
use sp_std::vec;

use crate::{
    pallet, proof, Call, Config, CurrentGrandpaKeys, GrandpaKeyChangeKind, GrandpaKeyOwnerByKey,
    Pallet, CHINA_CB,
};

fn nrc_cid() -> engingcore::types::CidNumber {
    CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits")
}

fn nrc_admin<T: pallet::Config>() -> T::AccountId {
    frame_benchmarking::account(CHINA_CB[0].cid_number, 0, 0)
}

fn seeded_pair(seed: u8) -> SigningKey {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0] = seed;
    SigningKey::from_bytes(&seed_bytes)
}

fn committee_role() -> engingcore::types::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("benchmark role fits")
}

fn benchmark_proof_expiry<T: pallet::Config>() -> frame_system::pallet_prelude::BlockNumberFor<T> {
    frame_system::Pallet::<T>::block_number().saturating_add(100_000u32.saturated_into())
}

fn seed_current_key<T: Config>(cid: &engingcore::CidNumber, public_key: [u8; 32]) {
    if let Some(previous) = CurrentGrandpaKeys::<T>::get(cid) {
        GrandpaKeyOwnerByKey::<T>::remove(previous);
    }
    CurrentGrandpaKeys::<T>::insert(cid, public_key);
    GrandpaKeyOwnerByKey::<T>::insert(public_key, cid);
        pallet_grandpa::Authorities::<T>::put(pallet_grandpa::BoundedAuthorityList::<
            T::MaxAuthorities,
        >::force_from(
            vec![(
                GrandpaAuthorityId::from(ed25519::Public::from_raw(public_key)),
                1,
            )],
            None,
        ));

}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn propose_emergency_grandpa_key_recovery() {
        let actor_cid_number = nrc_cid();
        let actor_role_code = committee_role();
        let proposer = nrc_admin::<T>();
        let new_pair = seeded_pair(11);
        let new_public_key = new_pair.verifying_key().to_bytes();
        let proof_nonce = 0;
        let proof_expires_at = benchmark_proof_expiry::<T>();
        let old_public_key = seeded_pair(12).verifying_key().to_bytes();
        seed_current_key::<T>(&actor_cid_number, old_public_key);
        let payload = proof::payload::<T>(
            actor_cid_number.clone(),
            actor_role_code.clone(),
            proposer.clone(),
            old_public_key,
            new_public_key,
            proof_nonce,
            proof_expires_at,
            GrandpaKeyChangeKind::EmergencyRecovery,
        );
        let new_public_key_signature = new_pair.sign(&proof::signing_digest(&payload)).to_bytes();

        #[extrinsic_call]
        propose_emergency_grandpa_key_recovery(
            RawOrigin::Signed(proposer),
            actor_cid_number,
            actor_role_code,
            new_public_key,
            proof_nonce,
            proof_expires_at,
            new_public_key_signature,
        );

        assert!(engingcore::Pallet::<T>::get_proposal_data(0).is_some());
    }

    #[benchmark]
    fn schedule_grandpa_key_rotation() {
        let actor_cid_number = nrc_cid();
        let actor_role_code = committee_role();
        let proposer = nrc_admin::<T>();
        let old_pair = seeded_pair(12);
        let new_pair = seeded_pair(13);
        let old_public_key = old_pair.verifying_key().to_bytes();
        let new_public_key = new_pair.verifying_key().to_bytes();

        seed_current_key::<T>(&actor_cid_number, old_public_key);

        let proof_nonce = 0;
        let proof_expires_at = benchmark_proof_expiry::<T>();
        let payload = proof::payload::<T>(
            actor_cid_number.clone(),
            actor_role_code.clone(),
            proposer.clone(),
            old_public_key,
            new_public_key,
            proof_nonce,
            proof_expires_at,
            GrandpaKeyChangeKind::RoutineRotation,
        );
        let old_public_key_signature = old_pair.sign(&proof::signing_digest(&payload)).to_bytes();
        let new_public_key_signature = new_pair.sign(&proof::signing_digest(&payload)).to_bytes();

        #[extrinsic_call]
        schedule_grandpa_key_rotation(
            RawOrigin::Signed(proposer),
            actor_cid_number,
            actor_role_code,
            new_public_key,
            proof_nonce,
            proof_expires_at,
            old_public_key_signature,
            new_public_key_signature,
        );

        assert!(pallet_grandpa::Pallet::<T>::pending_change().is_some());
    }
}
