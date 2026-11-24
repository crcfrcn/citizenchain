//! 核对创世阶段、声明和固定事实，拒绝受保护字段非法更改。

use codec::Decode;
use codec::Encode;
use primitives::genesis::CITIZENS;
use primitives::genesis::COUNTRY;
use primitives::genesis::GENESIS_CITIZEN_MAX;

use std::collections::BTreeMap;

const PALLET_NAME: &[u8] = b"Genesis";

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
    ImmutableStorageTouched(Vec<u8>),
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
            crate::guard::genesis_pallet::decode_exact::<u16>(&raw, "Genesis::StorageVersion")
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
    let citizen_max =
        crate::guard::genesis_pallet::decode_exact::<u64>(&citizen_max, "Genesis::CitizenMax")?;
    if citizen_max != GENESIS_CITIZEN_MAX {
        return Err(crate::guard::genesis_pallet::GuardError::CitizenMaxChanged(
            citizen_max,
        ));
    }
    Ok(())
}

pub(crate) fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(&read)
}

pub(crate) fn check_full_state<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(&read)
}

pub(crate) fn check_transition<FParent, FPost>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: &FParent,
    post: &FPost,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let known = known_keys();
    for key in delta
        .keys()
        .filter(|k| k.starts_with(&storage_key::pallet_prefix()))
    {
        if !known.contains(key) {
            return Err(GuardError::UnknownStorageKey(key.clone()));
        }
    }
    for key in [
        storage_key::storage_version(),
        storage_key::citizens_declaration(),
        storage_key::country_declaration(),
        storage_key::citizen_max(),
    ] {
        if delta.contains_key(&key) {
            return Err(GuardError::ImmutableStorageTouched(key));
        }
    }
    check_fixed_facts(parent)?;
    check_fixed_facts(post)
}

pub mod storage_key {
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
