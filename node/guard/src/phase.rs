use codec::Decode;
use codec::Encode;
use primitives::genesis::CITIZENS;
use primitives::genesis::COUNTRY;
use primitives::genesis::GENESIS_CITIZEN_MAX;

use std::collections::BTreeMap;

const PALLET_NAME: &[u8] = b"GenesisPallet";

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
pub(crate) enum MChainPhase {
    Genesis,
    Operation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalState {
    Genesis,
    Operation,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GuardError {
    UnknownStorageKey(Vec<u8>),
    StorageValueDecodeFailed(&'static str),
    StorageVersionInvalid(u16),
    CitizensDeclarationMissing,
    CitizensDeclarationChanged,
    CountryDeclarationMissing,
    CountryDeclarationChanged,
    CitizenMaxMissing,
    CitizenMaxChanged(u64),
    PhaseStateInvalid,
    ImmutableStorageTouched(Vec<u8>),
    PartialPhaseTransition,
    PhaseTransitionWithoutRuntimeUpgrade,
    PhaseTransitionInvalid,
}

fn decode_exact<T: Decode>(raw: &[u8], label: &'static str) -> Result<T, GuardError> {
    let mut input = raw;
    let value = T::decode(&mut input)
        .map_err(|_| crate::guard::genesis_pallet::GuardError::StorageValueDecodeFailed(label))?;
    if !input.is_empty() {
        return Err(crate::guard::genesis_pallet::GuardError::StorageValueDecodeFailed(label));
    }
    Ok(value)
}

fn known_keys() -> [Vec<u8>; 6] {
    [
        crate::guard::genesis_pallet::storage_key::storage_version(),
        crate::guard::genesis_pallet::storage_key::phase(),
        crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled(),
        crate::guard::genesis_pallet::storage_key::citizens_declaration(),
        crate::guard::genesis_pallet::storage_key::country_declaration(),
        crate::guard::genesis_pallet::storage_key::citizen_max(),
    ]
}

fn check_fixed_facts<F>(read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let storage_version = read(&crate::guard::genesis_pallet::storage_key::storage_version())
        .map(|raw| {
            crate::guard::genesis_pallet::decode_exact::<u16>(&raw, "GenesisPallet::StorageVersion")
        })
        .transpose()?
        .unwrap_or_default();
    if storage_version != 0 {
        return Err(
            crate::guard::genesis_pallet::GuardError::StorageVersionInvalid(storage_version),
        );
    }

    let citizens = read(&crate::guard::genesis_pallet::storage_key::citizens_declaration())
        .ok_or(crate::guard::genesis_pallet::GuardError::CitizensDeclarationMissing)?;
    if citizens != CITIZENS.as_bytes().to_vec().encode() {
        return Err(crate::guard::genesis_pallet::GuardError::CitizensDeclarationChanged);
    }

    let country = read(&crate::guard::genesis_pallet::storage_key::country_declaration())
        .ok_or(crate::guard::genesis_pallet::GuardError::CountryDeclarationMissing)?;
    if country != COUNTRY.as_bytes().to_vec().encode() {
        return Err(crate::guard::genesis_pallet::GuardError::CountryDeclarationChanged);
    }

    let citizen_max = read(&crate::guard::genesis_pallet::storage_key::citizen_max())
        .ok_or(crate::guard::genesis_pallet::GuardError::CitizenMaxMissing)?;
    let citizen_max = crate::guard::genesis_pallet::decode_exact::<u64>(
        &citizen_max,
        "GenesisPallet::CitizenMax",
    )?;
    if citizen_max != GENESIS_CITIZEN_MAX {
        return Err(crate::guard::genesis_pallet::GuardError::CitizenMaxChanged(
            citizen_max,
        ));
    }
    Ok(())
}

fn canonical_state<F>(read: &F) -> Result<CanonicalState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let phase = read(&crate::guard::genesis_pallet::storage_key::phase())
        .map(|raw| {
            crate::guard::genesis_pallet::decode_exact::<MChainPhase>(&raw, "GenesisPallet::Phase")
        })
        .transpose()?;
    let developer = read(&crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled())
        .map(|raw| {
            crate::guard::genesis_pallet::decode_exact::<bool>(
                &raw,
                "GenesisPallet::DeveloperUpgradeEnabled",
            )
        })
        .transpose()?;
    match (phase, developer) {
        // ValueQuery 的创世默认值不写 RAW key；显式写回默认值也属于非法状态改写。
        (None, None) => Ok(crate::guard::genesis_pallet::CanonicalState::Genesis),
        (Some(crate::guard::genesis_pallet::MChainPhase::Operation), Some(false)) => {
            Ok(crate::guard::genesis_pallet::CanonicalState::Operation)
        }
        _ => Err(crate::guard::genesis_pallet::GuardError::PhaseStateInvalid),
    }
}

fn check_state<F>(read: &F) -> Result<CanonicalState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(read)?;
    canonical_state(read)
}

pub(crate) fn check_imported_state<'a, I>(pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let known = known_keys();
    let mut state = BTreeMap::new();
    for (key, value) in pairs {
        if !known.contains(key) {
            return Err(crate::guard::genesis_pallet::GuardError::UnknownStorageKey(
                key.clone(),
            ));
        }
        state.insert(key.clone(), value.clone());
    }
    check_state(&|key| state.get(key).cloned()).map(|_| ())
}

