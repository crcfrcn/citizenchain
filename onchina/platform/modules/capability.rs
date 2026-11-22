//! 从机构类型和实际业务声明汇总登录态可见能力。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySet {
    pub can_view_citizens: bool,
    pub can_view_institutions: bool,
    pub can_view_private: bool,
    pub can_view_education: bool,
    #[serde(rename = "canViewFederalRegistryAdmins")]
    pub view_federal_admins: bool,
    #[serde(rename = "canViewCityRegistryAdmins")]
    pub view_city_admins: bool,
    #[serde(rename = "canCrudCityRegistryAdmins")]
    pub manage_city_admins: bool,
    /// 只读「本机构管理员」位:非注册局法人登录后可查看本机构链上管理员列表(只读)。
    #[serde(rename = "canViewOwnAdmins")]
    pub view_own_admins: bool,
    pub can_manage_institutions: bool,
    pub can_register_institutions: bool,
    pub can_business_write: bool,
    #[serde(rename = "canViewCityRegistry")]
    pub view_city_registry: bool,
    #[serde(rename = "canViewFederalRegistry")]
    pub view_federal_registry: bool,
    /// 立法:查看立法/提案/大屏(立法机构通用只读位)。
    pub can_view_legislation: bool,
    /// 立法:发起法律案(发起院 / 教委会 / 自治会;参议会无此位)。
    pub can_propose_legislation: bool,
    /// 立法：当前代表机构表决（发起院 / 参议会 / 国家教委会）。
    #[serde(rename = "canCastRepresentativeVote")]
    pub cast_representative_vote: bool,
    /// 立法:行政签署 / 三人会签 / 护宪终审(行政签署人 / 大法官;另线程接入时置位,本轮恒 false)。
    pub can_sign_legislation: bool,
    /// 立法:发起任免案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub can_propose_personnel: bool,
    /// 立法:发起预算案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub can_propose_budget: bool,
}

pub const EMPTY: CapabilitySet = CapabilitySet {
    can_view_citizens: false,
    can_view_institutions: false,
    can_view_private: false,
    can_view_education: false,
    view_federal_admins: false,
    view_city_admins: false,
    manage_city_admins: false,
    view_own_admins: false,
    can_manage_institutions: false,
    can_register_institutions: false,
    can_business_write: false,
    view_city_registry: false,
    view_federal_registry: false,
    can_view_legislation: false,
    can_propose_legislation: false,
    cast_representative_vote: false,
    can_sign_legislation: false,
    can_propose_personnel: false,
    can_propose_budget: false,
};

// 普通机构(NJD/公权/私权/非法人,非注册局):本期只开只读「本机构管理员」入口。
const OWN_ADMINS_READONLY: CapabilitySet = CapabilitySet {
    view_own_admins: true,
    ..EMPTY
};

// 各业务模块在其真实入口声明业务能力；本文件只保留公共类型、基线与汇总。

/// 机构码文本 → 能力集。按机构类(`primitives::cid::code` 单源)分发,未知/不归控制台返回空能力。
pub fn base_capabilities(institution_code: &str) -> CapabilitySet {
    use primitives::cid::code::{
        institution_code_from_str, is_private_legal_code, is_public_legal_code,
        is_unincorporated_code,
    };
    let Some(code) = institution_code_from_str(institution_code) else {
        return EMPTY;
    };

    if primitives::cid::code::is_fixed_governance_code(&code) || is_public_legal_code(&code) || is_private_legal_code(&code) || is_unincorporated_code(&code)
    {
        OWN_ADMINS_READONLY
    } else {
        EMPTY
    }
}
/// 业务能力由已装载模块提供，通用本机构读取仍按原规则。
pub fn capabilities_for(
    catalog: &crate::modules::ModuleCatalog,
    institution_code: &str,
) -> CapabilitySet {
    catalog
        .capabilities(institution_code)
        .unwrap_or_else(|| base_capabilities(institution_code))
}

#[cfg(test)]
#[path = "../tests/modules_capability.rs"]
mod tests;
