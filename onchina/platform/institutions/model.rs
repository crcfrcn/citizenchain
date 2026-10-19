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


// 私权机构类别字符串真源，既有值不变；此处不解析CID或地区。
pub(crate) const PRIVATE_TYPE_SOLE: &str = "SOLE";
pub(crate) const PRIVATE_TYPE_PARTNERSHIP: &str = "PARTNERSHIP";
pub(crate) const PRIVATE_TYPE_COMPANY: &str = "COMPANY";
pub(crate) const PRIVATE_TYPE_CORPORATION: &str = "CORPORATION";
pub(crate) const PRIVATE_TYPE_WELFARE: &str = "WELFARE";
pub(crate) const PRIVATE_TYPE_ASSOCIATION: &str = "ASSOCIATION";

/// 由私权机构码反推 private_type 展示码(如 `SFGY` → `WELFARE`)。
///
/// 链上 Institutions 只存机构码,不存 private_type —— 后者是按机构码确定性派生的业务分类。
/// 联邦 drill-in 把纯链上私权机构投影进本地库时,本地无既有行可继承 private_type,必须据机构码
/// 回填,否则私权列表 SQL 的 `private_type IS NOT NULL` 会把纯投影的私权机构(如创世公民链
/// 技术发展基金会 SFGY)挡在列表外。非私权机构码返回 `None`(不写 private_type)。
pub(crate) fn private_type_code_from_institution_code(
    institution_code: &str,
) -> Option<&'static str> {
    match institution_code {
        "SFGT" => Some(PRIVATE_TYPE_SOLE),
        "SFGP" | "SFLP" => Some(PRIVATE_TYPE_PARTNERSHIP),
        "SFGQ" => Some(PRIVATE_TYPE_COMPANY),
        "SFGF" => Some(PRIVATE_TYPE_CORPORATION),
        "SFGY" => Some(PRIVATE_TYPE_WELFARE),
        "SFAS" => Some(PRIVATE_TYPE_ASSOCIATION),
        _ => None,
    }
}
#[cfg(test)]
mod private_type_tests {
    use super::*;
    #[test]
    fn existing_private_classes_keep_their_original_codes() {
        for (institution, expected) in [("SFGT","SOLE"),("SFGP","PARTNERSHIP"),("SFLP","PARTNERSHIP"),("SFGQ","COMPANY"),("SFGF","CORPORATION"),("SFGY","WELFARE"),("SFAS","ASSOCIATION")] {
            assert_eq!(private_type_code_from_institution_code(institution), Some(expected));
        }
        assert_eq!(private_type_code_from_institution_code("NLG"), None);
    }
}
