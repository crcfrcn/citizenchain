use super::*;

use crate::guard::citizen_issuance::check_genesis_key_values;
use crate::guard::citizen_issuance::check_imported_state_key_values;
use crate::guard::citizen_issuance::check_transition;
use crate::guard::citizen_issuance::PendingCertificationReward;
use codec::Encode;
use primitives::citizen_const::CITIZEN_ISSUANCE_HIGH_REWARD;
use primitives::citizen_const::CITIZEN_ISSUANCE_MAX_COUNT;
use sp_crypto_hashing::twox_128;
use sp_crypto_hashing::twox_64;
use std::collections::BTreeMap;

fn put<T: Encode>(map: &mut BTreeMap<Vec<u8>, Vec<u8>>, key: Vec<u8>, value: T) {
    map.insert(key, value.encode());
}

#[test]
fn pending_reward_key_and_value_match_runtime_contract() {
    use sp_crypto_hashing::{twox_128, twox_64};

    let index = 3u32;
    let encoded_index = index.encode();
    let mut expected = twox_128(b"CitizenIssuance").to_vec();
    expected.extend_from_slice(&twox_128(b"PendingRewards"));
    expected.extend_from_slice(&twox_64(&encoded_index));
    expected.extend_from_slice(&encoded_index);
    assert_eq!(
        crate::guard::citizen_issuance::storage_key::pending_reward(index),
        expected
    );

    let pending = PendingCertificationReward {
        account_id: [7u8; 32],
        cid_number: b"GD001-CTZN1-GOLDEN".to_vec(),
    };
    assert_eq!(
        pending.encode(),
        ([7u8; 32], b"GD001-CTZN1-GOLDEN".to_vec()).encode()
    );
}

fn voting_identity() -> Vec<u8> {
    (
        20260101u32,
        20360101u32,
        crate::guard::citizen_issuance::CitizenStatus::Normal,
        b"GD".to_vec(),
        b"001".to_vec(),
        Vec::<u8>::new(),
        1u32,
    )
        .encode()
}

#[allow(clippy::type_complexity)]
fn valid_transition() -> (
    BTreeMap<Vec<u8>, Vec<u8>>,
    BTreeMap<Vec<u8>, Vec<u8>>,
    BTreeMap<Vec<u8>, Vec<u8>>,
    BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    [u8; 32],
) {
    let parent = BTreeMap::new();
    let mut pre = BTreeMap::new();
    let mut post = BTreeMap::new();
    let account = [7u8; 32];
    let cid = b"GD001-CTZN1-TEST".to_vec();
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::pending_count(),
        1u32,
    );
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::pending_reward(0),
        PendingCertificationReward {
            account_id: account,
            cid_number: cid.clone(),
        },
    );
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::pending_identity(&cid),
        (),
    );
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::pending_account(&account),
        (),
    );
    pre.insert(
        crate::guard::citizen_issuance::storage_key::voting_identity(&cid),
        voting_identity(),
    );
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::account_id_by_cid(&cid),
        account,
    );
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::cid_by_account_id(&account),
        cid.clone(),
    );
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::rewarded_count(),
        1u64,
    );
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::identity_claimed(&cid),
        (),
    );
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::account_rewarded(&account),
        (),
    );
    post.insert(
        crate::guard::citizen_issuance::storage_key::voting_identity(&cid),
        voting_identity(),
    );
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::account_id_by_cid(&cid),
        account,
    );
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::cid_by_account_id(&account),
        cid,
    );
    let pre_delta = pre
        .iter()
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect();
    let post_delta = post
        .iter()
        .filter(|(key, _)| {
            key.starts_with(&crate::guard::citizen_issuance::storage_key::pallet_prefix())
        })
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect();
    (parent, pre, post, pre_delta, post_delta, account)
}

#[test]
fn valid_pending_reward_registers_exact_finalize_plan() {
    let (parent, pre, post, pre_delta, post_delta, account) = valid_transition();
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut plan,
        ),
        Ok(())
    );
    assert_eq!(
        plan.accounts.get(&account),
        Some(&CITIZEN_ISSUANCE_HIGH_REWARD)
    );
    assert_eq!(plan.total, CITIZEN_ISSUANCE_HIGH_REWARD);
}

