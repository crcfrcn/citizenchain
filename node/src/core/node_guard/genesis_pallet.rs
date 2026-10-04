//! GenesisPallet 五字段节点永久策略。
//!
//! 节点直接使用 primitives 创世常量和固定 RAW key/SCALE 形态校验，不读取 runtime
//! metadata 或 runtime API。三个创世事实永久冻结；阶段与开发者直升开关只允许在同一
//! runtime 升级中从 `(Genesis, true)` 原子、单向切换为 `(Operation, false)`。

use std::collections::BTreeMap;

use codec::{Decode, Encode};
use sp_crypto_hashing::twox_128;

use primitives::genesis::{CITIZENS, COUNTRY, GENESIS_CITIZEN_MAX};

const PALLET_NAME: &[u8] = b"GenesisPallet";

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
enum MChainPhase {
    Genesis,
    Operation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalState {
    Genesis,
    Operation,
}

#[derive(Debug, Eq, PartialEq)]
pub enum GuardError {
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

pub mod storage_key {
    use super::*;

    fn storage_value(storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(PALLET_NAME, storage)
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

fn decode_exact<T: Decode>(raw: &[u8], label: &'static str) -> Result<T, GuardError> {
    let mut input = raw;
    let value = T::decode(&mut input).map_err(|_| GuardError::StorageValueDecodeFailed(label))?;
    if !input.is_empty() {
        return Err(GuardError::StorageValueDecodeFailed(label));
    }
    Ok(value)
}

fn known_keys() -> [Vec<u8>; 6] {
    [
        storage_key::storage_version(),
        storage_key::phase(),
        storage_key::developer_upgrade_enabled(),
        storage_key::citizens_declaration(),
        storage_key::country_declaration(),
        storage_key::citizen_max(),
    ]
}

fn check_fixed_facts<F>(read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let storage_version = read(&storage_key::storage_version())
        .map(|raw| decode_exact::<u16>(&raw, "GenesisPallet::StorageVersion"))
        .transpose()?
        .unwrap_or_default();
    if storage_version != 0 {
        return Err(GuardError::StorageVersionInvalid(storage_version));
    }

    let citizens =
        read(&storage_key::citizens_declaration()).ok_or(GuardError::CitizensDeclarationMissing)?;
    if citizens != CITIZENS.as_bytes().to_vec().encode() {
        return Err(GuardError::CitizensDeclarationChanged);
    }

    let country =
        read(&storage_key::country_declaration()).ok_or(GuardError::CountryDeclarationMissing)?;
    if country != COUNTRY.as_bytes().to_vec().encode() {
        return Err(GuardError::CountryDeclarationChanged);
    }

    let citizen_max = read(&storage_key::citizen_max()).ok_or(GuardError::CitizenMaxMissing)?;
    let citizen_max = decode_exact::<u64>(&citizen_max, "GenesisPallet::CitizenMax")?;
    if citizen_max != GENESIS_CITIZEN_MAX {
        return Err(GuardError::CitizenMaxChanged(citizen_max));
    }
    Ok(())
}

fn canonical_state<F>(read: &F) -> Result<CanonicalState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let phase = read(&storage_key::phase())
        .map(|raw| decode_exact::<MChainPhase>(&raw, "GenesisPallet::Phase"))
        .transpose()?;
    let developer = read(&storage_key::developer_upgrade_enabled())
        .map(|raw| decode_exact::<bool>(&raw, "GenesisPallet::DeveloperUpgradeEnabled"))
        .transpose()?;
    match (phase, developer) {
        // ValueQuery 的创世默认值不写 RAW key；显式写回默认值也属于非法状态改写。
        (None, None) => Ok(CanonicalState::Genesis),
        (Some(MChainPhase::Operation), Some(false)) => Ok(CanonicalState::Operation),
        _ => Err(GuardError::PhaseStateInvalid),
    }
}

fn check_state<F>(read: &F) -> Result<CanonicalState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(read)?;
    canonical_state(read)
}

pub fn check_imported_state<'a, I>(pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let known = known_keys();
    let mut state = BTreeMap::new();
    for (key, value) in pairs {
        if !known.contains(key) {
            return Err(GuardError::UnknownStorageKey(key.clone()));
        }
        state.insert(key.clone(), value.clone());
    }
    check_state(&|key| state.get(key).cloned()).map(|_| ())
}

pub fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_fixed_facts(&read)?;
    if canonical_state(&read)? != CanonicalState::Genesis {
        return Err(GuardError::PhaseStateInvalid);
    }
    Ok(())
}

pub fn check_full_state<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_state(&read).map(|_| ())
}

