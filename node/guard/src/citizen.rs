//! 解码公民身份和认证奖励事实，核对登记状态及发行变化。

use crate::guard::FinalizeIssuancePlan;
use codec::Decode;
use codec::Encode;
use primitives::citizen_const::CITIZEN_ISSUANCE_HIGH_REWARD;
use primitives::citizen_const::CITIZEN_ISSUANCE_HIGH_REWARD_COUNT;
use primitives::citizen_const::CITIZEN_ISSUANCE_MAX_COUNT;
use primitives::citizen_const::CITIZEN_ISSUANCE_NORMAL_REWARD;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

const CITIZEN_ISSUANCE_PALLET: &[u8] = b"CitizenIssuance";

const CITIZEN_IDENTITY_PALLET: &[u8] = b"CitizenIdentity";

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
enum CitizenStatus {
    Normal,
    Revoked,
}

#[derive(Clone, Debug, Decode, Eq, PartialEq)]
struct VotingIdentity {
    _passport_valid_from: u32,
    _passport_valid_until: u32,
    _citizen_status: CitizenStatus,
    _residence_province_code: Vec<u8>,
    _residence_city_code: Vec<u8>,
    _residence_town_code: Vec<u8>,
    _updated_at: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GuardError {
    StorageKeyMalformed(&'static str),
    StorageValueDecodeFailed(&'static str),
    GenesisStateNotEmpty(Vec<u8>),
    RewardedCountInvalid,
    RewardLimitExceeded,
    FirstIdentityMissing,
    IdentityReverseIndexMismatch,
    IdentityAlreadyExisted,
    RewardAlreadyClaimed,
    FinalizeIssuanceOverflow,
}

fn decode_exact<T: Decode>(raw: &[u8], label: &'static str) -> Result<T, GuardError> {
    let mut input = raw;
    let value = T::decode(&mut input)
        .map_err(|_| crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed(label))?;
    if !input.is_empty() {
        return Err(crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed(label));
    }
    Ok(value)
}

fn decode_or_zero<T: Decode + Default>(
    raw: Option<Vec<u8>>,
    label: &'static str,
) -> Result<T, GuardError> {
    match raw {
        Some(raw) => decode_exact(&raw, label),
        None => Ok(T::default()),
    }
}

fn unit_present<F>(read: &F, key: &[u8], label: &'static str) -> Result<bool, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let Some(raw) = read(key) else {
        return Ok(false);
    };
    let _: () = decode_exact(&raw, label)?;
    Ok(true)
}

fn reward_amount_at(index: u64) -> u128 {
    if index < CITIZEN_ISSUANCE_HIGH_REWARD_COUNT {
        CITIZEN_ISSUANCE_HIGH_REWARD
    } else {
        CITIZEN_ISSUANCE_NORMAL_REWARD
    }
}

/// 根据新增永久领取事实核对首次身份资格、人数、奖励档位和总发行计划。
pub(crate) fn check_transition<FParent, FPre, FPost>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: &FParent,
    pre: &FPre,
    post: &FPost,
    plan: &mut FinalizeIssuancePlan,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let claims = nodedesktop::storage::prefix(CITIZEN_ISSUANCE_PALLET, b"IdentityRewardClaimed");
    let accounts = nodedesktop::storage::prefix(CITIZEN_ISSUANCE_PALLET, b"AccountRewarded");
    let mut new_claims = Vec::new();
    let mut new_accounts = BTreeSet::new();
    for key in delta.keys() {
        if !key.starts_with(&claims) && !key.starts_with(&accounts) {
            continue;
        }
        let after = post(key).ok_or(GuardError::RewardAlreadyClaimed)?;
        decode_exact::<()>(&after, "permanent reward fact")?;
        if let Some(before) = parent(key) {
            decode_exact::<()>(&before, "permanent reward fact")?;
            continue;
        }
        if key.starts_with(&claims) {
            let encoded = key
                .get(claims.len() + 16..)
                .ok_or(GuardError::StorageKeyMalformed("IdentityRewardClaimed"))?;
            let cid =
                decode_exact::<identity::CidNumberBound>(encoded, "IdentityRewardClaimed key")?
                    .to_vec();
            if *key != storage_key::identity_claimed(&cid) {
                return Err(GuardError::StorageKeyMalformed("IdentityRewardClaimed"));
            }
            new_claims.push(cid);
        } else {
            let encoded = key
                .get(accounts.len() + 16..)
                .ok_or(GuardError::StorageKeyMalformed("AccountRewarded"))?;
            let account = decode_exact::<[u8; 32]>(encoded, "AccountRewarded key")?;
            if *key != storage_key::account_rewarded(&account) {
                return Err(GuardError::StorageKeyMalformed("AccountRewarded"));
            }
            new_accounts.insert(account);
        }
    }
    let count_key = storage_key::rewarded_count();
    let before = decode_or_zero::<u64>(parent(&count_key), "RewardedCount")?;
    let after = decode_or_zero::<u64>(post(&count_key), "RewardedCount")?;
    let expected = before
        .checked_add(new_claims.len() as u64)
        .ok_or(GuardError::RewardLimitExceeded)?;
    if expected > CITIZEN_ISSUANCE_MAX_COUNT {
        return Err(GuardError::RewardLimitExceeded);
    }
    if after != expected {
        return Err(GuardError::RewardedCountInvalid);
    }
    let mut recipients = BTreeSet::new();
    for (index, cid) in new_claims.iter().enumerate() {
        let voting = storage_key::voting_identity(cid);
        if parent(&voting).is_some() {
            return Err(GuardError::IdentityAlreadyExisted);
        }
        let raw = pre(&voting).ok_or(GuardError::FirstIdentityMissing)?;
        let _: VotingIdentity = decode_exact(&raw, "VotingIdentityByCid")?;
        let raw = pre(&storage_key::account_id_by_cid(cid))
            .ok_or(GuardError::IdentityReverseIndexMismatch)?;
        let account = decode_exact::<[u8; 32]>(&raw, "AccountIdByCid")?;
        let raw = pre(&storage_key::cid_by_account_id(&account))
            .ok_or(GuardError::IdentityReverseIndexMismatch)?;
        if decode_exact::<Vec<u8>>(&raw, "CidByAccountId")? != *cid {
            return Err(GuardError::IdentityReverseIndexMismatch);
        }
        if unit_present(
            parent,
            &storage_key::account_rewarded(&account),
            "AccountRewarded",
        )? || !recipients.insert(account)
        {
            return Err(GuardError::RewardAlreadyClaimed);
        }
        if !new_accounts.remove(&account) {
            return Err(GuardError::RewardedCountInvalid);
        }
        plan.add(reward_amount_at(before + index as u64))
            .map_err(|_| GuardError::FinalizeIssuanceOverflow)?;
    }
    if !new_accounts.is_empty() {
        return Err(GuardError::RewardedCountInvalid);
    }
    Ok(())
}

/// 创世认证发行计数与永久领取事实应为空。
pub(crate) fn check_genesis_key_values<'a, I>(pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let count = storage_key::rewarded_count();
    let claims = nodedesktop::storage::prefix(CITIZEN_ISSUANCE_PALLET, b"IdentityRewardClaimed");
    let accounts = nodedesktop::storage::prefix(CITIZEN_ISSUANCE_PALLET, b"AccountRewarded");
    for (key, value) in pairs {
        if key == &count {
            if decode_exact::<u64>(value, "RewardedCount")? != 0 {
                return Err(GuardError::RewardedCountInvalid);
            }
        } else if key.starts_with(&claims) || key.starts_with(&accounts) {
            return Err(GuardError::GenesisStateNotEmpty(key.clone()));
        }
    }
    Ok(())
}