#[test]
fn queue_gap_wrong_identity_and_residual_pending_are_rejected() {
    let (parent, mut pre, post, pre_delta, post_delta, _) = valid_transition();
    pre.remove(&crate::guard::citizen_issuance::storage_key::pending_reward(0));
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::PendingQueueGap)
    );

    let (parent, pre, mut post, pre_delta, post_delta, _) = valid_transition();
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::pending_count(),
        1u32,
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::PendingQueueNotCleared)
    );
}

#[test]
fn missing_markers_reverse_index_and_wrong_rewarded_count_are_rejected() {
    let (parent, mut pre, post, pre_delta, post_delta, _) = valid_transition();
    let cid = b"GD001-CTZN1-TEST".to_vec();
    pre.remove(&crate::guard::citizen_issuance::storage_key::pending_identity(&cid));
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::PendingMarkerMissing)
    );

    let (parent, pre, post, pre_delta, post_delta, _) = valid_transition();
    let mut bad_reverse = pre.clone();
    put(
        &mut bad_reverse,
        crate::guard::citizen_issuance::storage_key::account_id_by_cid(&cid),
        [9u8; 32],
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| bad_reverse.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch)
    );

    let (parent, pre, mut post, pre_delta, post_delta, _) = valid_transition();
    put(
        &mut post,
        crate::guard::citizen_issuance::storage_key::rewarded_count(),
        2u64,
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::RewardedCountInvalid)
    );
}

#[test]
fn either_cid_or_account_permanent_marker_rejects_reward() {
    let (mut parent, pre, post, pre_delta, post_delta, _) = valid_transition();
    let cid = b"GD001-CTZN1-TEST".to_vec();
    put(
        &mut parent,
        crate::guard::citizen_issuance::storage_key::identity_claimed(&cid),
        (),
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::RewardAlreadyClaimed)
    );

    let (mut parent, pre, post, pre_delta, post_delta, account) = valid_transition();
    put(
        &mut parent,
        crate::guard::citizen_issuance::storage_key::account_rewarded(&account),
        (),
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::RewardAlreadyClaimed)
    );
}

#[test]
fn genesis_must_have_no_claim_or_pending_state() {
    let empty = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    assert_eq!(check_genesis_key_values(empty.iter()), Ok(()));
    let mut canonical_zero = BTreeMap::new();
    put(
        &mut canonical_zero,
        crate::guard::citizen_issuance::storage_key::rewarded_count(),
        0u64,
    );
    put(
        &mut canonical_zero,
        crate::guard::citizen_issuance::storage_key::pending_count(),
        0u32,
    );
    put(
        &mut canonical_zero,
        crate::guard::citizen_issuance::storage_key::storage_version(),
        0u16,
    );
    assert_eq!(check_genesis_key_values(canonical_zero.iter()), Ok(()));
    let mut bad = BTreeMap::new();
    put(
        &mut bad,
        crate::guard::citizen_issuance::storage_key::pending_count(),
        1u32,
    );
    assert_eq!(
        check_genesis_key_values(bad.iter()),
        Err(
            crate::guard::citizen_issuance::GuardError::GenesisStateNotEmpty(
                crate::guard::citizen_issuance::storage_key::pending_count()
            )
        )
    );
}

fn rewarded_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
    // 使用既有普通区块夹具的 finalize 后状态，保留真实永久防重格式。
    valid_transition()
        .2
        .into_iter()
        .filter(|(key, _)| {
            key.starts_with(&crate::guard::citizen_issuance::storage_key::pallet_prefix())
        })
        .collect()
}

#[test]
fn imported_state_accepts_completed_reward_but_genesis_remains_empty() {
    let empty = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    assert_eq!(check_imported_state_key_values(1, empty.iter()), Ok(()));
    let state = rewarded_state();
    assert_eq!(check_imported_state_key_values(1, state.iter()), Ok(()));
    assert_eq!(
        check_imported_state_key_values(u32::MAX, state.iter()),
        Ok(())
    );
    assert!(matches!(
        check_imported_state_key_values(0, state.iter()),
        Err(crate::guard::citizen_issuance::GuardError::GenesisStateNotEmpty(_))
    ));
    let zeros = BTreeMap::from([
        (
            crate::guard::citizen_issuance::storage_key::rewarded_count(),
            0u64.encode(),
        ),
        (
            crate::guard::citizen_issuance::storage_key::pending_count(),
            0u32.encode(),
        ),
        (
            crate::guard::citizen_issuance::storage_key::storage_version(),
            0u16.encode(),
        ),
    ]);
    assert_eq!(check_imported_state_key_values(0, zeros.iter()), Ok(()));
}

