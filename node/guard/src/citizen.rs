use crate::guard::FinalizeIssuancePlan;
use codec::Decode;
use codec::Encode;
use primitives::citizen_const::CITIZEN_ISSUANCE_HIGH_REWARD;
use primitives::citizen_const::CITIZEN_ISSUANCE_HIGH_REWARD_COUNT;
use primitives::citizen_const::CITIZEN_ISSUANCE_MAX_COUNT;
use primitives::citizen_const::CITIZEN_ISSUANCE_NORMAL_REWARD;

#[cfg(test)]
use sp_crypto_hashing::twox_64;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const CITIZEN_ISSUANCE_PALLET: &[u8] = b"CitizenIssuance";

const CITIZEN_IDENTITY_PALLET: &[u8] = b"CitizenIdentity";

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct PendingCertificationReward {
    account_id: [u8; 32],
    cid_number: Vec<u8>,
}

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
    StorageVersionInvalid(u16),
    UnknownStorageKey(Vec<u8>),
    GenesisStateNotEmpty(Vec<u8>),
    PendingCountChangedOutsideExtrinsic,
    PendingCountExceedsExtrinsics,
    PendingQueueGap,
    PendingQueueUnexpectedKey,
    PendingQueueNotCleared,
    PendingDuplicate,
    PendingMarkerMissing,
    PermanentMarkerChangedBeforeFinalize,
    PermanentMarkerMissingAfterFinalize,
    RewardedCountChangedBeforeFinalize,
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

