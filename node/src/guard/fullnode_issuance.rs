//! 全节点 PoW 发行节点永久策略。
//!
//! 节点从 PoW digest、finalize 前后 RAW 状态和编译期常量独立复算奖励，不读取 runtime metadata。
//! runtime 中的累计字段只是审计落点，不是规则来源；金额、高度范围和累计公式全部由节点二进制决定。

use super::MAccountInfo;
use std::collections::BTreeMap;

use codec::Decode;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::Header as HeaderT;

use citizenchain::opaque::Header;
use primitives::pow_const::{
    FULLNODE_BLOCK_REWARD, FULLNODE_REWARD_END_BLOCK, FULLNODE_REWARD_START_BLOCK,
};

use super::FinalizeIssuancePlan;

const FULLNODE_PALLET: &[u8] = b"FullnodeIssuance";
const BALANCES_PALLET: &[u8] = b"Balances";
const SYSTEM_PALLET: &[u8] = b"System";

/// 节点镜像的 `frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>`。
/// 节点只读取 free；其余字段占位保持 SCALE 顺序。
#[derive(Debug, PartialEq)]
pub enum GuardError {
    PowAuthorMissing,
    RewardedBlockCountInvalid { expected: u32, found: u32 },
    TotalFullnodeIssuedInvalid { expected: u128, found: u128 },
    RewardAuditMissing,
    RewardAuditUnexpected,
    RewardAuditInvalid,
    LastAuthoredBlockInvalid,
    TotalIssuanceMissing,
    RewardAccountDecodeFailed,
    FinalizeIssuanceOverflow,
    CounterChangedBeforeFinalize,
    AuditChangedBeforeFinalize,
}

pub mod storage_key {
    use super::*;

    // `crate::shared::storage_keys` 单源的薄委托;blake2_map 传裸 32 字节账户(AccountId32 无长度前缀)。
    fn storage_value(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(pallet, storage)
    }

    fn blake2_map(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_map(pallet, storage, encoded_key)
    }

    pub fn pallet_prefix() -> [u8; 16] {
        twox_128(FULLNODE_PALLET)
    }

    pub fn system_prefix() -> [u8; 16] {
        twox_128(SYSTEM_PALLET)
    }

    pub fn system_account_prefix() -> Vec<u8> {
        storage_value(SYSTEM_PALLET, b"Account")
    }

    pub fn rewarded_block_count() -> Vec<u8> {
        storage_value(FULLNODE_PALLET, b"RewardedBlockCount")
    }

    pub fn total_fullnode_issued() -> Vec<u8> {
        storage_value(FULLNODE_PALLET, b"TotalFullnodeIssued")
    }

    pub fn last_reward_audit() -> Vec<u8> {
        storage_value(FULLNODE_PALLET, b"LastRewardAudit")
    }

    pub fn reward_account_id(miner: &[u8; 32]) -> Vec<u8> {
        blake2_map(FULLNODE_PALLET, b"RewardAccountIdByMiner", miner)
    }

    pub fn last_authored(miner: &[u8; 32]) -> Vec<u8> {
        blake2_map(FULLNODE_PALLET, b"LastAuthoredBlockByMiner", miner)
    }

    pub fn total_issuance() -> Vec<u8> {
        storage_value(BALANCES_PALLET, b"TotalIssuance")
    }

    pub fn system_account(account: &[u8; 32]) -> Vec<u8> {
        blake2_map(SYSTEM_PALLET, b"Account", account)
    }
}

type RewardAudit = (u32, [u8; 32], [u8; 32], u128);

fn decode_exact<T: Decode>(bytes: &[u8]) -> Result<T, ()> {
    let mut input = bytes;
    let value = T::decode(&mut input).map_err(|_| ())?;
    if !input.is_empty() {
        return Err(());
    }
    Ok(value)
}

fn decode_or_zero<T: Decode + Default>(raw: Option<Vec<u8>>) -> Result<T, ()> {
    match raw {
        Some(bytes) => decode_exact(&bytes),
        None => Ok(T::default()),
    }
}