pub(crate) fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(&read)?;
    if canonical_state(&read)? != crate::guard::genesis_pallet::CanonicalState::Genesis {
        return Err(crate::guard::genesis_pallet::GuardError::PhaseStateInvalid);
    }
    Ok(())
}

pub(crate) fn check_full_state<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_state(&read).map(|_| ())
}

pub(crate) fn check_transition<FParent, FPost>(
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: &FParent,
    post: &FPost,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let prefix = crate::guard::genesis_pallet::storage_key::pallet_prefix();
    let known = known_keys();
    for key in post_delta.keys().filter(|key| key.starts_with(&prefix)) {
        if !known.contains(key) {
            return Err(crate::guard::genesis_pallet::GuardError::UnknownStorageKey(
                key.clone(),
            ));
        }
    }

    for immutable in [
        crate::guard::genesis_pallet::storage_key::storage_version(),
        crate::guard::genesis_pallet::storage_key::citizens_declaration(),
        crate::guard::genesis_pallet::storage_key::country_declaration(),
        crate::guard::genesis_pallet::storage_key::citizen_max(),
    ] {
        if post_delta.contains_key(&immutable) {
            return Err(
                crate::guard::genesis_pallet::GuardError::ImmutableStorageTouched(immutable),
            );
        }
    }

    check_fixed_facts(parent)?;
    check_fixed_facts(post)?;
    let before = canonical_state(parent)?;
    let after = canonical_state(post)?;
    let phase_touched =
        post_delta.contains_key(&crate::guard::genesis_pallet::storage_key::phase());
    let developer_touched = post_delta
        .contains_key(&crate::guard::genesis_pallet::storage_key::developer_upgrade_enabled());

    if !phase_touched && !developer_touched {
        return if before == after {
            Ok(())
        } else {
            Err(crate::guard::genesis_pallet::GuardError::PhaseTransitionInvalid)
        };
    }
    if phase_touched != developer_touched {
        return Err(crate::guard::genesis_pallet::GuardError::PartialPhaseTransition);
    }
    if !post_delta.contains_key(sp_storage::well_known_keys::CODE) {
        return Err(crate::guard::genesis_pallet::GuardError::PhaseTransitionWithoutRuntimeUpgrade);
    }
    if before != crate::guard::genesis_pallet::CanonicalState::Genesis
        || after != crate::guard::genesis_pallet::CanonicalState::Operation
    {
        return Err(crate::guard::genesis_pallet::GuardError::PhaseTransitionInvalid);
    }
    Ok(())
}

pub mod storage_key {
    use super::*;
    use crate::guard::genesis_pallet::PALLET_NAME;
    use sp_crypto_hashing::twox_128;

    fn storage_value(storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(PALLET_NAME, storage)
    }

    pub fn pallet_prefix() -> [u8; 16] {
        twox_128(PALLET_NAME)
    }

    pub fn storage_version() -> Vec<u8> {
        let mut key = pallet_prefix().to_vec();
        key.extend_from_slice(&twox_128(b":__STORAGE_VERSION__:"));
        key
    }

    pub fn phase() -> Vec<u8> {
        storage_value(b"Phase")
    }

    pub fn developer_upgrade_enabled() -> Vec<u8> {
        storage_value(b"DeveloperUpgradeEnabled")
    }

    pub fn citizens_declaration() -> Vec<u8> {
        storage_value(b"CitizensDeclaration")
    }

    pub fn country_declaration() -> Vec<u8> {
        storage_value(b"CountryDeclaration")
    }

    pub fn citizen_max() -> Vec<u8> {
        storage_value(b"CitizenMax")
    }
}
#[cfg(test)]
#[path = "../tests/phase.rs"]
mod tests;
