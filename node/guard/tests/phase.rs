use super::*;

use crate::guard::genesis_pallet::check_genesis;
use crate::guard::genesis_pallet::check_imported_state;
use crate::guard::genesis_pallet::check_transition;
use crate::guard::genesis_pallet::PALLET_NAME;
use primitives::genesis::CITIZENS;
use primitives::genesis::COUNTRY;
use primitives::genesis::GENESIS_CITIZEN_MAX;
use sp_crypto_hashing::twox_128;
use std::collections::BTreeMap;

fn genesis_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
    BTreeMap::from([
        (
            crate::guard::genesis_pallet::storage_key::storage_version(),
            0u16.encode(),
        ),
        (
            crate::guard::genesis_pallet::storage_key::citizens_declaration(),
            CITIZENS.as_bytes().to_vec().encode(),
        ),
        (
            crate::guard::genesis_pallet::storage_key::country_declaration(),
            COUNTRY.as_bytes().to_vec().encode(),
        ),
        (
            crate::guard::genesis_pallet::storage_key::citizen_max(),
            GENESIS_CITIZEN_MAX.encode(),
        ),
    ])
}

fn operation_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut state = genesis_state();
    state.insert(
        crate::guard::genesis_pallet::storage_key::phase(),
        crate::guard::genesis_pallet::MChainPhase::Operation.encode(),
    );
    state.insert(
        crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
        false.encode(),
    );
    state
}

#[test]
fn raw_keys_are_frame_storage_keys() {
    let prefix = crate::guard::genesis_pallet::storage_key::pallet_prefix();
    assert_eq!(prefix, twox_128(PALLET_NAME));
    let mut phase = twox_128(PALLET_NAME).to_vec();
    phase.extend_from_slice(&twox_128(b"Phase"));
    assert_eq!(crate::guard::genesis_pallet::storage_key::phase(), phase);
    let mut developer = twox_128(PALLET_NAME).to_vec();
    developer.extend_from_slice(&twox_128(b"DeveloperUpgradeEnabled"));
    assert_eq!(
        crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
        developer
    );
}

#[test]
fn canonical_genesis_and_operation_states_pass() {
    let genesis = genesis_state();
    check_genesis(|key| genesis.get(key).cloned()).unwrap();
    check_imported_state(genesis.iter()).unwrap();
    check_imported_state(operation_state().iter()).unwrap();
}

#[test]
fn fixed_facts_and_unknown_storage_are_rejected() {
    for key in [
        crate::guard::genesis_pallet::storage_key::citizens_declaration(),
        crate::guard::genesis_pallet::storage_key::country_declaration(),
        crate::guard::genesis_pallet::storage_key::citizen_max(),
    ] {
        let mut state = genesis_state();
        state.insert(key, vec![0]);
        assert!(check_imported_state(state.iter()).is_err());
    }
    let mut state = genesis_state();
    let mut unknown = crate::guard::genesis_pallet::storage_key::pallet_prefix().to_vec();
    unknown.extend_from_slice(&twox_128(b"TargetBlockTimeMs"));
    state.insert(unknown.clone(), 360_000u64.encode());
    assert_eq!(
        check_imported_state(state.iter()),
        Err(crate::guard::genesis_pallet::GuardError::UnknownStorageKey(
            unknown
        ))
    );
}

#[test]
fn malformed_scale_and_noncanonical_defaults_are_rejected() {
    let mut state = genesis_state();
    state.insert(
        crate::guard::genesis_pallet::storage_key::citizen_max(),
        vec![0; 9],
    );
    assert!(check_imported_state(state.iter()).is_err());

    let mut state = genesis_state();
    state.insert(
        crate::guard::genesis_pallet::storage_key::phase(),
        crate::guard::genesis_pallet::MChainPhase::Genesis.encode(),
    );
    state.insert(
        crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
        true.encode(),
    );
    assert_eq!(
        check_imported_state(state.iter()),
        Err(crate::guard::genesis_pallet::GuardError::PhaseStateInvalid)
    );
}

#[test]
fn only_atomic_runtime_upgrade_transition_passes() {
    let genesis = genesis_state();
    let operation = operation_state();
    let mut delta = BTreeMap::from([
        (
            crate::guard::genesis_pallet::storage_key::phase(),
            Some(crate::guard::genesis_pallet::MChainPhase::Operation.encode()),
        ),
        (
            crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
            Some(false.encode()),
        ),
        (sp_storage::well_known_keys::CODE.to_vec(), Some(vec![1])),
    ]);
    check_transition(&delta, &|key| genesis.get(key).cloned(), &|key| {
        operation.get(key).cloned()
    })
    .unwrap();

    delta.remove(sp_storage::well_known_keys::CODE);
    assert_eq!(
        check_transition(&delta, &|key| genesis.get(key).cloned(), &|key| operation
            .get(key)
            .cloned(),),
        Err(crate::guard::genesis_pallet::GuardError::PhaseTransitionWithoutRuntimeUpgrade)
    );
}

#[test]
fn partial_reverse_and_fixed_fact_transitions_are_rejected() {
    let genesis = genesis_state();
    let operation = operation_state();
    let partial = BTreeMap::from([(
        crate::guard::genesis_pallet::storage_key::phase(),
        Some(crate::guard::genesis_pallet::MChainPhase::Operation.encode()),
    )]);
    assert_eq!(
        check_transition(&partial, &|key| genesis.get(key).cloned(), &|key| operation
            .get(key)
            .cloned(),),
        Err(crate::guard::genesis_pallet::GuardError::PartialPhaseTransition)
    );

    let reverse = BTreeMap::from([
        (crate::guard::genesis_pallet::storage_key::phase(), None),
        (
            crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
            None,
        ),
        (sp_storage::well_known_keys::CODE.to_vec(), Some(vec![2])),
    ]);
    assert_eq!(
        check_transition(&reverse, &|key| operation.get(key).cloned(), &|key| genesis
            .get(key)
            .cloned(),),
        Err(crate::guard::genesis_pallet::GuardError::PhaseTransitionInvalid)
    );

    let fixed = BTreeMap::from([(
        crate::guard::genesis_pallet::storage_key::citizen_max(),
        Some(GENESIS_CITIZEN_MAX.encode()),
    )]);
    assert!(matches!(
        check_transition(&fixed, &|key| genesis.get(key).cloned(), &|key| genesis
            .get(key)
            .cloned(),),
        Err(crate::guard::genesis_pallet::GuardError::ImmutableStorageTouched(_))
    ));
}
