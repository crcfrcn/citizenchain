use super::*;

use crate::guard::provincialbank_interest::check_imported_state;
use crate::guard::provincialbank_interest::check_transition;
use crate::guard::provincialbank_interest::expected_annual_audit;
use crate::guard::provincialbank_interest::expected_total;
use crate::guard::provincialbank_interest::expected_year;
use crate::guard::provincialbank_interest::interest_bp;
use crate::guard::provincialbank_interest::interest_for_principal;
use crate::guard::provincialbank_interest::NEW_BALANCES_FLAGS;
use codec::Encode;
use primitives::cid::china::china_ch::CHINA_CH;
use primitives::pow_const::BLOCKS_PER_YEAR;
use sp_crypto_hashing::blake2_128;
use sp_crypto_hashing::twox_128;
use std::collections::BTreeMap;

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
                crate::guard::provincialbank_interest::storage_key::system_account(
                    &bank.stake_account,
                ),
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
    assert_eq!(
        crate::guard::provincialbank_interest::storage_key::system_account(&account),
        expected
    );

    let mut audit = twox_128(b"ProvincialBankInterest").to_vec();
    audit.extend_from_slice(&twox_128(b"LastProvincialBankInterestAudit"));
    assert_eq!(
        crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
        audit
    );
}

#[test]
fn genesis_allows_stake_topup_but_rejects_principal_spend() {
    let state = genesis_state();
    assert_eq!(check_imported_state(0, state.iter()), Ok(()));

    let first = &CHINA_CH[0];

    // 转入/增加（free > 本金）放行:质押账户只禁支出，不禁转入。
    let mut topped_up = state.clone();
    topped_up.insert(
        crate::guard::provincialbank_interest::storage_key::system_account(&first.stake_account),
        account(first.stake_amount + 1),
    );
    assert_eq!(check_imported_state(0, topped_up.iter()), Ok(()));

    // 支出/减少（free < 本金）拒绝:本金被动用即违规。
    let mut spent = state.clone();
    spent.insert(
        crate::guard::provincialbank_interest::storage_key::system_account(&first.stake_account),
        account(first.stake_amount - 1),
    );
    assert_eq!(
        check_imported_state(0, spent.iter()),
        Err(
            crate::guard::provincialbank_interest::GuardError::StakeAccountChanged(
                first.stake_account
            )
        )
    );
}

