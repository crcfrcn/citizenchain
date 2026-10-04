//! 立法与表决域(卡 20260630-onchina-legislation-console-framework,Phase 0 地基)。
//!
//! 承载立法机构管理员在 OnChina 的「发起提案 → 院内/两院表决 → 查看进度」与大屏只读展示。
//! 提案以「提案类型(ProposalCategory)」为可扩展维度:法律案(本轮)/任免案/预算案(预留)。
//! 本域只做「组织提案数据 + 扫码冷签 + 提交 extrinsic + 读链展示」,绝不计票/推进状态机(全归投票引擎)。
//!
//! Phase 0 仅落地数据地基:提案类型枚举(`model`)+ 提案候选与立法角色解析(`category`)。
//! 链交互(`law/chain_*`)、HTTP 入口(`handler`/`service`)、大屏聚合(`display`)在后续 Phase 落地。

/// 预算案（类>款>项>目）字段 schema；未来由预算业务模块接入代表机构表决。
pub(crate) mod budget;
pub(crate) mod category;
/// 提案进度只读投影（Proposal + 代表/法律元数据 + tally）。
pub(crate) mod chain_read_proposal;
/// 大屏只读看板(Phase 3)——本节点机构名册 × 活跃提案 × 逐席投票聚合,免登录只读。
pub(crate) mod display;
/// 立法与表决 HTTP handler(/api/legislation/* 发起/表决/读法律/读提案)。
pub(crate) mod handler;
/// 法律案——章节条款提案 + 院内/两院表决 + 签署的链交互编码器(Phase 1)。
pub(crate) mod law;
pub(crate) mod model;
/// 任免案（人事任免职书）字段 schema；未来由任免业务模块接入代表机构表决。
pub(crate) mod personnel;