pub fn check_transition<FParent, FPost>(
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: &FParent,
    post: &FPost,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let prefix = storage_key::pallet_prefix();
    let known = known_keys();
    for key in post_delta.keys().filter(|key| key.starts_with(&prefix)) {
        if !known.contains(key) {
            return Err(GuardError::UnknownStorageKey(key.clone()));
        }
    }

    for immutable in [
        storage_key::storage_version(),
        storage_key::citizens_declaration(),
        storage_key::country_declaration(),
        storage_key::citizen_max(),
    ] {
        if post_delta.contains_key(&immutable) {
            return Err(GuardError::ImmutableStorageTouched(immutable));
        }
    }

    check_fixed_facts(parent)?;
    check_fixed_facts(post)?;
    let before = canonical_state(parent)?;
    let after = canonical_state(post)?;
    let phase_touched = post_delta.contains_key(&storage_key::phase());
    let developer_touched = post_delta.contains_key(&storage_key::developer_upgrade_enabled());

    if !phase_touched && !developer_touched {
        return if before == after {
            Ok(())
        } else {
            Err(GuardError::PhaseTransitionInvalid)
        };
    }
    if phase_touched != developer_touched {
        return Err(GuardError::PartialPhaseTransition);
    }
    if !post_delta.contains_key(sp_storage::well_known_keys::CODE) {
        return Err(GuardError::PhaseTransitionWithoutRuntimeUpgrade);
    }
    if before != CanonicalState::Genesis || after != CanonicalState::Operation {
        return Err(GuardError::PhaseTransitionInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genesis_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
        BTreeMap::from([
            (storage_key::storage_version(), 0u16.encode()),
            (
                storage_key::citizens_declaration(),
                CITIZENS.as_bytes().to_vec().encode(),
            ),
            (
                storage_key::country_declaration(),
                COUNTRY.as_bytes().to_vec().encode(),
            ),
            (storage_key::citizen_max(), GENESIS_CITIZEN_MAX.encode()),
        ])
    }

    fn operation_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
        let mut state = genesis_state();
        state.insert(storage_key::phase(), MChainPhase::Operation.encode());
        state.insert(storage_key::developer_upgrade_enabled(), false.encode());
        state
    }

    #[test]
    fn raw_keys_are_frame_storage_keys() {
        let prefix = storage_key::pallet_prefix();
        assert_eq!(prefix, twox_128(PALLET_NAME));
        let mut phase = twox_128(PALLET_NAME).to_vec();
        phase.extend_from_slice(&twox_128(b"Phase"));
        assert_eq!(storage_key::phase(), phase);
        let mut developer = twox_128(PALLET_NAME).to_vec();
        developer.extend_from_slice(&twox_128(b"DeveloperUpgradeEnabled"));
        assert_eq!(storage_key::developer_upgrade_enabled(), developer);
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
            storage_key::citizens_declaration(),
            storage_key::country_declaration(),
            storage_key::citizen_max(),
        ] {
            let mut state = genesis_state();
            state.insert(key, vec![0]);
            assert!(check_imported_state(state.iter()).is_err());
        }
        let mut state = genesis_state();
        let mut unknown = storage_key::pallet_prefix().to_vec();
        unknown.extend_from_slice(&twox_128(b"TargetBlockTimeMs"));
        state.insert(unknown.clone(), 360_000u64.encode());
        assert_eq!(
            check_imported_state(state.iter()),
            Err(GuardError::UnknownStorageKey(unknown))
        );
    }

    #[test]
    fn malformed_scale_and_noncanonical_defaults_are_rejected() {
        let mut state = genesis_state();
        state.insert(storage_key::citizen_max(), vec![0; 9]);
        assert!(check_imported_state(state.iter()).is_err());

        let mut state = genesis_state();
        state.insert(storage_key::phase(), MChainPhase::Genesis.encode());
        state.insert(storage_key::developer_upgrade_enabled(), true.encode());
        assert_eq!(
            check_imported_state(state.iter()),
            Err(GuardError::PhaseStateInvalid)
        );
    }

    #[test]
    fn only_atomic_runtime_upgrade_transition_passes() {
        let genesis = genesis_state();
        let operation = operation_state();
        let mut delta = BTreeMap::from([
            (storage_key::phase(), Some(MChainPhase::Operation.encode())),
            (
                storage_key::developer_upgrade_enabled(),
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
            Err(GuardError::PhaseTransitionWithoutRuntimeUpgrade)
        );
    }

    #[test]
    fn partial_reverse_and_fixed_fact_transitions_are_rejected() {
        let genesis = genesis_state();
        let operation = operation_state();
        let partial =
            BTreeMap::from([(storage_key::phase(), Some(MChainPhase::Operation.encode()))]);
        assert_eq!(
            check_transition(&partial, &|key| genesis.get(key).cloned(), &|key| operation
                .get(key)
                .cloned(),),
            Err(GuardError::PartialPhaseTransition)
        );

        let reverse = BTreeMap::from([
            (storage_key::phase(), None),
            (storage_key::developer_upgrade_enabled(), None),
            (sp_storage::well_known_keys::CODE.to_vec(), Some(vec![2])),
        ]);
        assert_eq!(
            check_transition(&reverse, &|key| operation.get(key).cloned(), &|key| genesis
                .get(key)
                .cloned(),),
            Err(GuardError::PhaseTransitionInvalid)
        );

        let fixed = BTreeMap::from([(
            storage_key::citizen_max(),
            Some(GENESIS_CITIZEN_MAX.encode()),
        )]);
        assert!(matches!(
            check_transition(&fixed, &|key| genesis.get(key).cloned(), &|key| genesis
                .get(key)
                .cloned(),),
            Err(GuardError::ImmutableStorageTouched(_))
        ));
    }
}
