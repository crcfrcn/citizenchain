//! 机构能力公共类型和装载汇总；业务能力由所属模块真实入口声明，经会话下发前端。
//!
//! 决定「什么机构登录后进入什么工作台、能看到什么入口」。这是权限的【声明式单源】,与后端实际执行
//! 边界(`is_tier1_registry` 谓词 / `scope` / 链上 Active 集合)同源对齐;前端只据此 render-gating,
//! 不构成安全边界(后端始终对越权独立拒绝)。
//!
//! 机构类分发(`primitives::cid::code` 单源):
//! - Tier1 创世注册局(FRG):拥有 Tier2 全部业务能力,并额外管理联邦/市注册局管理员。
//! - Tier2 下级注册局(CREG,公权子集):录入公民/机构 + 看本市注册局 + 只读本省联邦注册局。
//! - 国家司法院(NJD):可登录司法院工作台,本期只开只读「本机构管理员」位。
//! - 国家储委会/省储委会/省储行(NRC/PRC/PRB):使用节点桌面端,不归本控制台 → 空能力。
//! - 其余公权/私权/非法人机构:本期只开只读「本机构管理员」位(`can_view_own_admins`);
//!   CRUD / 录入等具体功能待各机构功能落地时再开(机制就绪、不越权)。
//!
//! serde 字段名用 camelCase,与前端 `onchina/platform/frontend/platform_capabilityMap.ts` 的 CapabilitySet 逐字段对齐。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitySet {
    pub(crate) can_view_citizens: bool,
    pub(crate) can_view_institutions: bool,
    pub(crate) can_view_private: bool,
    pub(crate) can_view_education: bool,
    #[serde(rename = "canViewFederalRegistryAdmins")]
    pub(crate) view_federal_admins: bool,
    #[serde(rename = "canViewCityRegistryAdmins")]
    pub(crate) view_city_admins: bool,
    #[serde(rename = "canCrudCityRegistryAdmins")]
    pub(crate) manage_city_admins: bool,
    /// 只读「本机构管理员」位:非注册局法人登录后可查看本机构链上管理员列表(只读)。
    #[serde(rename = "canViewOwnAdmins")]
    pub(crate) view_own_admins: bool,
    pub(crate) can_manage_institutions: bool,
    pub(crate) can_register_institutions: bool,
    pub(crate) can_business_write: bool,
    #[serde(rename = "canViewCityRegistry")]
    pub(crate) view_city_registry: bool,
    #[serde(rename = "canViewFederalRegistry")]
    pub(crate) view_federal_registry: bool,
    /// 立法:查看立法/提案/大屏(立法机构通用只读位)。
    pub(crate) can_view_legislation: bool,
    /// 立法:发起法律案(发起院 / 教委会 / 自治会;参议会无此位)。
    pub(crate) can_propose_legislation: bool,
    /// 立法：当前代表机构表决（发起院 / 参议会 / 国家教委会）。
    #[serde(rename = "canCastRepresentativeVote")]
    pub(crate) cast_representative_vote: bool,
    /// 立法:行政签署 / 三人会签 / 护宪终审(行政签署人 / 大法官;另线程接入时置位,本轮恒 false)。
    pub(crate) can_sign_legislation: bool,
    /// 立法:发起任免案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub(crate) can_propose_personnel: bool,
    /// 立法:发起预算案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub(crate) can_propose_budget: bool,
}