pub mod storage_key {
    use super::*;
    use crate::guard::citizen_issuance::CITIZEN_IDENTITY_PALLET;
    use crate::guard::citizen_issuance::CITIZEN_ISSUANCE_PALLET;
    use sp_crypto_hashing::twox_128;

    // `nodedesktop::storage` 单源的薄委托；map key 必须传对应类型的完整 SCALE 编码。
    fn storage_prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn blake2_map_raw(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, encoded_key)
    }

    pub fn pallet_prefix() -> [u8; 16] {
        twox_128(CITIZEN_ISSUANCE_PALLET)
    }

    pub fn rewarded_count() -> Vec<u8> {
        storage_prefix(CITIZEN_ISSUANCE_PALLET, b"RewardedCount")
    }

    pub fn identity_claimed(cid_number: &[u8]) -> Vec<u8> {
        blake2_map_raw(
            CITIZEN_ISSUANCE_PALLET,
            b"IdentityRewardClaimed",
            &cid_number.to_vec().encode(),
        )
    }

    pub fn account_rewarded(account: &[u8; 32]) -> Vec<u8> {
        blake2_map_raw(CITIZEN_ISSUANCE_PALLET, b"AccountRewarded", account)
    }

    pub fn voting_identity(cid: &[u8]) -> Vec<u8> {
        blake2_map_raw(
            CITIZEN_IDENTITY_PALLET,
            b"VotingIdentityByCid",
            &cid.encode(),
        )
    }

    pub fn account_id_by_cid(cid: &[u8]) -> Vec<u8> {
        blake2_map_raw(CITIZEN_IDENTITY_PALLET, b"AccountIdByCid", &cid.encode())
    }

    pub fn cid_by_account_id(account: &[u8; 32]) -> Vec<u8> {
        blake2_map_raw(CITIZEN_IDENTITY_PALLET, b"CidByAccountId", account)
    }
}
#[cfg(test)]
#[path = "../tests/citizen.rs"]
mod tests;
