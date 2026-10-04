//! 全节点铸块与发行常量。
//! 六分钟是 PoW 难度调整追踪的固定平均目标，不是出块等待时间或最晚期限。

/// PoW 创世难度。
pub const POW_INITIAL_DIFFICULTY: u64 = 100;

/// PoW 参数创世版本；后续参数版本只能经 runtime 升级逐次递增。
pub const POW_PARAMS_VERSION: u32 = 1;

/// 当前节点支持的首版动态难度公式。
pub const POW_ALGORITHM_VERSION: u16 = 1;

/// 创世默认难度调整周期；运行后以链上 ActiveParams 为准。
pub const DIFFICULTY_ADJUSTMENT_INTERVAL: u32 = 600;

/// 创世默认单次调整最大倍率；运行后以链上 ActiveParams 为准。
pub const DIFFICULTY_MAX_ADJUST_FACTOR: u64 = 4;

/// 创世默认单次调整最小倍率分母；运行后以链上 ActiveParams 为准。
pub const DIFFICULTY_MIN_ADJUST_FACTOR: u64 = 4;

/// 创世默认难度调整目标窗口，仅用于创世与防漂移测试。
pub const DIFFICULTY_TARGET_WINDOW_MS: u64 =
    DIFFICULTY_ADJUSTMENT_INTERVAL as u64 * POW_TARGET_BLOCK_TIME_MS;

/// 全节点区块奖励,单位:分。
pub const FULLNODE_BLOCK_REWARD: u128 = 999_900; // 每个区块奖励：9999.00 元 = 999_900 分

// 全节点发行区块范围。
pub const FULLNODE_REWARD_START_BLOCK: u32 = 1; // 全节点奖励起始区块高度（含）
pub const FULLNODE_REWARD_END_BLOCK: u32 = 9_999_999; // 全节点奖励结束区块高度（含）

// 全节点发行总量。
pub const FULLNODE_REWARD_BLOCK_COUNT: u32 = // 全节点发行区块总数
    FULLNODE_REWARD_END_BLOCK - FULLNODE_REWARD_START_BLOCK + 1;
pub const FULLNODE_TOTAL_ISSUANCE: u128 = // 全节点发行总量（单位：分）， = 999_900 * 9_999_999
    FULLNODE_BLOCK_REWARD * FULLNODE_REWARD_BLOCK_COUNT as u128;

// PoW 区块与时间参数。
// 这是难度调整追踪的长期平均目标，不是最短间隔或最晚出块期限；有效 PoW 找到后立即出块。
pub const POW_TARGET_BLOCK_TIME_MS: u64 = 360_000; // 360,000 毫秒（6 分钟）

/// 制度期限按每块平均 6 分钟换算；真实区块仍可能提前或延后找到。
pub const SECONDS_PER_BLOCK: u64 = 360; // 目标平均出块间隔：6 分钟 = 360 秒
pub const BLOCKS_PER_HOUR: u64 = 3_600 / SECONDS_PER_BLOCK; // 每小时区块数：10
pub const BLOCKS_PER_DAY: u64 = BLOCKS_PER_HOUR * 24; // 每天区块数：240

/// 省储行年度利息结算周期,单位:区块。
pub const BLOCKS_PER_YEAR: u64 = 87_600;

#[cfg(test)]
mod tests {
    use super::*;

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
