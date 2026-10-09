//! 省储行创立质押本金与固定年度利息节点永久策略。
//!
//! 节点直接使用编译期 `CHINA_CH`、利率和年度常量复算，不读取 runtime metadata 或 runtime API。
//! 43 个 `stake_account` 的创世本金必须永久保持原样；每个年度边界的利息只能在 finalize
//! 发到对应 `main_account`，并与累计量、最近审计及统一原生发行计划逐项闭环。

use std::collections::{BTreeMap, BTreeSet};

use codec::{Decode, Encode};
use sp_crypto_hashing::twox_128;

use primitives::{
    cid::china::china_ch::CHINA_CH,
    core_const::{
        PROVINCIALBANK_INITIAL_INTEREST_BP, PROVINCIALBANK_INTEREST_DECREASE_BP,
        PROVINCIALBANK_INTEREST_DURATION_YEARS,
    },
    pow_const::BLOCKS_PER_YEAR,
};

use super::{FinalizeIssuancePlan, MAccountInfo};

const PALLET_NAME: &[u8] = b"ProvincialBankInterest";
const SYSTEM_PALLET: &[u8] = b"System";
const BASIS_POINTS_DENOMINATOR: u128 = 10_000;
const NEW_BALANCES_FLAGS: u128 = 0x80000000_00000000_00000000_00000000;

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct MProvincialBankInterestAudit {
    year: u32,
    bank_count: u32,
    total_interest: u128,
}

#[derive(Debug, Eq, PartialEq)]
pub enum GuardError {
    StorageValueDecodeFailed(&'static str),
    UnknownStorageKey(Vec<u8>),
    StorageVersionInvalid(u16),
    LastSettledYearInvalid { expected: u32, found: u32 },
    TotalInterestIssuedInvalid { expected: u128, found: u128 },
    LastAuditMissing,
    LastAuditUnexpected,
    LastAuditInvalid,
    StateChangedBeforeFinalize,
    InterestOverflow,
    FinalizeIssuanceOverflow,
    StakeAccountMissing([u8; 32]),
    StakeAccountDecodeFailed([u8; 32]),
    StakeAccountChanged([u8; 32]),
}

pub mod storage_key {
    use super::*;

    fn storage_value(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(pallet, storage)
    }

    fn blake2_map(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_map(pallet, storage, encoded_key)
    }

    pub fn pallet_prefix() -> [u8; 16] {
        twox_128(PALLET_NAME)
    }

    pub fn last_settled_year() -> Vec<u8> {
        storage_value(PALLET_NAME, b"LastSettledYear")
    }

    pub fn total_interest_issued() -> Vec<u8> {
        storage_value(PALLET_NAME, b"TotalProvincialBankInterestIssued")
    }

    pub fn last_interest_audit() -> Vec<u8> {
        storage_value(PALLET_NAME, b"LastProvincialBankInterestAudit")
    }

    pub fn storage_version() -> Vec<u8> {
        let mut key = pallet_prefix().to_vec();
        key.extend_from_slice(&twox_128(b":__STORAGE_VERSION__:"));
        key
    }