fn decode_required<T: Decode>(raw: Option<Vec<u8>>) -> Result<T, ()> {
    let bytes = raw.ok_or(())?;
    decode_exact(&bytes)
}

fn decode_audit(raw: Option<Vec<u8>>) -> Result<Option<RewardAudit>, ()> {
    raw.map(|bytes| decode_exact(&bytes)).transpose()
}

fn free_balance<F>(read: &F, account: &[u8; 32]) -> Result<u128, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    match read(&storage_key::system_account(account)) {
        Some(bytes) => decode_exact::<MAccountInfo>(&bytes)
            .map(|info| info.data.free)
            .map_err(|_| GuardError::RewardAccountDecodeFailed),
        None => Ok(0),
    }
}

pub fn expected_rewarded_blocks(block: u32) -> u32 {
    if block < FULLNODE_REWARD_START_BLOCK {
        0
    } else {
        block.min(FULLNODE_REWARD_END_BLOCK) - FULLNODE_REWARD_START_BLOCK + 1
    }
}

fn expected_total(block: u32) -> u128 {
    FULLNODE_BLOCK_REWARD * u128::from(expected_rewarded_blocks(block))
}

/// 从节点已验证的 PoW pre-runtime digest 解析矿工账户。
pub fn author_from_header(header: &Header) -> Option<[u8; 32]> {
    header.digest().logs().iter().find_map(|digest| {
        let (engine_id, data) = digest.as_pre_runtime()?;
        if engine_id != sp_consensus_pow::POW_ENGINE_ID {
            return None;
        }
        decode_exact::<sp_core::sr25519::Public>(data)
            .ok()
            .map(|public| public.0)
    })
}

