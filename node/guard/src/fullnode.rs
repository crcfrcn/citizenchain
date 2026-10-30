use crate::guard::FinalizeIssuancePlan;
use crate::guard::MAccountInfo;
use codec::Decode;
use primitives::pow_const::FULLNODE_BLOCK_REWARD;
use primitives::pow_const::FULLNODE_REWARD_END_BLOCK;
use primitives::pow_const::FULLNODE_REWARD_START_BLOCK;
use runtime::opaque::Header;

use sp_runtime::traits::Header as HeaderT;
use std::collections::BTreeMap;

const FULLNODE_PALLET: &[u8] = b"FullnodeIssuance";

const BALANCES_PALLET: &[u8] = b"Balances";

const SYSTEM_PALLET: &[u8] = b"System";

/// 节点镜像的 `frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>`。
/// 节点只读取 free；其余字段占位保持 SCALE 顺序。
#[derive(Debug, PartialEq)]
pub(crate) enum GuardError {
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
    match read(&crate::guard::fullnode_issuance::storage_key::system_account(account)) {
        Some(bytes) => crate::guard::fullnode_issuance::decode_exact::<MAccountInfo>(&bytes)
            .map(|info| info.data.free)
            .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAccountDecodeFailed),
        None => Ok(0),
    }
}