    pub fn system_account(account: &[u8; 32]) -> Vec<u8> {
        blake2_map(SYSTEM_PALLET, b"Account", account)
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

fn decode_or_zero<T: Decode + Default>(
    raw: Option<Vec<u8>>,
    label: &'static str,
) -> Result<T, GuardError> {
    raw.map(|bytes| decode_exact(&bytes, label))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn expected_year(block: u32) -> u32 {
    u32::try_from(u64::from(block) / BLOCKS_PER_YEAR)
        .unwrap_or(u32::MAX)
        .min(PROVINCIALBANK_INTEREST_DURATION_YEARS)
}

fn interest_bp(year: u32) -> u32 {
    PROVINCIALBANK_INITIAL_INTEREST_BP.saturating_sub(
        year.saturating_sub(1)
            .saturating_mul(PROVINCIALBANK_INTEREST_DECREASE_BP),
    )
}

fn interest_for_principal(principal: u128, year: u32) -> Result<u128, GuardError> {
    principal
        .checked_mul(u128::from(interest_bp(year)))
        .map(|gross| gross / BASIS_POINTS_DENOMINATOR)
        .ok_or(GuardError::InterestOverflow)
}

fn expected_annual_audit(year: u32) -> Result<MProvincialBankInterestAudit, GuardError> {
    let principal_units = CHINA_CH.iter().try_fold(0u128, |total, bank| {
        if bank.stake_amount % BASIS_POINTS_DENOMINATOR != 0 {
            return Err(GuardError::InterestOverflow);
        }
        total
            .checked_add(bank.stake_amount / BASIS_POINTS_DENOMINATOR)
            .ok_or(GuardError::InterestOverflow)
    })?;
    let total_interest = principal_units
        .checked_mul(u128::from(interest_bp(year)))
        .ok_or(GuardError::InterestOverflow)?;
    Ok(MProvincialBankInterestAudit {
        year,
        bank_count: u32::try_from(CHINA_CH.len()).map_err(|_| GuardError::InterestOverflow)?,
        total_interest,
    })
}

fn expected_total(last_year: u32) -> Result<u128, GuardError> {
    if last_year == 0 {
        return Ok(0);
    }
    let first = u128::from(PROVINCIALBANK_INITIAL_INTEREST_BP);
    let decrease = u128::from(PROVINCIALBANK_INTEREST_DECREASE_BP);
    let years = u128::from(last_year);
    let last = first
        .checked_sub(
            years
                .saturating_sub(1)
                .checked_mul(decrease)
                .ok_or(GuardError::InterestOverflow)?,
        )
        .ok_or(GuardError::InterestOverflow)?;
    let rate_sum = years
        .checked_mul(
            first
                .checked_add(last)
                .ok_or(GuardError::InterestOverflow)?,
        )
        .ok_or(GuardError::InterestOverflow)?
        / 2;
    let principal_units = CHINA_CH.iter().try_fold(0u128, |total, bank| {
        if bank.stake_amount % BASIS_POINTS_DENOMINATOR != 0 {
            return Err(GuardError::InterestOverflow);
        }
        total
            .checked_add(bank.stake_amount / BASIS_POINTS_DENOMINATOR)
            .ok_or(GuardError::InterestOverflow)
    })?;
    principal_units
        .checked_mul(rate_sum)
        .ok_or(GuardError::InterestOverflow)
}

fn check_audit_state<F>(block: u32, read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let storage_version: u16 =
        decode_or_zero(read(&storage_key::storage_version()), "StorageVersion")?;
    if storage_version != 0 {
        return Err(GuardError::StorageVersionInvalid(storage_version));
    }
    let year = expected_year(block);
    let found_year: u32 =
        decode_or_zero(read(&storage_key::last_settled_year()), "LastSettledYear")?;
    if found_year != year {
        return Err(GuardError::LastSettledYearInvalid {
            expected: year,
            found: found_year,
        });
    }
    let expected_total = expected_total(year)?;
    let found_total: u128 = decode_or_zero(
        read(&storage_key::total_interest_issued()),
        "TotalProvincialBankInterestIssued",
    )?;
    if found_total != expected_total {
        return Err(GuardError::TotalInterestIssuedInvalid {
            expected: expected_total,
            found: found_total,
        });
    }

    let audit = read(&storage_key::last_interest_audit())
        .map(|raw| {
            decode_exact::<MProvincialBankInterestAudit>(&raw, "LastProvincialBankInterestAudit")
        })
        .transpose()?;
    if year == 0 {
        if audit.is_some() {
            return Err(GuardError::LastAuditUnexpected);
        }
    } else if audit.ok_or(GuardError::LastAuditMissing)? != expected_annual_audit(year)? {
        return Err(GuardError::LastAuditInvalid);
    }
    Ok(())
}

fn check_stake_account<F>(read: &F, account: [u8; 32], principal: u128) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let raw = read(&storage_key::system_account(&account))
        .ok_or(GuardError::StakeAccountMissing(account))?;
    let info = decode_exact::<MAccountInfo>(&raw, "省储行 stake System::Account")
        .map_err(|_| GuardError::StakeAccountDecodeFailed(account))?;
    // 质押账户只禁「支出/减少」，允许「转入/增加」。
    // 年度利息按固定常量本金(stake_amount)计算，与账户实际余额无关，故转入无害；
    // 只要求 free 不得「低于」本金：free < principal = 被支出/被 slash → 拒;
    // free == principal(未动)或 free > principal(被转入)→ 放行。
    // 转出必须由该账户自己签名 → nonce != 0，已被下方 nonce 检查一并挡住。
    if info.nonce != 0
        || info.consumers != 0
        || info.providers != 1
        || info.sufficients != 0
        || info.data.free < principal
        || info.data.reserved != 0
        || info.data.frozen != 0
        || info.data.flags != NEW_BALANCES_FLAGS
    {
        return Err(GuardError::StakeAccountChanged(account));
    }
    Ok(())
}

fn check_all_stake_accounts<F>(read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for bank in CHINA_CH.iter() {
        check_stake_account(read, bank.stake_account, bank.stake_amount)?;
    }
    Ok(())
}

fn known_pallet_keys() -> BTreeSet<Vec<u8>> {
    [
        storage_key::last_settled_year(),
        storage_key::total_interest_issued(),
        storage_key::last_interest_audit(),
        storage_key::storage_version(),
    ]
    .into_iter()
    .collect()
}

fn reject_unknown_pallet_keys<'a, I>(keys: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = &'a Vec<u8>>,
{
    let prefix = storage_key::pallet_prefix();
    let known = known_pallet_keys();
    for key in keys {
        if key.starts_with(&prefix) && !known.contains(key) {
            return Err(GuardError::UnknownStorageKey(key.clone()));
        }
    }
    Ok(())
}

/// 节点启动和 block#0 完整导入时验证固定本金与规范空审计状态。
pub fn check_genesis<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_audit_state(0, &read)?;
    check_all_stake_accounts(&read)
}

/// 普通区块校验：年度审计只能在 finalize 原子推进，年度利息逐户登记到共享发行计划。
pub fn check_transition<FParent, FPre, FPost>(
    block: u32,
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
    reject_unknown_pallet_keys(pre_delta.keys().chain(post_delta.keys()))?;
    check_audit_state(block.saturating_sub(1), parent)?;
    check_audit_state(block.saturating_sub(1), pre)
        .map_err(|_| GuardError::StateChangedBeforeFinalize)?;
    check_audit_state(block, post)?;

    // 永久质押账户若在任一执行阶段被触及，目标状态必须仍与创世规范逐字段一致。
    for bank in CHINA_CH.iter() {
        let key = storage_key::system_account(&bank.stake_account);
        if pre_delta.contains_key(&key) || post_delta.contains_key(&key) {
            check_stake_account(parent, bank.stake_account, bank.stake_amount)?;
            check_stake_account(pre, bank.stake_account, bank.stake_amount)?;
            check_stake_account(post, bank.stake_account, bank.stake_amount)?;
        }
    }

    if expected_year(block) > expected_year(block.saturating_sub(1)) {
        let year = expected_year(block);
        for bank in CHINA_CH.iter() {
            issuance_plan
                .add(
                    bank.main_account,
                    interest_for_principal(bank.stake_amount, year)?,
                )
                .map_err(|_| GuardError::FinalizeIssuanceOverflow)?;
        }
    }
    Ok(())
}

/// runtime 升级全检时重新读取全部固定本金和当前年度审计。
pub fn check_full_state<F>(block: u32, read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_audit_state(block, read)?;
    check_all_stake_accounts(read)
}

/// 完整状态导入只接受 block#0；调用方已统一拒绝非创世 CID 状态导入。
pub fn check_imported_genesis<'a, I>(pairs: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let state: BTreeMap<Vec<u8>, Vec<u8>> = pairs
        .into_iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    reject_unknown_pallet_keys(state.keys())?;
    check_genesis(|key| state.get(key).cloned())
}

/// 完整状态单遍分区只保留本 pallet 和 43 个永久质押账户。
pub fn relevant_import_keys() -> BTreeSet<Vec<u8>> {
    let mut keys = known_pallet_keys();
    keys.extend(
        CHINA_CH
            .iter()
            .map(|bank| storage_key::system_account(&bank.stake_account)),
    );
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;
    use sp_crypto_hashing::{blake2_128, twox_128};

    fn account(principal: u128) -> Vec<u8> {
        let mut bytes = Vec::new();
        0u32.encode_to(&mut bytes);
        0u32.encode_to(&mut bytes);
        1u32.encode_to(&mut bytes);
        0u32.encode_to(&mut bytes);
        principal.encode_to(&mut bytes);
        0u128.encode_to(&mut bytes);
        0u128.encode_to(&mut bytes);
        NEW_BALANCES_FLAGS.encode_to(&mut bytes);
        bytes
    }

    fn genesis_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
        CHINA_CH
            .iter()
            .map(|bank| {
                (
                    storage_key::system_account(&bank.stake_account),
                    account(bank.stake_amount),
                )
            })
            .collect()
    }

