//! 预算案子域(Phase 4 预留)。
//!
//! 政府提预算案 → 立法机关**单院常规案**表决(《预算法》授权,非宪法直授,亦非法律案)。
//! 结构 = 国标政府收支科目四级:**类 > 款 > 项 > 目**;金额单位**分**(u128 整数,禁浮点)。
//!
//! 当前没有预算业务 pallet 或 extrinsic，故本模块仅锁定链下字段 schema。未来预算业务应保存
//! 预算正文并调用 `legislation-vote` 的代表机构表决，不能新增预算投票 kind 或自行计票。
//! `类/款/项/目` code 编码规则仍为显式待定项，当前 code 以自由文本承载。

/// 预算案字段 schema(`BudgetClass>Section>Item>Subitem` + `BudgetPlan` + `ProposeBudgetInput`)。
pub(crate) mod model;
