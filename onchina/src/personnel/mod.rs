//! 任免案子域(Phase 4 预留)。
//!
//! 政府提「人事任免职书」→ 参议会(NSN)/省参议会(PSN)/市立法会(CLEG)**单院常规案**
//! 表决任免(宪法第100/106条;副总统/部长/省长/市长任免走第53/55/57/64条)。**非法律案**,
//! 无公投/签署/护宪。
//!
//! 当前没有任免业务 pallet 或 extrinsic，故本模块仅锁定链下字段 schema。未来任免业务应保存
//! 任免职书并调用 `legislation-vote` 的代表机构表决，不能新增任免投票 kind 或自行计票。
//! 升级路径（3 次驳回→重要案→直授）字段化随具体业务细则确定，本轮不引入。

/// 任免职书字段 schema(`PersonnelAction` / `PersonnelDecision` / `ProposePersonnelInput`)。
pub(crate) mod model;