#[test]
fn imported_state_rejects_wrong_counts_limit_and_pending_residue() {
    for count in [0u64, 2, CITIZEN_ISSUANCE_MAX_COUNT] {
        let mut state = rewarded_state();
        put(
            &mut state,
            crate::guard::citizen_issuance::storage_key::rewarded_count(),
            count,
        );
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::RewardedCountInvalid)
        );
    }
    let mut state = rewarded_state();
    put(
        &mut state,
        crate::guard::citizen_issuance::storage_key::rewarded_count(),
        CITIZEN_ISSUANCE_MAX_COUNT + 1,
    );
    assert_eq!(
        check_imported_state_key_values(1, state.iter()),
        Err(crate::guard::citizen_issuance::GuardError::RewardLimitExceeded)
    );
    for key in [
        crate::guard::citizen_issuance::storage_key::identity_claimed(b"GD001-CTZN1-TEST"),
        crate::guard::citizen_issuance::storage_key::account_rewarded(&[7u8; 32]),
    ] {
        let mut state = rewarded_state();
        state.remove(&key);
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::RewardedCountInvalid)
        );
    }
    for (key, value) in [
        (
            crate::guard::citizen_issuance::storage_key::pending_count(),
            0u32.encode(),
        ),
        (
            crate::guard::citizen_issuance::storage_key::pending_reward(0),
            Vec::new(),
        ),
        (
            crate::guard::citizen_issuance::storage_key::pending_identity(b"GD001-CTZN1-TEST"),
            ().encode(),
        ),
        (
            crate::guard::citizen_issuance::storage_key::pending_account(&[7u8; 32]),
            ().encode(),
        ),
    ] {
        let mut state = rewarded_state();
        state.insert(key, value);
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::PendingQueueNotCleared)
        );
    }
}

#[test]
fn imported_state_rejects_malformed_markers_values_and_unknown_keys() {
    for (key, label) in [
        (
            crate::guard::citizen_issuance::storage_key::identity_claimed(b"GD001-CTZN1-TEST"),
            "IdentityRewardClaimed",
        ),
        (
            crate::guard::citizen_issuance::storage_key::account_rewarded(&[7u8; 32]),
            "AccountRewarded",
        ),
    ] {
        let mut state = rewarded_state();
        let mut truncated = key.clone();
        truncated.truncate(32);
        let value = state.remove(&key).unwrap();
        state.insert(truncated, value);
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::StorageKeyMalformed(label))
        );
        let mut state = rewarded_state();
        let mut bad_key = key.clone();
        bad_key[32] ^= 1;
        let value = state.remove(&key).unwrap();
        state.insert(bad_key, value);
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::StorageKeyMalformed(label))
        );
        let mut state = rewarded_state();
        state.insert(key, vec![0]);
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed(label))
        );
    }
    let mut state = rewarded_state();
    let oversized_cid = vec![b'A'; 33];
    state.remove(
        &crate::guard::citizen_issuance::storage_key::identity_claimed(b"GD001-CTZN1-TEST"),
    );
    state.insert(
        crate::guard::citizen_issuance::storage_key::identity_claimed(&oversized_cid),
        ().encode(),
    );
    assert_eq!(
        check_imported_state_key_values(1, state.iter()),
        Err(
            crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed(
                "IdentityRewardClaimed key"
            )
        )
    );
    let mut state = rewarded_state();
    state
        .get_mut(&crate::guard::citizen_issuance::storage_key::rewarded_count())
        .unwrap()
        .push(0);
    assert_eq!(
        check_imported_state_key_values(1, state.iter()),
        Err(crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed("RewardedCount"))
    );
    let mut state = rewarded_state();
    put(
        &mut state,
        crate::guard::citizen_issuance::storage_key::storage_version(),
        1u16,
    );
    assert_eq!(
        check_imported_state_key_values(1, state.iter()),
        Err(crate::guard::citizen_issuance::GuardError::StorageVersionInvalid(1))
    );
    let mut state = rewarded_state();
    let unknown = crate::guard::citizen_issuance::storage_key::pallet_prefix().to_vec();
    state.insert(unknown.clone(), Vec::new());
    assert_eq!(
        check_imported_state_key_values(1, state.iter()),
        Err(crate::guard::citizen_issuance::GuardError::UnknownStorageKey(unknown))
    );
}