    #[test]
    fn raw_keys_match_runtime_contract() {
        let account = [7u8; 32];
        let mut expected = twox_128(b"System").to_vec();
        expected.extend_from_slice(&twox_128(b"Account"));
        expected.extend_from_slice(&blake2_128(&account));
        expected.extend_from_slice(&account);
        assert_eq!(storage_key::system_account(&account), expected);

        let mut audit = twox_128(b"ProvincialBankInterest").to_vec();
        audit.extend_from_slice(&twox_128(b"LastProvincialBankInterestAudit"));
        assert_eq!(storage_key::last_interest_audit(), audit);
    }

    #[test]
    fn genesis_allows_stake_topup_but_rejects_principal_spend() {
        let state = genesis_state();
        assert_eq!(check_imported_genesis(state.iter()), Ok(()));

        let first = &CHINA_CH[0];

        // 转入/增加（free > 本金）放行:质押账户只禁支出，不禁转入。
        let mut topped_up = state.clone();
        topped_up.insert(
            storage_key::system_account(&first.stake_account),
            account(first.stake_amount + 1),
        );
        assert_eq!(check_imported_genesis(topped_up.iter()), Ok(()));

        // 支出/减少（free < 本金）拒绝:本金被动用即违规。
        let mut spent = state.clone();
        spent.insert(
            storage_key::system_account(&first.stake_account),
            account(first.stake_amount - 1),
        );
        assert_eq!(
            check_imported_genesis(spent.iter()),
            Err(GuardError::StakeAccountChanged(first.stake_account))
        );
    }