/// 校验一个普通执行型区块在 finalize 前后的全节点奖励状态变化。
pub fn check_transition<FParent, FPre, FPost>(
    block: u32,
    author: Option<[u8; 32]>,
    parent: FParent,
    pre_finalize: FPre,
    post: FPost,
    issuance_plan: &mut FinalizeIssuancePlan,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let author = author.ok_or(GuardError::PowAuthorMissing)?;
    let previous_block = block.saturating_sub(1);
    let expected_before_count = expected_rewarded_blocks(previous_block);
    let expected_after_count = expected_rewarded_blocks(block);
    let expected_before_total = expected_total(previous_block);
    let expected_after_total = expected_total(block);

    let parent_count: u32 = decode_or_zero(parent(&storage_key::rewarded_block_count()))
        .map_err(|_| GuardError::CounterChangedBeforeFinalize)?;
    let pre_count: u32 = decode_or_zero(pre_finalize(&storage_key::rewarded_block_count()))
        .map_err(|_| GuardError::CounterChangedBeforeFinalize)?;
    if parent_count != expected_before_count || pre_count != parent_count {
        return Err(GuardError::CounterChangedBeforeFinalize);
    }
    let post_count: u32 =
        decode_or_zero(post(&storage_key::rewarded_block_count())).map_err(|_| {
            GuardError::RewardedBlockCountInvalid {
                expected: expected_after_count,
                found: u32::MAX,
            }
        })?;
    if post_count != expected_after_count {
        return Err(GuardError::RewardedBlockCountInvalid {
            expected: expected_after_count,
            found: post_count,
        });
    }

    let parent_total: u128 = decode_or_zero(parent(&storage_key::total_fullnode_issued()))
        .map_err(|_| GuardError::CounterChangedBeforeFinalize)?;
    let pre_total: u128 = decode_or_zero(pre_finalize(&storage_key::total_fullnode_issued()))
        .map_err(|_| GuardError::CounterChangedBeforeFinalize)?;
    if parent_total != expected_before_total || pre_total != parent_total {
        return Err(GuardError::CounterChangedBeforeFinalize);
    }
    let post_total: u128 =
        decode_or_zero(post(&storage_key::total_fullnode_issued())).map_err(|_| {
            GuardError::TotalFullnodeIssuedInvalid {
                expected: expected_after_total,
                found: u128::MAX,
            }
        })?;
    if post_total != expected_after_total {
        return Err(GuardError::TotalFullnodeIssuedInvalid {
            expected: expected_after_total,
            found: post_total,
        });
    }

    let parent_audit = decode_audit(parent(&storage_key::last_reward_audit()))
        .map_err(|_| GuardError::AuditChangedBeforeFinalize)?;
    let pre_audit = decode_audit(pre_finalize(&storage_key::last_reward_audit()))
        .map_err(|_| GuardError::AuditChangedBeforeFinalize)?;
    if pre_audit != parent_audit {
        return Err(GuardError::AuditChangedBeforeFinalize);
    }

    let in_reward_range = expected_after_count > expected_before_count;
    if !in_reward_range {
        if decode_audit(post(&storage_key::last_reward_audit()))
            .map_err(|_| GuardError::RewardAuditInvalid)?
            != pre_audit
        {
            return Err(GuardError::RewardAuditUnexpected);
        }
        return Ok(());
    }

    let recipient_account_id = match pre_finalize(&storage_key::reward_account_id(&author)) {
        Some(bytes) => {
            decode_exact::<[u8; 32]>(&bytes).map_err(|_| GuardError::RewardAccountDecodeFailed)?
        }
        None => author,
    };
    let audit = decode_audit(post(&storage_key::last_reward_audit()))
        .map_err(|_| GuardError::RewardAuditInvalid)?
        .ok_or(GuardError::RewardAuditMissing)?;
    if audit != (block, author, recipient_account_id, FULLNODE_BLOCK_REWARD) {
        return Err(GuardError::RewardAuditInvalid);
    }
    let last_authored: u32 = decode_or_zero(post(&storage_key::last_authored(&author)))
        .map_err(|_| GuardError::LastAuthoredBlockInvalid)?;
    if last_authored != block {
        return Err(GuardError::LastAuthoredBlockInvalid);
    }

    issuance_plan
        .add(recipient_account_id, FULLNODE_BLOCK_REWARD)
        .map_err(|_| GuardError::FinalizeIssuanceOverflow)?;
    Ok(())
}

/// warp/完整状态导入只能验证目标态累计公式；历史逐块实际到账由提供该 finalized 状态的守卫节点背书。
pub fn check_imported_state_key_values<'a, I>(block: u32, pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let mut map = BTreeMap::new();
    let fullnode_prefix = storage_key::pallet_prefix();
    let system_prefix = storage_key::system_prefix();
    let total_issuance = storage_key::total_issuance();
    for (key, value) in pairs {
        if key.starts_with(&fullnode_prefix)
            || key.starts_with(&system_prefix)
            || key == &total_issuance
        {
            map.insert(key.clone(), value.clone());
        }
    }
    let read = |key: &[u8]| map.get(key).cloned();
    let expected_count = expected_rewarded_blocks(block);
    let count: u32 = decode_or_zero(read(&storage_key::rewarded_block_count())).map_err(|_| {
        GuardError::RewardedBlockCountInvalid {
            expected: expected_count,
            found: u32::MAX,
        }
    })?;
    if count != expected_count {
        return Err(GuardError::RewardedBlockCountInvalid {
            expected: expected_count,
            found: count,
        });
    }
    let expected_issued = expected_total(block);
    let issued: u128 =
        decode_or_zero(read(&storage_key::total_fullnode_issued())).map_err(|_| {
            GuardError::TotalFullnodeIssuedInvalid {
                expected: expected_issued,
                found: u128::MAX,
            }
        })?;
    if issued != expected_issued {
        return Err(GuardError::TotalFullnodeIssuedInvalid {
            expected: expected_issued,
            found: issued,
        });
    }
    let audit = decode_audit(read(&storage_key::last_reward_audit()))
        .map_err(|_| GuardError::RewardAuditInvalid)?;
    let _: u128 = decode_required(read(&storage_key::total_issuance()))
        .map_err(|_| GuardError::TotalIssuanceMissing)?;
    if expected_count == 0 {
        return if audit.is_none() {
            Ok(())
        } else {
            Err(GuardError::RewardAuditUnexpected)
        };
    }
    let (audit_block, miner, reward_account_id, amount) =
        audit.ok_or(GuardError::RewardAuditMissing)?;
    let expected_last_block = block.min(FULLNODE_REWARD_END_BLOCK);
    if audit_block != expected_last_block || amount != FULLNODE_BLOCK_REWARD {
        return Err(GuardError::RewardAuditInvalid);
    }
    let last_authored: u32 = decode_or_zero(read(&storage_key::last_authored(&miner)))
        .map_err(|_| GuardError::LastAuthoredBlockInvalid)?;
    if last_authored != expected_last_block {
        return Err(GuardError::LastAuthoredBlockInvalid);
    }
    if block <= FULLNODE_REWARD_END_BLOCK {
        let bound: Option<[u8; 32]> = read(&storage_key::reward_account_id(&miner))
            .map(|bytes| decode_exact::<[u8; 32]>(&bytes))
            .transpose()
            .map_err(|_| GuardError::RewardAuditInvalid)?;
        if bound.unwrap_or(miner) != reward_account_id
            || free_balance(&read, &reward_account_id)? < FULLNODE_BLOCK_REWARD
        {
            return Err(GuardError::RewardAuditInvalid);
        }
    }
    Ok(())
}

