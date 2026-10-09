//! 大屏看板 DTO(camelCase 出线,前端 `legislation/display/types.ts` 逐字镜像)。
//!
//! 嵌入既有 `LegProposalState`(不重定义),叠加席位与聚合计数。

use serde::Serialize;

use crate::domains::legislation::chain_read_proposal::LegProposalState;

/// 单个席位(议员)对当前提案的投票态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeatView {
    /// 议员账户(0x 小写 hex)。
    #[serde(rename = "account_id")]
    pub(crate) account_id: String,
    /// 管理员姓名不在链上保存，保留空值仅供既有席位 DTO 布局使用。
    pub(crate) name: String,
    /// 机构岗位名称(链上 entity 任职关系)。
    pub(crate) role_name: String,
    /// 该席位对本提案的院内投票:`Some(true)`=赞成 / `Some(false)`=反对 / `None`=未投。
    pub(crate) vote: Option<bool>,
}

/// 单个活跃提案的看板视图(进度投影 + 席位板 + 聚合计数)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveProposalView {
    /// 提案进度只读投影(阶段/状态/计票,复用 `LegProposalState`)。
    pub(crate) state: LegProposalState,
    /// 本机构议员对该提案的逐席投票。
    pub(crate) seats: Vec<SeatView>,
    /// 赞成席位数。
    pub(crate) approved_count: u32,
    /// 反对席位数。
    pub(crate) rejected_count: u32,
    /// 未投席位数。
    pub(crate) pending_count: u32,
}

/// 大屏看板顶层快照(本节点机构 + 名册规模 + 活跃提案列表)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisplayBoard {
    /// 本节点绑定机构码(如 `NRP`)。
    pub(crate) institution_code: String,
    /// 机构简称(单源 `cid_short_name`;未加载为 `None`)。
    pub(crate) cid_short_name: Option<String>,
    /// 辖区文案(省·市;国家级为「全国」)。
    pub(crate) scope_label: String,
    /// 议员名册规模(链上 Active 管理员总数)。
    pub(crate) roster_total: u32,
    /// 活跃立法提案看板(仅 kind=立法;无活跃提案时为空)。
    pub(crate) active_proposals: Vec<ActiveProposalView>,
}
