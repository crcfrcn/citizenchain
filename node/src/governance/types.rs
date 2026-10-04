// 治理模块数据类型，对应链上治理机构和管理员 pallet 的存储结构。

use serde::Serialize;

/// 机构类型枚举，数值与链上 `org` 编码一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstitutionType {
    /// 国家储委会 National Reserve Committee
    Nrc = 0,
    /// 省储委会 Provincial Reserve Committee
    Prc = 1,
    /// 省储行 Provincial Reserve Bank
    Prb = 2,
}

impl InstitutionType {
    pub fn label(&self) -> &'static str {
        match self {
            InstitutionType::Nrc => "国家储委会",
            InstitutionType::Prc => "省储委会",
            InstitutionType::Prb => "省储行",
        }
    }
}

/// 机构管理员人员记录、全部有效岗位任职及可选链上余额。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminInfo {
    #[serde(rename = "account_id")]
    pub account_id: String,
    pub family_name: String,
    pub given_name: String,
    pub assignments: Vec<crate::admins::management::types::InstitutionRoleAssignmentInfo>,
    /// 链上余额（分），节点未运行或余额查询失败时为 null。
    pub balance_fen: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionDetail {
    /// 机构中文全称,字段唯一对齐 runtime/CID 的 `cid_full_name`。
    pub cid_full_name: String,
    /// 机构中文简称,字段唯一对齐 runtime/CID 的 `cid_short_name`。
    pub cid_short_name: String,
    /// 机构英文全称,字段唯一对齐 runtime/CID 的 `cid_full_name_en`。
    pub cid_full_name_en: String,
    /// 机构英文简称,字段唯一对齐 runtime/CID 的 `cid_short_name_en`。
    pub cid_short_name_en: String,
    /// 链上身份标识。
    pub cid_number: String,
    /// 机构类型：0=NRC, 1=PRC, 2=PRB。
    pub org_type: u8,
    /// 机构类型显示标签。
    pub org_type_label: String,
    /// 主账户 ID，固定为小写 `0x` + 64 位十六进制。
    #[serde(rename = "main_account_id")]
    pub main_account_id: String,
    /// 主账户链上余额（分），节点未运行时为 null。
    pub balance_fen: Option<String>,
    /// 管理员账户列表；每个账户携带其全部有效岗位任职。
    pub admins: Vec<AdminInfo>,
    /// 内部投票通过阈值。
    pub internal_threshold: u32,
    /// 联合投票权重。
    pub joint_vote_weight: u32,
    /// 永久质押账户 AccountId hex（仅 PRB）。
    #[serde(rename = "stake_account_id")]
    pub stake_account_id: Option<String>,
    /// 永久质押账户余额（分，仅 PRB）。
    pub staking_balance_fen: Option<String>,
    /// 手续费账户 AccountId hex（仅 PRB）。
    #[serde(rename = "fee_account_id")]
    pub fee_account_id: Option<String>,
    /// 手续费账户余额（分，仅 PRB）。
    pub fee_balance_fen: Option<String>,
    /// 储委会费用账户 AccountId hex（省储委会，仅 PRC）。
    #[serde(rename = "cb_fee_account_id")]
    pub cb_fee_account_id: Option<String>,
    /// 储委会费用账户余额（分，仅 PRC）。
    pub cb_fee_balance_fen: Option<String>,
    /// 国家储委会费用账户 AccountId hex（仅 NRC）。
    #[serde(rename = "nrc_fee_account_id")]
    pub nrc_fee_account_id: Option<String>,
    /// 国家储委会手续费账户余额（分，仅 NRC）。
    pub nrc_fee_balance_fen: Option<String>,
    /// 国家储委会安全基金账户 AccountId hex（仅 NRC）。
    #[serde(rename = "safety_fund_account_id")]
    pub safety_fund_account_id: Option<String>,
    /// 国家储委会安全基金账户余额（分，仅 NRC）。
    pub safety_fund_balance_fen: Option<String>,
    /// 告警信息。
    pub warning: Option<String>,
}

/// 治理详情页余额更新事件，仅覆盖链上金额和告警，不改页面结构。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionBalanceUpdate {
    /// 链上身份标识，用于前端过滤当前页面事件。
    pub cid_number: String,
    /// 主账户链上余额（分）。
    pub balance_fen: Option<String>,
    /// 永久质押账户链上余额（分，仅 PRB）。
    pub staking_balance_fen: Option<String>,
    /// 费用账户链上余额（分，仅 PRB）。
    pub fee_balance_fen: Option<String>,
    /// 省储委会费用账户链上余额（分，仅 PRC）。
    pub cb_fee_balance_fen: Option<String>,
    /// 国家储委会费用账户链上余额（分，仅 NRC）。
    pub nrc_fee_balance_fen: Option<String>,
    /// 国家储委会安全基金账户链上余额（分，仅 NRC）。
    pub safety_fund_balance_fen: Option<String>,
    /// 链上查询告警。
    pub warning: Option<String>,
}

/// 机构列表项，用于治理首页机构列表。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionListItem {
    /// 机构中文全称,字段唯一对齐 runtime/CID 的 `cid_full_name`。
    pub cid_full_name: String,
    /// 机构中文简称,字段唯一对齐 runtime/CID 的 `cid_short_name`。
    pub cid_short_name: String,
    /// 机构英文全称,字段唯一对齐 runtime/CID 的 `cid_full_name_en`。
    pub cid_full_name_en: String,
    /// 机构英文简称,字段唯一对齐 runtime/CID 的 `cid_short_name_en`。
    pub cid_short_name_en: String,
    pub cid_number: String,
    pub org_type: u8,
    pub org_type_label: String,
    /// 主账户 ID，固定为小写 `0x` + 64 位十六进制。
    #[serde(rename = "main_account_id")]
    pub main_account_id: String,
}

/// 治理首页聚合数据。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceOverview {
    /// 国家储委会列表。
    pub national_councils: Vec<InstitutionListItem>,
    /// 省储委会列表。
    pub provincial_councils: Vec<InstitutionListItem>,
    /// 省储行列表。
    pub provincial_banks: Vec<InstitutionListItem>,
    pub warning: Option<String>,
}
