//! 定义立法大屏的席位、活跃提案和看板响应模型。

use serde::Serialize;

use crate::chain_read_proposal::LegProposalState;

/// 单个席位(议员)对当前提案的投票态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeatView {
    /// 完整票据身份，仅供前端稳定 key 使用。
    pub(crate) ticket_id: String,
    pub(crate) role_code: String,
    /// 人员当前有效绑定账户；没有绑定时保留冻结人员CID和票据。
    pub(crate) account_id: Option<String>,
    pub(crate) person_cid: String,
    /// 姓名只取同块identity可用资料；缺失时留空，不推造历史姓名。
    pub(crate) name: String,
    /// 机构岗位名称(链上 entity 任职关系)。
    #[serde(rename = "title")]
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
    /// 本机构议员岗位的有效人员任职数。
    pub(crate) roster_total: u32,
    /// 活跃立法提案看板(仅 kind=立法;无活跃提案时为空)。
    pub(crate) active_proposals: Vec<ActiveProposalView>,
}
