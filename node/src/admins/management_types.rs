use primitives::cid::code::{is_valid_governance_code, InstitutionCode};
use serde::Serialize;

/// 一个岗位绑定的一条业务动作权限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRolePermissionInfo {
    pub module_tag: String,
    pub action_code: u32,
    pub operation: u8,
    pub operation_label: String,
}

/// 管理员账户在机构岗位上的一条有效任职。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRoleAssignmentInfo {
    pub role_code: String,
    pub role_name: String,
    pub term_required: bool,
    pub term_start: u32,
    pub term_end: u32,
    pub assignment_source: u8,
    pub assignment_source_label: String,
    pub assignment_source_ref: String,
    /// 权限属于岗位而非管理员；为便于桌面端核对，随每条有效岗位任职展示。
    pub permissions: Vec<InstitutionRolePermissionInfo>,
}

/// 一个机构管理员人员记录及其在本机构的全部有效岗位任职。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAdminInfo {
    /// 管理员账户 ID，固定为小写 `0x` + 64 位十六进制。
    #[serde(rename = "account_id")]
    pub account_id: String,
    /// 公权管理员公民 CID；私权管理员及尚未补齐的公权记录为空。
    pub cid_number: String,
    /// 管理员姓；公权记录允许暂时为空。
    pub family_name: String,
    /// 管理员名；公权记录允许暂时为空。
    pub given_name: String,
    pub assignments: Vec<InstitutionRoleAssignmentInfo>,
}

/// 公权或私权机构 `AdminAccounts[cid_number]` 的桌面端联合展示状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAdminsState {
    /// 机构唯一主键。
    pub cid_number: String,
    /// 链上机构码（CID institution_code，[u8;4]，治理分类唯一真源）。
    pub institution_code: InstitutionCode,
    pub institution_code_label: String,
    /// Node 按实际命中的公权/私权管理员 pallet 派生的类型编码。
    pub kind: u8,
    pub kind_label: String,
    /// 当前管理员账户及其有效岗位任职；账户在本集合内唯一。
    pub admins: Vec<InstitutionAdminInfo>,
}

/// 解码后的链上机构管理员集合原始值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstitutionAdminsDecoded {
    pub institution_code: InstitutionCode,
    pub admins: Vec<AdminDecoded>,
}

/// 从共享 SCALE 类型严格解码出的管理员统一展示记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminDecoded {
    pub account_id: String,
    pub cid_number: String,
    pub family_name: String,
    pub given_name: String,
}

/// 机构码 4 字符展示串（末尾 `\0` 填充字节去掉）。
pub fn institution_code_label(code: &InstitutionCode) -> String {
    let end = code.iter().position(|&b| b == 0).unwrap_or(code.len());
    String::from_utf8_lossy(&code[..end]).into_owned()
}

pub fn kind_label(kind: u8) -> &'static str {
    match kind {
        0 => "公权机构",
        1 => "私权机构",
        _ => "未知账户",
    }
}

pub fn is_valid_institution_code(code: &InstitutionCode) -> bool {
    is_valid_governance_code(code)
}