/// 校验普通区块的待发队列、身份依据、永久防重状态，并登记精确 finalize 发行计划。
pub(crate) fn check_transition<FParent, FPre, FPost>(
    extrinsic_count: usize,
    pre_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: &FParent,
    pre: &FPre,
    post: &FPost,
    issuance_plan: &mut FinalizeIssuancePlan,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let pending_count_key = crate::guard::citizen_issuance::storage_key::pending_count();
    if parent(&pending_count_key).is_some() {
        return Err(
            crate::guard::citizen_issuance::GuardError::PendingCountChangedOutsideExtrinsic,
        );
    }
    let pending_count: u32 = decode_or_zero(pre(&pending_count_key), "PendingRewardCount")?;
    if usize::try_from(pending_count).unwrap_or(usize::MAX) > extrinsic_count {
        return Err(crate::guard::citizen_issuance::GuardError::PendingCountExceedsExtrinsics);
    }
    if post(&pending_count_key).is_some() {
        return Err(crate::guard::citizen_issuance::GuardError::PendingQueueNotCleared);
    }

    let parent_count: u64 = decode_or_zero(
        parent(&crate::guard::citizen_issuance::storage_key::rewarded_count()),
        "RewardedCount",
    )?;
    let pre_count: u64 = decode_or_zero(
        pre(&crate::guard::citizen_issuance::storage_key::rewarded_count()),
        "RewardedCount",
    )?;
    if pre_count != parent_count {
        return Err(crate::guard::citizen_issuance::GuardError::RewardedCountChangedBeforeFinalize);
    }
    let expected_after = parent_count
        .checked_add(u64::from(pending_count))
        .ok_or(crate::guard::citizen_issuance::GuardError::RewardLimitExceeded)?;
    if expected_after > CITIZEN_ISSUANCE_MAX_COUNT {
        return Err(crate::guard::citizen_issuance::GuardError::RewardLimitExceeded);
    }
    let post_count: u64 = decode_or_zero(
        post(&crate::guard::citizen_issuance::storage_key::rewarded_count()),
        "RewardedCount",
    )?;
    if post_count != expected_after {
        return Err(crate::guard::citizen_issuance::GuardError::RewardedCountInvalid);
    }

    let mut expected_pre_keys = BTreeSet::from([pending_count_key.clone()]);
    // Overlay 可能把“本块插入后删除”保留为显式 deletion delta；只要后状态确实为空即可接受。
    let mut expected_post_keys = BTreeSet::from([pending_count_key]);
    if pending_count > 0
        || post_delta.contains_key(&crate::guard::citizen_issuance::storage_key::rewarded_count())
    {
        expected_post_keys.insert(crate::guard::citizen_issuance::storage_key::rewarded_count());
    }
    let mut accounts = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for index in 0..pending_count {
        let pending_key = crate::guard::citizen_issuance::storage_key::pending_reward(index);
        let pending: PendingCertificationReward = pre(&pending_key)
            .ok_or(crate::guard::citizen_issuance::GuardError::PendingQueueGap)
            .and_then(|raw| decode_exact(&raw, "PendingRewards"))?;
        if !accounts.insert(pending.account_id) || !identities.insert(pending.cid_number.clone()) {
            return Err(crate::guard::citizen_issuance::GuardError::PendingDuplicate);
        }

        let pending_identity_key =
            crate::guard::citizen_issuance::storage_key::pending_identity(&pending.cid_number);
        let pending_account_key =
            crate::guard::citizen_issuance::storage_key::pending_account(&pending.account_id);
        if !unit_present(pre, &pending_identity_key, "PendingIdentityRewardClaimed")?
            || !unit_present(pre, &pending_account_key, "PendingAccountRewarded")?
        {
            return Err(crate::guard::citizen_issuance::GuardError::PendingMarkerMissing);
        }
        if post(&pending_key).is_some()
            || post(&pending_identity_key).is_some()
            || post(&pending_account_key).is_some()
        {
            return Err(crate::guard::citizen_issuance::GuardError::PendingQueueNotCleared);
        }

        let cid_number: Vec<u8> = pre(
            &crate::guard::citizen_issuance::storage_key::cid_by_account_id(&pending.account_id),
        )
        .ok_or(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch)
        .and_then(|raw| decode_exact(&raw, "CidByAccountId"))?;
        if cid_number != pending.cid_number {
            return Err(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch);
        }
        let voting_key = crate::guard::citizen_issuance::storage_key::voting_identity(&cid_number);
        if parent(&voting_key).is_some() {
            return Err(crate::guard::citizen_issuance::GuardError::IdentityAlreadyExisted);
        }
        let _identity: VotingIdentity = pre(&voting_key)
            .ok_or(crate::guard::citizen_issuance::GuardError::FirstIdentityMissing)
            .and_then(|raw| decode_exact(&raw, "VotingIdentityByCid"))?;
        let reverse: [u8; 32] =
            pre(&crate::guard::citizen_issuance::storage_key::account_id_by_cid(&cid_number))
                .ok_or(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch)
                .and_then(|raw| decode_exact(&raw, "AccountIdByCid"))?;
        if reverse != pending.account_id {
            return Err(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch);
        }

        let claimed_key =
            crate::guard::citizen_issuance::storage_key::identity_claimed(&pending.cid_number);
        let rewarded_key =
            crate::guard::citizen_issuance::storage_key::account_rewarded(&pending.account_id);
        if unit_present(parent, &claimed_key, "IdentityRewardClaimed")?
            || unit_present(parent, &rewarded_key, "AccountRewarded")?
        {
            return Err(crate::guard::citizen_issuance::GuardError::RewardAlreadyClaimed);
        }
        if unit_present(pre, &claimed_key, "IdentityRewardClaimed")?
            || unit_present(pre, &rewarded_key, "AccountRewarded")?
        {
            return Err(
                crate::guard::citizen_issuance::GuardError::PermanentMarkerChangedBeforeFinalize,
            );
        }
        if !unit_present(post, &claimed_key, "IdentityRewardClaimed")?
            || !unit_present(post, &rewarded_key, "AccountRewarded")?
        {
            return Err(
                crate::guard::citizen_issuance::GuardError::PermanentMarkerMissingAfterFinalize,
            );
        }

        expected_pre_keys.insert(pending_key);
        expected_pre_keys.insert(pending_identity_key.clone());
        expected_pre_keys.insert(pending_account_key.clone());
        expected_post_keys.insert(crate::guard::citizen_issuance::storage_key::pending_reward(
            index,
        ));
        expected_post_keys.insert(pending_identity_key);
        expected_post_keys.insert(pending_account_key);
        expected_post_keys.insert(claimed_key);
        expected_post_keys.insert(rewarded_key);

        let reward = reward_amount_at(parent_count + u64::from(index));
        issuance_plan
            .add(pending.account_id, reward)
            .map_err(|_| crate::guard::citizen_issuance::GuardError::FinalizeIssuanceOverflow)?;
    }

    let pallet_prefix = crate::guard::citizen_issuance::storage_key::pallet_prefix();
    for key in pre_delta
        .keys()
        .filter(|key| key.starts_with(&pallet_prefix))
    {
        if !expected_pre_keys.contains(key) {
            return Err(crate::guard::citizen_issuance::GuardError::PendingQueueUnexpectedKey);
        }
    }
    for key in post_delta
        .keys()
        .filter(|key| key.starts_with(&pallet_prefix))
    {
        if !expected_post_keys.contains(key) {
            return Err(crate::guard::citizen_issuance::GuardError::PendingQueueUnexpectedKey);
        }
    }
    Ok(())
}