#[test]
fn twox_pending_key_parser_rejects_tampering() {
    let prefix = crate::guard::citizen_issuance::storage_key::pending_reward_prefix();
    let key = crate::guard::citizen_issuance::storage_key::pending_reward(9);
    assert_eq!(parse_twox_u32_key(&key, &prefix), Ok(9));
    let mut bad = key;
    bad[prefix.len()] ^= 1;
    assert_eq!(
        parse_twox_u32_key(&bad, &prefix),
        Err(crate::guard::citizen_issuance::GuardError::StorageKeyMalformed("PendingRewards"))
    );
}

#[test]
fn malicious_identity_claim_and_unknown_delta_are_rejected() {
    let (mut parent, pre, post, pre_delta, post_delta, _) = valid_transition();
    let cid = b"GD001-CTZN1-TEST".to_vec();
    parent.insert(
        crate::guard::citizen_issuance::storage_key::voting_identity(&cid),
        voting_identity(),
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::IdentityAlreadyExisted)
    );

    let (parent, mut pre, post, pre_delta, post_delta, account) = valid_transition();
    put(
        &mut pre,
        crate::guard::citizen_issuance::storage_key::cid_by_account_id(&account),
        b"GD001-CTZN1-TAMPERED".to_vec(),
    );
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::IdentityReverseIndexMismatch)
    );

    let (parent, pre, post, mut pre_delta, post_delta, _) = valid_transition();
    let mut unknown = crate::guard::citizen_issuance::storage_key::pallet_prefix().to_vec();
    unknown.extend_from_slice(b"shadow-audit");
    pre_delta.insert(unknown, Some(vec![1]));
    assert_eq!(
        check_transition(
            2,
            &pre_delta,
            &post_delta,
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::citizen_issuance::GuardError::PendingQueueUnexpectedKey)
    );
}

#[test]
fn genesis_rejects_trailing_zero_and_unknown_keys() {
    let mut trailing = BTreeMap::new();
    let mut zero = 0u64.encode();
    zero.push(0xff);
    trailing.insert(
        crate::guard::citizen_issuance::storage_key::rewarded_count(),
        zero,
    );
    assert_eq!(
        check_genesis_key_values(trailing.iter()),
        Err(crate::guard::citizen_issuance::GuardError::StorageValueDecodeFailed("RewardedCount"))
    );

    let mut unknown = BTreeMap::new();
    let mut key = crate::guard::citizen_issuance::storage_key::pallet_prefix().to_vec();
    key.extend_from_slice(b"unknown");
    unknown.insert(key.clone(), Vec::new());
    assert_eq!(
        check_genesis_key_values(unknown.iter()),
        Err(crate::guard::citizen_issuance::GuardError::GenesisStateNotEmpty(key))
    );
}

pub(super) fn parse_twox_u32_key(key: &[u8], prefix: &[u8]) -> Result<u32, GuardError> {
    if !key.starts_with(prefix) || key.len() != prefix.len() + 12 {
        return Err(
            crate::guard::citizen_issuance::GuardError::StorageKeyMalformed("PendingRewards"),
        );
    }
    let hash_at = prefix.len();
    let encoded_at = hash_at + 8;
    let encoded = &key[encoded_at..];
    if twox_64(encoded) != key[hash_at..encoded_at] {
        return Err(
            crate::guard::citizen_issuance::GuardError::StorageKeyMalformed("PendingRewards"),
        );
    }
    decode_exact(encoded, "PendingRewards key")
}