/// 节点启动时直接从 block#0 后端读取发行累计状态，阻止污染创世基准启动。
pub fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let count: u32 = decode_or_zero(read(&storage_key::rewarded_block_count())).map_err(|_| {
        GuardError::RewardedBlockCountInvalid {
            expected: 0,
            found: u32::MAX,
        }
    })?;
    if count != 0 {
        return Err(GuardError::RewardedBlockCountInvalid {
            expected: 0,
            found: count,
        });
    }
    let issued: u128 =
        decode_or_zero(read(&storage_key::total_fullnode_issued())).map_err(|_| {
            GuardError::TotalFullnodeIssuedInvalid {
                expected: 0,
                found: u128::MAX,
            }
        })?;
    if issued != 0 {
        return Err(GuardError::TotalFullnodeIssuedInvalid {
            expected: 0,
            found: issued,
        });
    }
    if decode_audit(read(&storage_key::last_reward_audit()))
        .map_err(|_| GuardError::RewardAuditInvalid)?
        .is_some()
    {
        return Err(GuardError::RewardAuditUnexpected);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;

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
        super::check_transition(
            block,
            author,
            parent,
            pre_finalize,
            post,
            &mut FinalizeIssuancePlan::default(),
        )
    }

    fn put<T: Encode>(map: &mut BTreeMap<Vec<u8>, Vec<u8>>, key: Vec<u8>, value: T) {
        map.insert(key, value.encode());
    }

    #[test]
    fn raw_storage_keys_match_runtime_contract() {
        use sp_crypto_hashing::{blake2_128, twox_128};

        let account = [5u8; 32];
        let mut expected_reward_account_id = twox_128(b"FullnodeIssuance").to_vec();
        expected_reward_account_id.extend_from_slice(&twox_128(b"RewardAccountIdByMiner"));
        expected_reward_account_id.extend_from_slice(&blake2_128(&account));
        expected_reward_account_id.extend_from_slice(&account);
        assert_eq!(
            storage_key::reward_account_id(&account),
            expected_reward_account_id
        );

        let mut expected_total = twox_128(b"Balances").to_vec();
        expected_total.extend_from_slice(&twox_128(b"TotalIssuance"));
        assert_eq!(storage_key::total_issuance(), expected_total);
    }

    fn account(free: u128) -> Vec<u8> {
        (0u32, 0u32, 1u32, 0u32, (free, 0u128, 0u128, 0u128)).encode()
    }

    #[allow(clippy::type_complexity)]
    fn valid_transition(
        block: u32,
        author: [u8; 32],
        recipient_account_id: [u8; 32],
    ) -> (
        BTreeMap<Vec<u8>, Vec<u8>>,
        BTreeMap<Vec<u8>, Vec<u8>>,
        BTreeMap<Vec<u8>, Vec<u8>>,
    ) {
        let before_count = expected_rewarded_blocks(block - 1);
        let after_count = expected_rewarded_blocks(block);
        let mut parent = BTreeMap::new();
        let mut pre = BTreeMap::new();
        let mut post = BTreeMap::new();
        for map in [&mut parent, &mut pre] {
            put(map, storage_key::rewarded_block_count(), before_count);
            put(
                map,
                storage_key::total_fullnode_issued(),
                expected_total(block - 1),
            );
            put(map, storage_key::total_issuance(), 10_000_000u128);
            map.insert(
                storage_key::system_account(&recipient_account_id),
                account(1_000),
            );
            if recipient_account_id != author {
                put(
                    map,
                    storage_key::reward_account_id(&author),
                    recipient_account_id,
                );
            }
        }
        put(&mut post, storage_key::rewarded_block_count(), after_count);
        put(
            &mut post,
            storage_key::total_fullnode_issued(),
            expected_total(block),
        );
        put(
            &mut post,
            storage_key::total_issuance(),
            10_000_000u128 + FULLNODE_BLOCK_REWARD,
        );
        post.insert(
            storage_key::system_account(&recipient_account_id),
            account(1_000 + FULLNODE_BLOCK_REWARD),
        );
        put(&mut post, storage_key::last_authored(&author), block);
        put(
            &mut post,
            storage_key::last_reward_audit(),
            (block, author, recipient_account_id, FULLNODE_BLOCK_REWARD),
        );
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
            storage_key::last_reward_audit(),
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
            Err(GuardError::RewardAuditInvalid)
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
            put(&mut post, storage_key::last_reward_audit(), audit);
            assert_eq!(
                check_transition(
                    1,
                    Some(author),
                    |key| parent.get(key).cloned(),
                    |key| pre.get(key).cloned(),
                    |key| post.get(key).cloned(),
                ),
                Err(GuardError::RewardAuditInvalid)
            );
        }

        let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
        put(&mut post, storage_key::last_authored(&author), 2u32);
        assert_eq!(
            check_transition(
                1,
                Some(author),
                |key| parent.get(key).cloned(),
                |key| pre.get(key).cloned(),
                |key| post.get(key).cloned(),
            ),
            Err(GuardError::LastAuthoredBlockInvalid)
        );
    }

    #[test]
    fn reward_is_registered_in_shared_finalize_issuance_plan() {
        let author = [8u8; 32];
        let (parent, pre, post) = valid_transition(1, author, author);
        let mut plan = FinalizeIssuancePlan::default();
        assert_eq!(
            super::check_transition(
                1,
                Some(author),
                |key| parent.get(key).cloned(),
                |key| pre.get(key).cloned(),
                |key| post.get(key).cloned(),
                &mut plan,
            ),
            Ok(())
        );
        assert_eq!(plan.accounts.get(&author), Some(&FULLNODE_BLOCK_REWARD));
        assert_eq!(plan.total, FULLNODE_BLOCK_REWARD);
    }

    #[test]
    fn imported_state_missing_balances_total_issuance_is_rejected() {
        let author = [11u8; 32];
        let (_parent, _pre, mut post) = valid_transition(1, author, author);
        post.remove(&storage_key::total_issuance());
        assert_eq!(
            check_imported_state_key_values(1, post.iter()),
            Err(GuardError::TotalIssuanceMissing)
        );
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
                storage_key::rewarded_block_count(),
                expected_rewarded_blocks(block),
            );
            put(
                map,
                storage_key::total_fullnode_issued(),
                expected_total(block),
            );
            put(map, storage_key::last_reward_audit(), audit);
            put(map, storage_key::total_issuance(), 50_000_000u128);
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
            storage_key::last_reward_audit(),
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
            Err(GuardError::RewardAuditUnexpected)
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
            Err(GuardError::PowAuthorMissing)
        );
        put(&mut pre, storage_key::rewarded_block_count(), 1u32);
        assert_eq!(
            check_transition(
                1,
                Some(author),
                |key| parent.get(key).cloned(),
                |key| pre.get(key).cloned(),
                |key| post.get(key).cloned(),
            ),
            Err(GuardError::CounterChangedBeforeFinalize)
        );
    }

    #[test]
    fn warp_state_rejects_wrong_cumulative_issuance() {
        let author = [6u8; 32];
        let mut state = BTreeMap::new();
        put(&mut state, storage_key::rewarded_block_count(), 1u32);
        put(
            &mut state,
            storage_key::total_fullnode_issued(),
            FULLNODE_BLOCK_REWARD - 1,
        );
        put(&mut state, storage_key::last_authored(&author), 1u32);
        put(
            &mut state,
            storage_key::last_reward_audit(),
            (1u32, author, author, FULLNODE_BLOCK_REWARD),
        );
        put(
            &mut state,
            storage_key::total_issuance(),
            10_000_000u128 + FULLNODE_BLOCK_REWARD,
        );
        state.insert(
            storage_key::system_account(&author),
            account(FULLNODE_BLOCK_REWARD),
        );
        assert_eq!(
            check_imported_state_key_values(1, state.iter()),
            Err(GuardError::TotalFullnodeIssuedInvalid {
                expected: FULLNODE_BLOCK_REWARD,
                found: FULLNODE_BLOCK_REWARD - 1,
            })
        );
    }

    #[test]
    fn warp_state_accepts_exact_first_reward_state() {
        let author = [10u8; 32];
        let mut state = BTreeMap::new();
        put(&mut state, storage_key::rewarded_block_count(), 1u32);
        put(
            &mut state,
            storage_key::total_fullnode_issued(),
            FULLNODE_BLOCK_REWARD,
        );
        put(&mut state, storage_key::last_authored(&author), 1u32);
        put(
            &mut state,
            storage_key::last_reward_audit(),
            (1u32, author, author, FULLNODE_BLOCK_REWARD),
        );
        put(
            &mut state,
            storage_key::total_issuance(),
            10_000_000u128 + FULLNODE_BLOCK_REWARD,
        );
        state.insert(
            storage_key::system_account(&author),
            account(FULLNODE_BLOCK_REWARD),
        );
        assert_eq!(check_imported_state_key_values(1, state.iter()), Ok(()));
    }

    #[test]
    fn genesis_rejects_nonzero_audit_state() {
        let mut state = BTreeMap::new();
        put(&mut state, storage_key::rewarded_block_count(), 1u32);
        assert_eq!(
            check_genesis(|key| state.get(key).cloned()),
            Err(GuardError::RewardedBlockCountInvalid {
                expected: 0,
                found: 1,
            })
        );
    }

    #[test]
    fn scale_values_with_trailing_bytes_are_rejected() {
        let author = [12u8; 32];
        let reward_account_id = [13u8; 32];
        let (parent, pre, mut post) = valid_transition(1, author, reward_account_id);
        post.get_mut(&storage_key::rewarded_block_count())
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
            Err(GuardError::RewardedBlockCountInvalid {
                expected: 1,
                found: u32::MAX,
            })
        );

        let (parent, mut pre, post) = valid_transition(1, author, reward_account_id);
        pre.get_mut(&storage_key::reward_account_id(&author))
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
            Err(GuardError::RewardAccountDecodeFailed)
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

        exact_header.digest_mut().logs[0] =
            DigestItem::PreRuntime(sp_consensus_pow::POW_ENGINE_ID, {
                let mut bytes = author.encode();
                bytes.push(0xff);
                bytes
            });
        assert_eq!(author_from_header(&exact_header), None);
    }
}
