//! 验证全节点奖励存储键、合法发行及异常状态转换拒绝。

use super::*;

use crate::guard::fullnode_issuance::author_from_header;
use crate::guard::fullnode_issuance::check_genesis;
use crate::guard::fullnode_issuance::expected_rewarded_blocks;
use crate::guard::fullnode_issuance::expected_total;
use crate::guard::fullnode_issuance::GuardError;
use codec::Encode;
use primitives::pow_const::FULLNODE_BLOCK_REWARD;
use primitives::pow_const::FULLNODE_REWARD_END_BLOCK;
use std::collections::BTreeMap;

fn check_transition<FParent, FPre, FPost>(
    block: u32,
    author: Option<[u8; 32]>,
    parent: FParent,
    pre_finalize: FPre,
    post: FPost,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    crate::guard::fullnode_issuance::check_transition(
        block,
        author,
        parent,
        pre_finalize,
        post,
        &mut crate::guard::FinalizeIssuancePlan::default(),
    )
}

fn put<T: Encode>(map: &mut BTreeMap<Vec<u8>, Vec<u8>>, key: Vec<u8>, value: T) {
    map.insert(key, value.encode());
}

#[test]
fn raw_storage_keys_match_runtime_contract() {
    use sp_crypto_hashing::{blake2_128, twox_128};

    let account = [5u8; 32];
    let mut expected_reward_account_id = twox_128(b"FullnodeIssue").to_vec();
    expected_reward_account_id.extend_from_slice(&twox_128(b"RewardAccountIdByMiner"));
    expected_reward_account_id.extend_from_slice(&blake2_128(&account));
    expected_reward_account_id.extend_from_slice(&account);
    assert_eq!(
        crate::guard::fullnode_issuance::storage_key::reward_account_id(&account),
        expected_reward_account_id
    );

    let mut expected_total = twox_128(b"Balances").to_vec();
    expected_total.extend_from_slice(&twox_128(b"TotalIssuance"));
    assert_eq!(
        crate::guard::fullnode_issuance::storage_key::total_issuance(),
        expected_total
    );
}

fn valid_transition(
    block: u32,
    author: [u8; 32],
    recipient: [u8; 32],
) -> (
    BTreeMap<Vec<u8>, Vec<u8>>,
    BTreeMap<Vec<u8>, Vec<u8>>,
    BTreeMap<Vec<u8>, Vec<u8>>,
) {
    let parent = BTreeMap::from([
        (
            storage_key::rewarded_block_count(),
            expected_rewarded_blocks(block - 1).encode(),
        ),
        (
            storage_key::total_fullnode_issued(),
            expected_total(block - 1).encode(),
        ),
    ]);
    let mut pre = parent.clone();
    if recipient != author {
        pre.insert(storage_key::reward_account_id(&author), recipient.encode());
    }
    let post = BTreeMap::from([
        (
            storage_key::rewarded_block_count(),
            expected_rewarded_blocks(block).encode(),
        ),
        (
            storage_key::total_fullnode_issued(),
            expected_total(block).encode(),
        ),
        (
            storage_key::last_reward_audit(),
            (block, author, recipient, FULLNODE_BLOCK_REWARD).encode(),
        ),
        (storage_key::last_authored(&author), block.encode()),
    ]);
    (parent, pre, post)
}

#[test]
fn deterministic_schedule_boundaries_are_fixed() {
    assert_eq!(expected_rewarded_blocks(0), 0);
    assert_eq!(expected_rewarded_blocks(1), 1);
    assert_eq!(
        expected_rewarded_blocks(FULLNODE_REWARD_END_BLOCK),
        9_999_999
    );
    assert_eq!(
        expected_rewarded_blocks(FULLNODE_REWARD_END_BLOCK + 1),
        9_999_999
    );
}

#[test]
fn valid_bound_reward_account_reward_passes() {
    let author = [1u8; 32];
    let reward_account_id = [2u8; 32];
    let (parent, pre, post) = valid_transition(1, author, reward_account_id);
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Ok(())
    );
}

#[test]
fn valid_unbound_miner_reward_falls_back_to_author() {
    let author = [7u8; 32];
    let (parent, pre, post) = valid_transition(1, author, author);
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Ok(())
    );
}

#[test]
fn wrong_reward_amount_or_recipient_is_rejected() {
    let author = [3u8; 32];
    let reward_account_id = [4u8; 32];
    let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
    put(
        &mut post,
        crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
        (1u32, author, reward_account_id, FULLNODE_BLOCK_REWARD - 1),
    );
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)
    );
}