    #[test]
    fn genesis_rejects_unknown_interest_storage() {
        let mut state = genesis_state();
        let mut unknown = storage_key::pallet_prefix().to_vec();
        unknown.extend_from_slice(&twox_128(b"ShadowInterest"));
        state.insert(unknown.clone(), 1u32.encode());
        assert_eq!(
            check_imported_genesis(state.iter()),
            Err(GuardError::UnknownStorageKey(unknown))
        );
    }

    #[test]
    fn year_formula_is_fixed_from_one_to_one_hundred() {
        assert_eq!(interest_bp(1), 100);
        assert_eq!(interest_bp(2), 99);
        assert_eq!(interest_bp(100), 1);
        assert_eq!(expected_year((BLOCKS_PER_YEAR - 1) as u32), 0);
        assert_eq!(expected_year(BLOCKS_PER_YEAR as u32), 1);
        let iterative: u128 = (1..=100)
            .map(|year| expected_annual_audit(year).unwrap().total_interest)
            .sum();
        assert_eq!(expected_total(100), Ok(iterative));
    }

    #[test]
    fn annual_transition_registers_exact_forty_three_account_plan() {
        let parent = genesis_state();
        let pre = parent.clone();
        let mut post = parent.clone();
        let audit = expected_annual_audit(1).unwrap();
        post.insert(storage_key::last_settled_year(), 1u32.encode());
        post.insert(
            storage_key::total_interest_issued(),
            audit.total_interest.encode(),
        );
        post.insert(storage_key::last_interest_audit(), audit.encode());
        let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> = post
            .iter()
            .filter(|(key, value)| parent.get(*key) != Some(*value))
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        let mut plan = FinalizeIssuancePlan::default();

        assert_eq!(
            check_transition(
                BLOCKS_PER_YEAR as u32,
                &BTreeMap::new(),
                &post_delta,
                &|key| parent.get(key).cloned(),
                &|key| pre.get(key).cloned(),
                &|key| post.get(key).cloned(),
                &mut plan,
            ),
            Ok(())
        );
        assert_eq!(plan.accounts.len(), CHINA_CH.len());
        assert_eq!(plan.total, audit.total_interest);
        for bank in CHINA_CH.iter() {
            assert_eq!(
                plan.accounts.get(&bank.main_account),
                Some(&interest_for_principal(bank.stake_amount, 1).unwrap())
            );
        }
    }