/// block#0 的公民认证发行、永久防重与临时队列必须处于规范空状态。
///
/// FRAME 创世构建器会写入当前 pallet 存储版本，也可能把 `ValueQuery` 计数显式编码为零；
/// 零与 key 不存在语义相同。因此只允许存储版本 0 和两个计数的精确 SCALE 零值，任何
/// 防重标记、队列项、非零计数或未知 key 均拒绝。
pub(crate) fn check_genesis_key_values<'a, I>(pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let rewarded_count = crate::guard::citizen_issuance::storage_key::rewarded_count();
    let pending_count = crate::guard::citizen_issuance::storage_key::pending_count();
    let storage_version = crate::guard::citizen_issuance::storage_key::storage_version();
    for (key, value) in pairs {
        let canonical_empty = if key == &rewarded_count {
            crate::guard::citizen_issuance::decode_exact::<u64>(value, "RewardedCount")? == 0
        } else if key == &pending_count {
            crate::guard::citizen_issuance::decode_exact::<u32>(value, "PendingRewardCount")? == 0
        } else if key == &storage_version {
            crate::guard::citizen_issuance::decode_exact::<u16>(value, "StorageVersion")? == 0
        } else {
            false
        };
        if canonical_empty {
            continue;
        }
        return Err(crate::guard::citizen_issuance::GuardError::GenesisStateNotEmpty(key.clone()));
    }
    Ok(())
}