#[test]
fn wrong_audit_height_miner_and_last_authored_are_rejected() {
    let author = [21u8; 32];
    let reward_account_id = [22u8; 32];

    for audit in [
        (2u32, author, reward_account_id, FULLNODE_BLOCK_REWARD),
        (1u32, [23u8; 32], reward_account_id, FULLNODE_BLOCK_REWARD),
    ] {
        let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
        put(
            &mut post,
            crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
            audit,
        );
        assert_eq!(
            check_transition(
                1,
                Some(author),
                |key| parent.get(key).cloned(),
                |key| pre.get(key).cloned(),
                |key| post.get(key).cloned(),
            ),
            Err(crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)
        );
    }

    let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
    put(
        &mut post,
        crate::guard::fullnode_issuance::storage_key::last_authored(&author),
        2u32,
    );
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::LastAuthoredBlockInvalid)
    );
}

#[test]
fn reward_is_registered_in_shared_finalize_issuance_plan() {
    let author = [8u8; 32];
    let (parent, pre, post) = valid_transition(1, author, author);
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
    assert_eq!(
        crate::guard::fullnode_issuance::check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
            &mut plan,
        ),
        Ok(())
    );
    assert_eq!(plan.total, FULLNODE_BLOCK_REWARD);
}

#[test]
fn post_reward_range_finalize_must_not_mint_or_rewrite_audit() {
    let block = FULLNODE_REWARD_END_BLOCK + 1;
    let author = [9u8; 32];
    let audit = (
        FULLNODE_REWARD_END_BLOCK,
        author,
        author,
        FULLNODE_BLOCK_REWARD,
    );
    let mut parent = BTreeMap::new();
    let mut pre = BTreeMap::new();
    let mut post = BTreeMap::new();
    for map in [&mut parent, &mut pre, &mut post] {
        put(
            map,
            crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
            expected_rewarded_blocks(block),
        );
        put(
            map,
            crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
            expected_total(block),
        );
        put(
            map,
            crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
            audit,
        );
        put(
            map,
            crate::guard::fullnode_issuance::storage_key::total_issuance(),
            50_000_000u128,
        );
    }
    assert_eq!(
        check_transition(
            block,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Ok(())
    );

    put(
        &mut post,
        crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
        (block, author, author, FULLNODE_BLOCK_REWARD),
    );
    assert_eq!(
        check_transition(
            block,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::RewardAuditUnexpected)
    );
}

#[test]
fn missing_author_and_pre_finalize_counter_tamper_are_rejected() {
    let author = [5u8; 32];
    let (parent, mut pre, post) = valid_transition(1, author, author);
    assert_eq!(
        check_transition(
            1,
            None,
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::PowAuthorMissing)
    );
    put(
        &mut pre,
        crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
        1u32,
    );
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize)
    );
}

#[test]
fn genesis_rejects_nonzero_audit_state() {
    let mut state = BTreeMap::new();
    put(
        &mut state,
        crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
        1u32,
    );
    assert_eq!(
        check_genesis(|key| state.get(key).cloned()),
        Err(
            crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
                expected: 0,
                found: 1,
            }
        )
    );
}

#[test]
fn scale_values_with_trailing_bytes_are_rejected() {
    let author = [12u8; 32];
    let reward_account_id = [13u8; 32];
    let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
    post.get_mut(&crate::guard::fullnode_issuance::storage_key::rewarded_block_count())
        .expect("rewarded count")
        .push(0xff);
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(
            crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
                expected: 1,
                found: u32::MAX,
            }
        )
    );

    let (parent, mut pre, post) = valid_transition(1, author, reward_account_id);
    pre.get_mut(&crate::guard::fullnode_issuance::storage_key::reward_account_id(&author))
        .expect("reward reward_account_id")
        .push(0xff);
    assert_eq!(
        check_transition(
            1,
            Some(author),
            |key| parent.get(key).cloned(),
            |key| pre.get(key).cloned(),
            |key| post.get(key).cloned(),
        ),
        Err(crate::guard::fullnode_issuance::GuardError::RewardAccountDecodeFailed)
    );
}

#[test]
fn pow_author_digest_rejects_trailing_bytes() {
    use sp_core::H256;
    use sp_runtime::{Digest, DigestItem};

    let author = sp_core::sr25519::Public::from_raw([42u8; 32]);
    let mut exact_header = Header::new(
        1,
        H256::repeat_byte(1),
        H256::repeat_byte(2),
        H256::repeat_byte(3),
        Digest {
            logs: vec![DigestItem::PreRuntime(
                sp_consensus_pow::POW_ENGINE_ID,
                author.encode(),
            )],
        },
    );
    assert_eq!(author_from_header(&exact_header), Some(author.0));

    exact_header.digest_mut().logs[0] = DigestItem::PreRuntime(sp_consensus_pow::POW_ENGINE_ID, {
        let mut bytes = author.encode();
        bytes.push(0xff);
        bytes
    });
    assert_eq!(author_from_header(&exact_header), None);
}