pub(crate) fn expected_rewarded_blocks(block: u32) -> u32 {
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
pub(crate) fn author_from_header(header: &Header) -> Option<[u8; 32]> {
    header.digest().logs().iter().find_map(|digest| {
        let (engine_id, data) = digest.as_pre_runtime()?;
        if engine_id != sp_consensus_pow::POW_ENGINE_ID {
            return None;
        }
        crate::guard::fullnode_issuance::decode_exact::<sp_core::sr25519::Public>(data)
            .ok()
            .map(|public| public.0)
    })
}

/// 校验一个普通执行型区块在 finalize 前后的全节点奖励状态变化。
pub(crate) fn check_transition<FParent, FPre, FPost>(
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
    let author = author.ok_or(crate::guard::fullnode_issuance::GuardError::PowAuthorMissing)?;
    let previous_block = block.saturating_sub(1);
    let expected_before_count = expected_rewarded_blocks(previous_block);
    let expected_after_count = expected_rewarded_blocks(block);
    let expected_before_total = expected_total(previous_block);
    let expected_after_total = expected_total(block);

    let parent_count: u32 = decode_or_zero(parent(
        &crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize)?;
    let pre_count: u32 = decode_or_zero(pre_finalize(
        &crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize)?;
    if parent_count != expected_before_count || pre_count != parent_count {
        return Err(crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize);
    }
    let post_count: u32 = decode_or_zero(post(
        &crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
            expected: expected_after_count,
            found: u32::MAX,
        },
    )?;
    if post_count != expected_after_count {
        return Err(
            crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
                expected: expected_after_count,
                found: post_count,
            },
        );
    }

    let parent_total: u128 = decode_or_zero(parent(
        &crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize)?;
    let pre_total: u128 = decode_or_zero(pre_finalize(
        &crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize)?;
    if parent_total != expected_before_total || pre_total != parent_total {
        return Err(crate::guard::fullnode_issuance::GuardError::CounterChangedBeforeFinalize);
    }
    let post_total: u128 = decode_or_zero(post(
        &crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
            expected: expected_after_total,
            found: u128::MAX,
        },
    )?;
    if post_total != expected_after_total {
        return Err(
            crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
                expected: expected_after_total,
                found: post_total,
            },
        );
    }

    let parent_audit = decode_audit(parent(
        &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::AuditChangedBeforeFinalize)?;
    let pre_audit = decode_audit(pre_finalize(
        &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::AuditChangedBeforeFinalize)?;
    if pre_audit != parent_audit {
        return Err(crate::guard::fullnode_issuance::GuardError::AuditChangedBeforeFinalize);
    }

    let in_reward_range = expected_after_count > expected_before_count;
    if !in_reward_range {
        if decode_audit(post(
            &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
        ))
        .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)?
            != pre_audit
        {
            return Err(crate::guard::fullnode_issuance::GuardError::RewardAuditUnexpected);
        }
        return Ok(());
    }

    let recipient_account_id = match pre_finalize(
        &crate::guard::fullnode_issuance::storage_key::reward_account_id(&author),
    ) {
        Some(bytes) => crate::guard::fullnode_issuance::decode_exact::<[u8; 32]>(&bytes)
            .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAccountDecodeFailed)?,
        None => author,
    };
    let audit = decode_audit(post(
        &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)?
    .ok_or(crate::guard::fullnode_issuance::GuardError::RewardAuditMissing)?;
    if audit != (block, author, recipient_account_id, FULLNODE_BLOCK_REWARD) {
        return Err(crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid);
    }
    let last_authored: u32 = decode_or_zero(post(
        &crate::guard::fullnode_issuance::storage_key::last_authored(&author),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::LastAuthoredBlockInvalid)?;
    if last_authored != block {
        return Err(crate::guard::fullnode_issuance::GuardError::LastAuthoredBlockInvalid);
    }

    issuance_plan
        .add(recipient_account_id, FULLNODE_BLOCK_REWARD)
        .map_err(|_| crate::guard::fullnode_issuance::GuardError::FinalizeIssuanceOverflow)?;
    Ok(())
}

/// warp/完整状态导入只能验证目标态累计公式；历史逐块实际到账由提供该 finalized 状态的守卫节点背书。
pub(crate) fn check_imported_state_key_values<'a, I>(block: u32, pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let mut map = BTreeMap::new();
    let fullnode_prefix = crate::guard::fullnode_issuance::storage_key::pallet_prefix();
    let system_prefix = crate::guard::fullnode_issuance::storage_key::system_prefix();
    let total_issuance = crate::guard::fullnode_issuance::storage_key::total_issuance();
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
    let count: u32 = decode_or_zero(read(
        &crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
            expected: expected_count,
            found: u32::MAX,
        },
    )?;
    if count != expected_count {
        return Err(
            crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
                expected: expected_count,
                found: count,
            },
        );
    }
    let expected_issued = expected_total(block);
    let issued: u128 = decode_or_zero(read(
        &crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
            expected: expected_issued,
            found: u128::MAX,
        },
    )?;
    if issued != expected_issued {
        return Err(
            crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
                expected: expected_issued,
                found: issued,
            },
        );
    }
    let audit = decode_audit(read(
        &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)?;
    let _: u128 = decode_required(read(
        &crate::guard::fullnode_issuance::storage_key::total_issuance(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::TotalIssuanceMissing)?;
    if expected_count == 0 {
        return if audit.is_none() {
            Ok(())
        } else {
            Err(crate::guard::fullnode_issuance::GuardError::RewardAuditUnexpected)
        };
    }
    let (audit_block, miner, reward_account_id, amount) =
        audit.ok_or(crate::guard::fullnode_issuance::GuardError::RewardAuditMissing)?;
    let expected_last_block = block.min(FULLNODE_REWARD_END_BLOCK);
    if audit_block != expected_last_block || amount != FULLNODE_BLOCK_REWARD {
        return Err(crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid);
    }
    let last_authored: u32 = decode_or_zero(read(
        &crate::guard::fullnode_issuance::storage_key::last_authored(&miner),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::LastAuthoredBlockInvalid)?;
    if last_authored != expected_last_block {
        return Err(crate::guard::fullnode_issuance::GuardError::LastAuthoredBlockInvalid);
    }
    if block <= FULLNODE_REWARD_END_BLOCK {
        let bound: Option<[u8; 32]> =
            read(&crate::guard::fullnode_issuance::storage_key::reward_account_id(&miner))
                .map(|bytes| crate::guard::fullnode_issuance::decode_exact::<[u8; 32]>(&bytes))
                .transpose()
                .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)?;
        if bound.unwrap_or(miner) != reward_account_id
            || free_balance(&read, &reward_account_id)? < FULLNODE_BLOCK_REWARD
        {
            return Err(crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid);
        }
    }
    Ok(())
}

/// 节点启动时直接从 block#0 后端读取发行累计状态，阻止污染创世基准启动。
pub(crate) fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let count: u32 = decode_or_zero(read(
        &crate::guard::fullnode_issuance::storage_key::rewarded_block_count(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
            expected: 0,
            found: u32::MAX,
        },
    )?;
    if count != 0 {
        return Err(
            crate::guard::fullnode_issuance::GuardError::RewardedBlockCountInvalid {
                expected: 0,
                found: count,
            },
        );
    }
    let issued: u128 = decode_or_zero(read(
        &crate::guard::fullnode_issuance::storage_key::total_fullnode_issued(),
    ))
    .map_err(
        |_| crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
            expected: 0,
            found: u128::MAX,
        },
    )?;
    if issued != 0 {
        return Err(
            crate::guard::fullnode_issuance::GuardError::TotalFullnodeIssuedInvalid {
                expected: 0,
                found: issued,
            },
        );
    }
    if decode_audit(read(
        &crate::guard::fullnode_issuance::storage_key::last_reward_audit(),
    ))
    .map_err(|_| crate::guard::fullnode_issuance::GuardError::RewardAuditInvalid)?
    .is_some()
    {
        return Err(crate::guard::fullnode_issuance::GuardError::RewardAuditUnexpected);
    }
    Ok(())
}

pub mod storage_key {
    use super::*;
    use crate::guard::fullnode_issuance::BALANCES_PALLET;
    use crate::guard::fullnode_issuance::FULLNODE_PALLET;
    use crate::guard::fullnode_issuance::SYSTEM_PALLET;
    use sp_crypto_hashing::twox_128;

    // `nodedesktop::storage` 单源的薄委托;blake2_map 传裸 32 字节账户(AccountId32 无长度前缀)。
    fn storage_value(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn blake2_map(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, encoded_key)
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
#[cfg(test)]
#[path = "../tests/fullnode.rs"]
mod tests;