pub(crate) const EMPTY: CapabilitySet = CapabilitySet {
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
pub(crate) fn base_capabilities(institution_code: &str) -> CapabilitySet {
    use primitives::cid::code::{
        institution_code_from_str, is_private_legal_code, is_public_legal_code,
        is_unincorporated_code, NRC, PRB, PRC,
    };
    let Some(code) = institution_code_from_str(institution_code) else {
        return EMPTY;
    };
    if matches!(code, NRC | PRC | PRB) {
        return EMPTY;
    }
    if is_public_legal_code(&code) || is_private_legal_code(&code) || is_unincorporated_code(&code)
    {
        OWN_ADMINS_READONLY
    } else {
        EMPTY
    }
}
/// 业务能力由已装载模块提供，通用本机构读取仍按原规则。
pub(crate) fn capabilities_for(
    catalog: &crate::modules::ModuleCatalog,
    institution_code: &str,
) -> CapabilitySet {
    catalog
        .capabilities(institution_code)
        .unwrap_or_else(|| base_capabilities(institution_code))
}

#[cfg(test)]
mod tests {
    fn capabilities_for(code: &str) -> super::CapabilitySet {
        super::capabilities_for(&crate::modules::test_support::fixture_catalog(), code)
    }

    #[test]
    fn tier1_registry_keeps_subordinate_registry_business_superset() {
        let federal = capabilities_for("FRG");
        let city = capabilities_for("CREG");

        // FRG 是 CREG 的省级上游,注册局工作台业务能力必须覆盖 CREG。
        assert!(federal.can_view_citizens && city.can_view_citizens);
        assert!(federal.can_view_institutions && city.can_view_institutions);
        assert!(federal.can_view_private && city.can_view_private);
        assert!(federal.can_view_education && city.can_view_education);
        assert!(federal.can_manage_institutions && city.can_manage_institutions);
        assert!(federal.can_register_institutions && city.can_register_institutions);
        assert!(federal.can_business_write && city.can_business_write);
        assert!(federal.view_city_registry && city.view_city_registry);
    }

    #[test]
    fn subordinate_registry_can_read_federal_registry_without_registry_crud_flags() {
        let federal = capabilities_for("FRG");
        let city = capabilities_for("CREG");

        // CREG 必须能进入联邦注册局入口只读本省管理员;注册局维护类写权只留给 FRG。
        assert!(city.view_federal_registry);
        assert!(city.view_federal_admins);
        assert!(!city.manage_city_admins);
        assert!(federal.view_federal_registry);
        assert!(federal.view_federal_admins);
        assert!(federal.manage_city_admins);
    }

    #[test]
    fn national_judicial_yuan_can_view_own_admins_only() {
        let judicial = capabilities_for("NJD");

        // NJD 可进入司法院工作台,但本期只给本机构管理员只读页,不获得注册局业务能力。
        assert!(judicial.view_own_admins);
        assert!(!judicial.can_view_citizens);
        assert!(!judicial.can_view_institutions);
        assert!(!judicial.view_city_registry);
        assert!(!judicial.view_federal_registry);
    }

    #[test]
    fn legislative_institutions_get_role_based_legislation_capabilities() {
        // 发起机构（国家众议会）：发起 + 代表机构表决；同时保留本机构管理员只读位。
        let house = capabilities_for("NRP");
        assert!(house.can_view_legislation);
        assert!(house.can_propose_legislation);
        assert!(house.cast_representative_vote);
        assert!(house.view_own_admins);

        // 参议会:只表决,无发起权(权力分离硬约束)。
        let senate = capabilities_for("NSN");
        assert!(senate.cast_representative_vote);
        assert!(!senate.can_propose_legislation);

        // 市教委会：只提案，不参加代表机构表决（由市立法会表决）。
        let city_education = capabilities_for("CEDU");
        assert!(city_education.can_propose_legislation);
        assert!(!city_education.cast_representative_vote);

        // 本轮任免/预算/签署位均未接入。
        assert!(!house.can_propose_personnel);
        assert!(!house.can_propose_budget);
        assert!(!house.can_sign_legislation);
    }

    #[test]
    fn ownerless_cases_exercise_real_base_capabilities() {
        let empty = crate::modules::ModuleCatalog::new(None);
        let fixture = crate::modules::test_support::fixture_catalog();
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("layout.fixture.json")).unwrap();
        for case in golden["capabilityCases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["owner"].is_null())
        {
            let code = case["code"].as_str().unwrap();
            // 无所属模块的夹具不得遮住真实base_capabilities分支。
            assert!(
                fixture.capabilities(code).is_none(),
                "{code} must not be injected by fixture"
            );
            assert!(fixture.workspace(code).is_none());
            let expected = serde_json::to_value(super::EMPTY).unwrap();
            assert_eq!(
                serde_json::to_value(super::capabilities_for(&empty, code)).unwrap(),
                expected,
                "{code}"
            );
            assert_eq!(
                serde_json::to_value(super::capabilities_for(&fixture, code)).unwrap(),
                expected,
                "{code}"
            );
        }
        let ordinary = super::capabilities_for(&empty, "SFGQ");
        assert!(ordinary.view_own_admins);
        assert!(!ordinary.can_business_write && !ordinary.can_view_citizens);
    }
}
