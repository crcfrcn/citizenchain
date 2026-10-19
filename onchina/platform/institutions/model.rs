//! 本机构管理员展示 DTO；字段及序列化规则保持不变。
use serde::Serialize;
#[derive(Serialize)]
pub(crate) struct OwnInstitutionAdminRow {
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) role_code: String,
    pub(crate) role_name: String,
    pub(crate) term_required: bool,
    pub(crate) term_start: u32,
    pub(crate) term_end: u32,
    pub(crate) assignment_source: u8,
    pub(crate) assignment_source_label: String,
    pub(crate) assignment_source_ref: String,
    /// 链上 finalized free 余额(分);查询失败或账户不存在时为空。
    pub(crate) balance_fen: Option<String>,
    pub(crate) is_self: bool,
}

#[derive(Serialize)]
pub(crate) struct OwnInstitutionAdminListOutput {
    pub(crate) institution_code: String,
    pub(crate) cid_short_name: Option<String>,
    pub(crate) rows: Vec<OwnInstitutionAdminRow>,
}