/// 完整导入的非创世后状态允许永久领奖记录；累计数须与两张防重表一致，临时队列必须清空。
/// 永久记录不依赖当前 CID 绑定，换绑或撤销身份不能抹掉历史领奖事实。
pub(crate) fn check_imported_state_key_values<'a, I>(block: u32, pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    if block == 0 {
        return check_genesis_key_values(pairs);
    }
    let prefix = |name: &[u8]| nodedesktop::storage::prefix(CITIZEN_ISSUANCE_PALLET, name);
    let identity_prefix = prefix(b"IdentityRewardClaimed");
    let account_prefix = prefix(b"AccountRewarded");
    let rewarded_count_key = crate::guard::citizen_issuance::storage_key::rewarded_count();
    let storage_version_key = crate::guard::citizen_issuance::storage_key::storage_version();
    let pending_count_key = crate::guard::citizen_issuance::storage_key::pending_count();
    let pending_prefixes = [
        prefix(b"PendingRewards"),
        prefix(b"PendingIdentityRewardClaimed"),
        prefix(b"PendingAccountRewarded"),
    ];
    let mut rewarded = 0u64;
    let mut identities = 0u64;
    let mut accounts = 0u64;
    for (key, value) in pairs {
        if key == &rewarded_count_key {
            rewarded = decode_exact(value, "RewardedCount")?;
            if rewarded > CITIZEN_ISSUANCE_MAX_COUNT {
                return Err(crate::guard::citizen_issuance::GuardError::RewardLimitExceeded);
            }
        } else if key == &storage_version_key {
            let version =
                crate::guard::citizen_issuance::decode_exact::<u16>(value, "StorageVersion")?;
            if version != 0 {
                return Err(
                    crate::guard::citizen_issuance::GuardError::StorageVersionInvalid(version),
                );
            }
        } else if key.starts_with(&identity_prefix) {
            let encoded = key.get(identity_prefix.len() + 16..).ok_or(
                crate::guard::citizen_issuance::GuardError::StorageKeyMalformed(
                    "IdentityRewardClaimed",
                ),
            )?;
            let cid = crate::guard::citizen_issuance::decode_exact::<identity::CidNumberBound>(
                encoded,
                "IdentityRewardClaimed key",
            )?;
            if *key != crate::guard::citizen_issuance::storage_key::identity_claimed(&cid) {
                return Err(
                    crate::guard::citizen_issuance::GuardError::StorageKeyMalformed(
                        "IdentityRewardClaimed",
                    ),
                );
            }
            crate::guard::citizen_issuance::decode_exact::<()>(value, "IdentityRewardClaimed")?;
            identities += 1;
        } else if key.starts_with(&account_prefix) {
            let encoded = key.get(account_prefix.len() + 16..).ok_or(
                crate::guard::citizen_issuance::GuardError::StorageKeyMalformed("AccountRewarded"),
            )?;
            let account = crate::guard::citizen_issuance::decode_exact::<[u8; 32]>(
                encoded,
                "AccountRewarded key",
            )?;
            if *key != crate::guard::citizen_issuance::storage_key::account_rewarded(&account) {
                return Err(
                    crate::guard::citizen_issuance::GuardError::StorageKeyMalformed(
                        "AccountRewarded",
                    ),
                );
            }
            crate::guard::citizen_issuance::decode_exact::<()>(value, "AccountRewarded")?;
            accounts += 1;
        } else if key == &pending_count_key
            || pending_prefixes
                .iter()
                .any(|prefix| key.starts_with(prefix))
        {
            return Err(crate::guard::citizen_issuance::GuardError::PendingQueueNotCleared);
        } else {
            return Err(crate::guard::citizen_issuance::GuardError::UnknownStorageKey(key.clone()));
        }
    }
    if identities != rewarded || accounts != rewarded {
        return Err(crate::guard::citizen_issuance::GuardError::RewardedCountInvalid);
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

    pub fn pending_count() -> Vec<u8> {
        storage_prefix(CITIZEN_ISSUANCE_PALLET, b"PendingRewardCount")
    }

    pub fn storage_version() -> Vec<u8> {
        let mut key = pallet_prefix().to_vec();
        key.extend_from_slice(&twox_128(b":__STORAGE_VERSION__:"));
        key
    }

    pub fn pending_reward(index: u32) -> Vec<u8> {
        nodedesktop::storage::twox64_map(
            CITIZEN_ISSUANCE_PALLET,
            b"PendingRewards",
            &index.encode(),
        )
    }

    #[cfg(test)]
    pub fn pending_reward_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_ISSUANCE_PALLET, b"PendingRewards")
    }

    pub fn pending_identity(cid_number: &[u8]) -> Vec<u8> {
        blake2_map_raw(
            CITIZEN_ISSUANCE_PALLET,
            b"PendingIdentityRewardClaimed",
            &cid_number.to_vec().encode(),
        )
    }

    pub fn pending_account(account: &[u8; 32]) -> Vec<u8> {
        blake2_map_raw(CITIZEN_ISSUANCE_PALLET, b"PendingAccountRewarded", account)
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
