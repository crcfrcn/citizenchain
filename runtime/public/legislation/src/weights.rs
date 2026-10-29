//! 立法院模块权重(ADR-027)。
//!
//! 当前使用固定保守权重。三个入口会解析岗位权限、冻结多机构岗位任职并创建
//! 立法投票提案；在业务 pallet 获得完整可执行 benchmark 夹具前不得恢复旧的低占位值。

use frame_support::weights::Weight;

/// 立法院三个提案入口的权重接口。
pub trait WeightInfo {
    fn propose_enact_law() -> Weight;
    fn propose_amend_law() -> Weight;
    fn propose_repeal_law() -> Weight;
}

/// 默认实现：为法律正文校验、岗位目录读取、VotePlan 和多岗位快照预留保守上界。
impl WeightInfo for () {
    fn propose_enact_law() -> Weight {
        Weight::from_parts(5_000_000_000, 1_500_000)
    }
    fn propose_amend_law() -> Weight {
        Weight::from_parts(5_000_000_000, 1_500_000)
    }
    fn propose_repeal_law() -> Weight {
        Weight::from_parts(3_000_000_000, 1_000_000)
    }
}
