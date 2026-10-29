//! 私有费用取整算法与公开费率的原单元测试。

use super::*;

/// 链上手续费三方分账必须等于 100%。
#[test]
fn onchain_fee_percents_sum_to_100() {
    assert_eq!(
        ONCHAIN_FEE_FULLNODE_PERCENT + ONCHAIN_FEE_NRC_PERCENT + ONCHAIN_FEE_SAFETY_FUND_PERCENT,
        100
    );
}

/// 投票统一价为 1 元。
#[test]
fn vote_flat_fee_equals_one_yuan() {
    assert_eq!(VOTE_FLAT_FEE, 100);
}

/// 链下费率上下限合法。
#[test]
fn offchain_rate_bounds_consistent() {
    assert!(
        OFFCHAIN_FEE_RATE_MIN.deconstruct() <= OFFCHAIN_FEE_RATE_MAX.deconstruct(),
        "OFFCHAIN_FEE_RATE_MIN must not exceed OFFCHAIN_FEE_RATE_MAX"
    );
}

/// 链上费率必须大于 0。
#[test]
fn onchain_rate_positive() {
    assert!(ONCHAIN_FEE_RATE.deconstruct() > 0);
}

/// tip 永远为零，任何非零输入必须由收费执行器直接拒绝。
#[test]
fn transaction_tip_is_disabled() {
    assert_eq!(TRANSACTION_TIP, 0);
}

#[test]
fn onchain_fee_uses_rate_rounding_and_minimum() {
    assert_eq!(calculate_onchain_fee(0), ONCHAIN_MIN_FEE);
    assert_eq!(calculate_onchain_fee(499), ONCHAIN_MIN_FEE);
    assert_eq!(calculate_onchain_fee(50_000), 50);
    assert_eq!(mul_perbill_round(500, ONCHAIN_FEE_RATE), 1);
    assert_eq!(mul_perbill_round(499, ONCHAIN_FEE_RATE), 0);
}

#[test]
fn onchain_fee_handles_u128_max_without_distortion() {
    assert_eq!(
        mul_perbill_round(u128::MAX, Perbill::from_percent(100)),
        u128::MAX
    );
}

/// 链下费率 Perbill 到 bp 的换算保持稳定。
#[test]
fn offchain_perbill_to_bp_conversion_stable() {
    assert_eq!(OFFCHAIN_FEE_RATE_MIN.deconstruct() / 100_000, 1);
    assert_eq!(OFFCHAIN_FEE_RATE_MAX.deconstruct() / 100_000, 10);
}
