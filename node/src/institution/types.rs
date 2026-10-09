// 清算行机构只读查询的 Tauri DTO,与前端 transaction/offchain/institution/types.ts 对齐。
//
//
// - 本文件承载清算行流程需要的机构身份只读类型:资格候选、账户余额、机构详情、提案分页、CID 注册凭证。
// - 机构创建(propose_create_institution)已迁出节点,归 onchina 控制台,故本文件不含任何创建输入类型。

use primitives::cid::code::InstitutionCode;
use serde::Serialize;

/// 节点桌面"添加清算行"页用的候选机构记录(序列化给 Tauri 前端)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EligibleClearingBankCandidate {
    pub cid_number: String,
    /// 机构中文名;两步式未命名时为空串。
    pub cid_full_name: String,
    pub ref_property: String,
    pub sub_type: Option<String>,
    pub parent_cid_number: Option<String>,
    pub parent_cid_full_name: Option<String>,
    pub parent_ref_property: Option<String>,
    pub province_name: String,
    pub city_name: String,
    #[serde(rename = "main_account_id")]
    pub main_account_id: Option<String>,
    #[serde(rename = "fee_account_id")]
    pub fee_account_id: Option<String>,
}

/// 单账户的链上展示形态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountWithBalance {
    pub account_name: String,
    /// 唯一账户 ID，固定为小写 `0x` + 64 位十六进制。
    #[serde(rename = "account_id")]
    pub account_id: String,
    /// 仅用于展示的 SS58 地址（GMB prefix=2027）。
    #[serde(rename = "ss58_address")]
    pub ss58_address: String,
    /// `frame_system::Account[address].data.free`,最小单位"分"。
    pub balance_min_units: String,
    /// 友好元字符串 `xxx.xx`。
    pub balance_text: String,
    /// 协议账户类别：main/fee/stake/safety_fund/he/named。
    pub account_kind: String,
    /// 只有 named 自定义账户允许关闭。
    pub can_close: bool,
}

/// 机构管理员账户及其全部有效岗位任职，与管理员管理模块共用同一 DTO。
pub type InstitutionAdminDisplay = crate::admins::management::types::InstitutionAdminInfo;

/// 机构详情 = `PublicManage/PrivateManage::Institutions[cid_number]`(机构最小集)
/// + 派生的主/费账户余额 + 管理员模块管理员集合 + entity 机构治理阈值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionDetail {
    pub cid_number: String,
    pub cid_full_name: String,
    /// 机构码（CID institution_code，[u8;4]）。清算行属于私权法人机构码。
    pub institution_code: InstitutionCode,
    #[serde(rename = "main_account_info")]
    pub main_account_info: AccountWithBalance,
    #[serde(rename = "fee_account_info")]
    pub fee_account_info: AccountWithBalance,
    /// 主账户/费用账户之外的全部账户(自定义初始账户)。
    pub other_accounts: Vec<AccountWithBalance>,
    pub admins_len: u32,
    pub threshold: u32,
    /// 管理员账户及其有效岗位任职。
    pub admins: Vec<InstitutionAdminDisplay>,
    pub created_at: u64,
    pub account_count: u32,
}

/// 机构提案列表分页结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionProposalPage {
    pub items: Vec<InstitutionProposalItem>,
    pub has_more: bool,
}

/// 提案列表条目。提案完整字段由 governance 模块掌握,这里只透传必需展示项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionProposalItem {
    pub proposal_id: u64,
    pub kind_label: String,
    pub status_label: String,
    pub summary: String,
}
