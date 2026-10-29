//! 所属原语的真实单元测试，保留原断言与固定输入。
mod monetary {

    use crate::{core_const::*, fee_policy};

    /// 表达常量必须保留分账本、ED 和既有制度费用；转换只由 SDK 实现。
    #[test]
    fn evm_representation_preserves_native_monetary_policy() {
        assert_eq!(TOKEN_DECIMALS, 2);
        assert_eq!(TOKEN_MIN_UNIT, 1);
        assert_eq!(ACCOUNT_EXISTENTIAL_DEPOSIT, 111);
        assert_eq!(EVM_DECIMALS, 18);
        assert_eq!(NATIVE_TO_ETH_RATIO, 10u64.pow(16));
        assert_eq!(100u128 * u128::from(NATIVE_TO_ETH_RATIO), 10u128.pow(18));
        assert_eq!(fee_policy::ONCHAIN_MIN_FEE, 10);
        assert_eq!(fee_policy::OFFCHAIN_MIN_FEE, 1);
        assert_eq!(fee_policy::VOTE_FLAT_FEE, 100);
        assert_eq!(fee_policy::calculate_onchain_fee(50_000), 50);
    }
}
mod count_const {

    use crate::count_const::*;

    #[test]
    fn joint_vote_total_matches_threshold() {
        // 联合投票总票数必须等于通过阈值（全票通过制）。
        assert_eq!(JOINT_VOTE_TOTAL, JOINT_VOTE_PASS_THRESHOLD);
    }
}
mod pow_const {

    use crate::pow_const::*;

    #[test]
    fn fullnode_total_issuance_is_consistent() {
        // 全节点发行总量 = 区块奖励 × 发行区块数。
        assert_eq!(
            FULLNODE_TOTAL_ISSUANCE,
            FULLNODE_BLOCK_REWARD * FULLNODE_REWARD_BLOCK_COUNT as u128
        );
    }

    #[test]
    fn pow_target_window_uses_fixed_six_minute_average() {
        assert_eq!(POW_TARGET_BLOCK_TIME_MS, 6 * 60 * 1_000);
        assert_eq!(DIFFICULTY_TARGET_WINDOW_MS, 600 * 6 * 60 * 1_000);
    }
}
