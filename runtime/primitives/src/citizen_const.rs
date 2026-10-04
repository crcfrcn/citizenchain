//! 公民认证发行常量。

use crate::genesis::GENESIS_CITIZEN_MAX;

// 发行总量与阶段。
pub const CITIZEN_ISSUANCE_MAX_COUNT: u64 = GENESIS_CITIZEN_MAX; // 可获得奖励的公民轻节点最大数量
pub const CITIZEN_ISSUANCE_HIGH_REWARD_COUNT: u64 = 14_436_417; // 高额奖励阶段节点数量

// 认证奖励金额,单位:分。
pub const CITIZEN_ISSUANCE_HIGH_REWARD: u128 = 999_900; // 前期高额认证奖励：9999.00 元/节点
pub const CITIZEN_ISSUANCE_NORMAL_REWARD: u128 = 99_900; // 后期常规认证奖励：999.00 元/节点

// 认证发行规则。
pub const CITIZEN_ISSUANCE_ONE_TIME_ONLY: bool = true; // 每个 CID 只能领取一次奖励
