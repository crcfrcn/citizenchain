//! 投票治理常量。
//! 内部投票产出机构结果,联合投票只统计机构结果。

use crate::pow_const;

// 机构基础数量。
pub const NRC_ADMIN_COUNT: u32 = 19; // 国家储委会管理员数量
pub const PRC_ADMIN_COUNT: u32 = 9; // 单个省储委会管理员数量
pub const PRB_ADMIN_COUNT: u32 = 9; // 单个省储行管理员数量
pub const FRG_PROVINCE_GROUP_ADMIN_COUNT: u32 = 5; // 单个联邦注册局省行政区组管理员数量
pub const NJD_ADMIN_COUNT: u32 = 15; // 国家司法院创世公职人员数量
pub const PRC_COUNT: u32 = (crate::cid::china::china_cb::CHINA_CB.len() - 1) as u32; // 初始省储委会数量（总储会-国家储委会）
pub const PRB_COUNT: u32 = crate::cid::china::china_ch::CHINA_CH.len() as u32; // 初始省储行数量（来自省储行数组）

// 内部投票阈值。
pub const NRC_INTERNAL_THRESHOLD: u32 = 13; // 国家储委会内部投票通过阈值
pub const PRC_INTERNAL_THRESHOLD: u32 = 6; // 省储委会内部投票通过阈值
pub const PRB_INTERNAL_THRESHOLD: u32 = 6; // 省储行内部投票通过阈值
pub const FRG_INTERNAL_THRESHOLD: u32 = 3; // 联邦注册局省行政区组内部投票通过阈值
pub const NJD_INTERNAL_THRESHOLD: u32 = 8; // 国家司法院内部投票通过阈值

// 联合投票权重与阈值。
pub const NRC_JOINT_VOTE_WEIGHT: u32 = 19; // 国家储委会在联合投票中的票数（仅当国家储委会内部投票通过，通过=19票，未通过=0票）
pub const PRC_JOINT_VOTE_WEIGHT: u32 = 1; // 单个省储委会在联合投票中的票数（仅当省储委会内部投票通过）
pub const PRB_JOINT_VOTE_WEIGHT: u32 = 1; // 单个省储行在联合投票中的票数（仅当省储行内部投票通过）
pub const JOINT_VOTE_TOTAL: u32 = NRC_JOINT_VOTE_WEIGHT
    + (PRC_COUNT * PRC_JOINT_VOTE_WEIGHT)
    + (PRB_COUNT * PRB_JOINT_VOTE_WEIGHT); // 联合投票总票数
pub const JOINT_VOTE_PASS_THRESHOLD: u32 = 105; // 联合投票通过条件：全票通过则立即执行，非全票通过则进入公民投票流程

// 投票时限,单位:区块。
pub const VOTING_DURATION_DAYS: u32 = 30; // 投票默认期限30天
pub const BLOCKS_PER_DAY: u32 = pow_const::BLOCKS_PER_DAY as u32; // 每天区块数（统一来源：pow_const）
pub const VOTING_DURATION_BLOCKS: u32 = BLOCKS_PER_DAY * VOTING_DURATION_DAYS; // 投票默认期限（区块）= 30 * BLOCKS_PER_DAY

// 决议发行常量。
pub const RESOLUTION_ISSUANCE_MAX_REASON_LEN: u32 = 1024; // 决议发行理由最大长度
pub const RESOLUTION_ISSUANCE_MAX_ALLOCATIONS: u32 = PRC_COUNT; // 决议发行单次最大分配条目数（与省储委会数量一致）

/// 公民宪法不可修改条款清单。
pub const IMMUTABLE_CONSTITUTION_ARTICLES: [u32; 8] = [1, 2, 3, 17, 19, 24, 34, 42];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_vote_total_matches_threshold() {
        // 联合投票总票数必须等于通过阈值（全票通过制）。
        assert_eq!(JOINT_VOTE_TOTAL, JOINT_VOTE_PASS_THRESHOLD);
    }
}
