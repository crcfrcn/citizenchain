use nodeupgrade::types::RuntimeUpgradeDetail;

use noderesolutionissue::types::ResolutionIssuanceDetail;

use nodedestroy::types::ResolutionDestroyDetail;

use primitives::cid::code::InstitutionCode;

use serde::Serialize;

/// 把机构码 [u8;4] 序列化为 4 字符展示串(末尾 `\0` 填充去掉),便于前端消费。
fn serialize_internal_code<S>(
    code: &Option<InstitutionCode>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match code {
        Some(code) => {
            let end = code.iter().position(|&b| b == 0).unwrap_or(code.len());
            serializer.serialize_some(&String::from_utf8_lossy(&code[..end]))
        }
        None => serializer.serialize_none(),
    }
}

/// 提案展示号（双层 ID）：`ProposalDisplayId[id]` 反查值。
///
/// 主键 `proposal_id` 是全局单调 u64;展示号通过本结构持有
/// `(year, seq_in_year)`,渲染层把它拼成 "2026000123" 风格。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDisplayMeta {
    pub year: u16,
    pub seq_in_year: u32,
}

/// 提案元数据（从 VotingEngine::Proposals 解码）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalMeta {
    pub proposal_id: u64,
    /// 0=内部投票, 1=联合投票。
    pub kind: u8,
    /// 0=内部投票, 1=联合投票内部投票阶段, 2=联合公投阶段。
    pub stage: u8,
    /// 0=投票中, 1=通过, 2=否决, 3=已执行, 4=执行失败。
    pub status: u8,
    /// 仅内部投票:机构码(CID institution_code),序列化为 4 字符展示串。
    #[serde(serialize_with = "serialize_internal_code")]
    pub internal_code: Option<InstitutionCode>,
    /// 发起机构唯一 CID；个人多签、公民个人或系统提案为空。
    pub actor_cid_number: Option<String>,
    /// 仅在具体资产账户或个人多签参与执行时存在。
    pub execution_account_id: Option<String>,
    /// 机构类提案关联的机构 CID 列表；个人多签提案为空。
    pub subject_cid_numbers: Vec<String>,
}

/// 投票计数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteTally {
    pub yes: u32,
    pub no: u32,
}

/// 联合投票计数（权重制）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JointVoteTally {
    pub yes: u32,
    pub no: u32,
}

/// 联合公投计数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferendumVoteTally {
    pub yes: u64,
    pub no: u64,
}

/// 提案完整信息（元数据 + 业务详情 + 投票进度）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalFullInfo {
    pub meta: ProposalMeta,
    #[serde(flatten)]
    pub business_details: nodemultisig::types::ProposalDetails,
    pub runtime_upgrade_detail: Option<RuntimeUpgradeDetail>,
    pub fee_rate_detail: Option<FeeRateProposalDetail>,
    pub resolution_issuance_detail: Option<ResolutionIssuanceDetail>,
    pub resolution_destroy_detail: Option<ResolutionDestroyDetail>,
    pub internal_tally: Option<VoteTally>,
    pub joint_tally: Option<JointVoteTally>,
    pub referendum_tally: Option<ReferendumVoteTally>,
    /// 提案创建时冻结的机构投票岗位主体；个人多签主体不在此列表中。
    pub voter_role_subjects: Vec<VoterRoleSubject>,
    /// 关联机构名称（通过 `subject_cid_numbers` 反查）。
    pub cid_full_name: Option<String>,
}

/// 前端用于过滤可投岗位的最小岗位主体镜像。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoterRoleSubject {
    pub cid_number: String,
    pub role_code: String,
}

/// 费率提案详情。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRateProposalDetail {
    pub proposal_id: u64,
    /// 发起机构唯一 CID。
    pub actor_cid_number: String,
    /// 承担该费率业务的机构账户。
    pub institution_account_id: String,
    pub new_rate_bp: u32,
}

/// 提案列表项（轻量，用于列表展示）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalListItem {
    pub proposal_id: u64,
    pub display_id: String,
    pub kind: u8,
    pub kind_label: String,
    pub stage: u8,
    pub stage_label: String,
    pub status: u8,
    pub status_label: String,
    pub cid_full_name: Option<String>,
    /// 简要描述（转账提案：金额+备注，升级提案：reason 前 50 字）。
    pub summary: String,
}

/// 提案分页结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPageResult {
    pub items: Vec<ProposalListItem>,
    /// 是否还有更多。
    pub has_more: bool,
    pub warning: Option<String>,
}

/// 提案业务动作（统一解码结果）。
///
/// 列表 summary 与详情 `ProposalFullInfo` 的各个 `*_detail` 字段均由它派生,
/// 保证两条路径对同一提案看到一致内容,避免再次出现"列表说无详情、详情页却能显示"的漂移。
///
/// 每种变体用 `Box` 包裹是为了控制 enum 大小(`ResolutionIssuance` 含 Vec 较重)。
pub(crate) enum ProposalAction {
    Business(Box<crate::business::ProposalAction>),
    RuntimeUpgrade(Box<RuntimeUpgradeDetail>),
    ResolutionIssuance(Box<ResolutionIssuanceDetail>),
    ResolutionDestroy(Box<ResolutionDestroyDetail>),
    /// 费率提案详情展示结构已保留，链上查询接入前该动作分支暂不构造。
    #[allow(dead_code)]
    FeeRate(Box<FeeRateProposalDetail>),
    /// 所有可能数据源都查过仍无命中,展示层回退为"无详情数据"。
    Unknown,
}

// ──── 投票状态查询 ────

/// 用户投票状态。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserVoteStatus {
    /// 提案 ID。
    pub proposal_id: u64,
    /// 提案 kind（0=内部, 1=联合）。
    pub kind: u8,
    /// 提案当前阶段。
    pub stage: u8,
    /// 机构岗位票据使用的岗位码；个人多签为 null。
    pub voter_role_code: Option<String>,
    /// 该用户是否已在内部阶段投票：null=未投票, true=赞成, false=反对。
    pub internal_vote: Option<bool>,
    /// 该用户是否已在联合阶段投票（通过机构）：null=未投票, true=赞成, false=反对。
    pub joint_vote: Option<bool>,
}