    #[test]
    fn annual_plan_closes_with_balances_and_total_issuance() {
        let total_key = super::super::fullnode_issuance::storage_key::total_issuance();
        let mut pre = genesis_state();
        pre.insert(total_key.clone(), 1_000_000u128.encode());
        let mut post = pre.clone();
        let audit = expected_annual_audit(1).unwrap();
        post.insert(storage_key::last_settled_year(), 1u32.encode());
        post.insert(
            storage_key::total_interest_issued(),
            audit.total_interest.encode(),
        );
        post.insert(storage_key::last_interest_audit(), audit.encode());
        post.insert(total_key, (1_000_000u128 + audit.total_interest).encode());
        for bank in CHINA_CH.iter() {
            post.insert(
                storage_key::system_account(&bank.main_account),
                account(interest_for_principal(bank.stake_amount, 1).unwrap()),
            );
        }
        let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> = post
            .iter()
            .filter(|(key, value)| pre.get(key.as_slice()) != Some(*value))
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        let mut plan = FinalizeIssuancePlan::default();
        check_transition(
            BLOCKS_PER_YEAR as u32,
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &mut plan,
        )
        .expect("年度固定发行策略应生成精确计划");

        super::super::verify_finalize_issuance(
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        )
        .expect("43 家余额增量与总发行量应和固定发行计划完全闭环");
    }

    #[test]
    fn annual_transition_rejects_wrong_audit_and_stake_mutation() {
        let parent = genesis_state();
        let pre = parent.clone();
        let mut wrong_audit = parent.clone();
        let audit = expected_annual_audit(1).unwrap();
        wrong_audit.insert(storage_key::last_settled_year(), 1u32.encode());
        wrong_audit.insert(
            storage_key::total_interest_issued(),
            (audit.total_interest + 1).encode(),
        );
        wrong_audit.insert(storage_key::last_interest_audit(), audit.encode());
        assert!(matches!(
            check_transition(
                BLOCKS_PER_YEAR as u32,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &|key| parent.get(key).cloned(),
                &|key| pre.get(key).cloned(),
                &|key| wrong_audit.get(key).cloned(),
                &mut FinalizeIssuancePlan::default(),
            ),
            Err(GuardError::TotalInterestIssuedInvalid { .. })
        ));

        let first = &CHINA_CH[0];
        let key = storage_key::system_account(&first.stake_account);

        // 支出/减少质押本金 → 拒绝（本金被动用即违规）。
        let mut spent = parent.clone();
        spent.insert(key.clone(), account(first.stake_amount - 1));
        let spent_delta = BTreeMap::from([(key.clone(), Some(spent[&key].clone()))]);
        assert!(check_transition(
            1,
            &spent_delta,
            &spent_delta,
            &|key| parent.get(key).cloned(),
            &|key| spent.get(key).cloned(),
            &|key| spent.get(key).cloned(),
            &mut FinalizeIssuancePlan::default(),
        )
        .is_err());

        // 转入/增加质押账户 → 放行（只禁支出，不禁转入）。
        let mut topped_up = parent.clone();
        topped_up.insert(key.clone(), account(first.stake_amount + 1));
        let topup_delta = BTreeMap::from([(key.clone(), Some(topped_up[&key].clone()))]);
        assert!(check_transition(
            1,
            &topup_delta,
            &topup_delta,
            &|key| parent.get(key).cloned(),
            &|key| topped_up.get(key).cloned(),
            &|key| topped_up.get(key).cloned(),
            &mut FinalizeIssuancePlan::default(),
        )
        .is_ok());
    }
}
