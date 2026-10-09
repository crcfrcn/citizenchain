//! 立法大屏只读看板(Phase 3)。
//!
//! 大厅正中大屏免登录只读展示——本节点绑定机构的**议员名册**(链上 Active 管理员)×
//! **活跃立法提案**(`VotingEngine::ActiveProposalsBySubject`)× **逐席投票**
//! (`LegislationVote::RepresentativeVotesByTicket`)。机构由节点绑定(`active_node_binding`)确定,
//! **不接受任何请求参数**——大屏只映射本节点自己,越权面为零(fail-closed)。
//!
//! 只读投影:计票/阶段/状态判定全归投票引擎,本域只搬运链上事实(复用 `chain_read_proposal`)。

/// 大屏专属链读:活跃提案 ID 列表 + 逐席院内投票映射。
pub(crate) mod chain_read;
/// 免登录只读 HTTP 入口(`GET /api/public/legislation/display/board`)。
pub(crate) mod handler;
/// 大屏看板 DTO(机构头 + 名册 + 每个活跃提案的进度与席位)。
pub(crate) mod model;
/// 名册 × 活跃提案 × 逐席投票的聚合装配。
pub(crate) mod service;