#[test]
fn genesis_rejects_unknown_interest_storage() {
    let mut state = genesis_state();
    let mut unknown = crate::guard::provincialbank_interest::storage_key::pallet_prefix().to_vec();
    unknown.extend_from_slice(&twox_128(b"ShadowInterest"));
    state.insert(unknown.clone(), 1u32.encode());
    assert_eq!(
        check_imported_state(0, state.iter()),
        Err(crate::guard::provincialbank_interest::GuardError::UnknownStorageKey(unknown))
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

fn settled_state(year: u32) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut state = genesis_state();
    if year > 0 {
        state.insert(
            crate::guard::provincialbank_interest::storage_key::last_settled_year(),
            year.encode(),
        );
        state.insert(
            crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
            expected_total(year).unwrap().encode(),
        );
        state.insert(
            crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
            expected_annual_audit(year).unwrap().encode(),
        );
    }
    state
}

#[test]
fn imported_state_uses_actual_height_at_annual_boundaries_and_final_year() {
    let year = BLOCKS_PER_YEAR as u32;
    for (block, settled) in [
        (0, 0),
        (1, 0),
        (year - 1, 0),
        (year, 1),
        (year + 1, 1),
        (100 * year - 1, 99),
        (100 * year, 100),
        (100 * year + 1, 100),
        (101 * year, 100),
        (u32::MAX, 100),
    ] {
        assert_eq!(
            check_imported_state(block, settled_state(settled).iter()),
            Ok(()),
            "block {block}"
        );
    }
    let state = settled_state(1);
    for block in [0, year - 1, 2 * year] {
        assert!(matches!(
            check_imported_state(block, state.iter()),
            Err(crate::guard::provincialbank_interest::GuardError::LastSettledYearInvalid { .. })
        ));
    }
}

#[test]
fn imported_running_state_rejects_wrong_audit_principal_and_unknown_key() {
    let block = BLOCKS_PER_YEAR as u32;
    let valid = settled_state(1);
    let mut state = valid.clone();
    state.insert(
        crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
        (expected_total(1).unwrap() + 1).encode(),
    );
    assert!(matches!(
        check_imported_state(block, state.iter()),
        Err(crate::guard::provincialbank_interest::GuardError::TotalInterestIssuedInvalid { .. })
    ));
    let mut state = valid.clone();
    state.remove(&crate::guard::provincialbank_interest::storage_key::last_interest_audit());
    assert_eq!(
        check_imported_state(block, state.iter()),
        Err(crate::guard::provincialbank_interest::GuardError::LastAuditMissing)
    );
    let mut state = valid.clone();
    let mut audit = expected_annual_audit(1).unwrap();
    audit.bank_count -= 1;
    state.insert(
        crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
        audit.encode(),
    );
    assert_eq!(
        check_imported_state(block, state.iter()),
        Err(crate::guard::provincialbank_interest::GuardError::LastAuditInvalid)
    );
    let first = &CHINA_CH[0];
    let mut state = valid.clone();
    state.insert(
        crate::guard::provincialbank_interest::storage_key::system_account(&first.stake_account),
        account(first.stake_amount - 1),
    );
    assert_eq!(
        check_imported_state(block, state.iter()),
        Err(
            crate::guard::provincialbank_interest::GuardError::StakeAccountChanged(
                first.stake_account
            )
        )
    );
    let mut state = valid;
    let mut unknown = crate::guard::provincialbank_interest::storage_key::pallet_prefix().to_vec();
    unknown.extend_from_slice(&twox_128(b"ShadowInterest"));
    state.insert(unknown.clone(), 1u32.encode());
    assert_eq!(
        check_imported_state(block, state.iter()),
        Err(crate::guard::provincialbank_interest::GuardError::UnknownStorageKey(unknown))
    );
}

#[test]
fn annual_transition_registers_exact_forty_three_account_plan() {
    let parent = genesis_state();
    let pre = parent.clone();
    let mut post = parent.clone();
    let audit = expected_annual_audit(1).unwrap();
    post.insert(
        crate::guard::provincialbank_interest::storage_key::last_settled_year(),
        1u32.encode(),
    );
    post.insert(
        crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
        audit.total_interest.encode(),
    );
    post.insert(
        crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
        audit.encode(),
    );
    let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> = post
        .iter()
        .filter(|(key, value)| parent.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect();
    let mut plan = crate::guard::FinalizeIssuancePlan::default();

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
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let mut pre = genesis_state();
    pre.insert(total_key.clone(), 1_000_000u128.encode());
    let mut post = pre.clone();
    let audit = expected_annual_audit(1).unwrap();
    post.insert(
        crate::guard::provincialbank_interest::storage_key::last_settled_year(),
        1u32.encode(),
    );
    post.insert(
        crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
        audit.total_interest.encode(),
    );
    post.insert(
        crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
        audit.encode(),
    );
    post.insert(total_key, (1_000_000u128 + audit.total_interest).encode());
    for bank in CHINA_CH.iter() {
        post.insert(
            crate::guard::provincialbank_interest::storage_key::system_account(&bank.main_account),
            account(interest_for_principal(bank.stake_amount, 1).unwrap()),
        );
    }
    let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> = post
        .iter()
        .filter(|(key, value)| pre.get(key.as_slice()) != Some(*value))
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect();
    let mut plan = crate::guard::FinalizeIssuancePlan::default();
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

    crate::guard::verify_finalize_issuance(
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
    wrong_audit.insert(
        crate::guard::provincialbank_interest::storage_key::last_settled_year(),
        1u32.encode(),
    );
    wrong_audit.insert(
        crate::guard::provincialbank_interest::storage_key::total_interest_issued(),
        (audit.total_interest + 1).encode(),
    );
    wrong_audit.insert(
        crate::guard::provincialbank_interest::storage_key::last_interest_audit(),
        audit.encode(),
    );
    assert!(matches!(
        check_transition(
            BLOCKS_PER_YEAR as u32,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &|key| parent.get(key).cloned(),
            &|key| pre.get(key).cloned(),
            &|key| wrong_audit.get(key).cloned(),
            &mut crate::guard::FinalizeIssuancePlan::default(),
        ),
        Err(crate::guard::provincialbank_interest::GuardError::TotalInterestIssuedInvalid { .. })
    ));

    let first = &CHINA_CH[0];
    let key =
        crate::guard::provincialbank_interest::storage_key::system_account(&first.stake_account);

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
        &mut crate::guard::FinalizeIssuancePlan::default(),
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
        &mut crate::guard::FinalizeIssuancePlan::default(),
    )
    .is_ok());
}
