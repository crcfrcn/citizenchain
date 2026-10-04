//! 机构码 → 控制台能力位的唯一权威源(后端单源,经会话下发给前端镜像)。
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
//! serde 字段名用 camelCase,与前端 `platform/capabilityMap.ts` 的 CapabilitySet 逐字段对齐。

use serde::Serialize;

use crate::domains::legislation::category::{legislation_role, LegislationRole};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitySet {
    pub(crate) can_view_citizens: bool,
    pub(crate) can_view_institutions: bool,
    pub(crate) can_view_private: bool,
    pub(crate) can_view_education: bool,
    pub(crate) can_view_federal_registry_admins: bool,
    pub(crate) can_view_city_registry_admins: bool,
    pub(crate) can_crud_city_registry_admins: bool,
    /// 只读「本机构管理员」位:非注册局法人登录后可查看本机构链上管理员列表(只读)。
    pub(crate) can_view_own_admins: bool,
    pub(crate) can_manage_institutions: bool,
    pub(crate) can_register_institutions: bool,
    pub(crate) can_business_write: bool,
    pub(crate) can_view_city_registry: bool,
    pub(crate) can_view_federal_registry: bool,
    /// 立法:查看立法/提案/大屏(立法机构通用只读位)。
    pub(crate) can_view_legislation: bool,
    /// 立法:发起法律案(发起院 / 教委会 / 自治会;参议会无此位)。
    pub(crate) can_propose_legislation: bool,
    /// 立法：当前代表机构表决（发起院 / 参议会 / 国家教委会）。
    pub(crate) can_cast_representative_vote: bool,
    /// 立法:行政签署 / 三人会签 / 护宪终审(行政签署人 / 大法官;另线程接入时置位,本轮恒 false)。
    pub(crate) can_sign_legislation: bool,
    /// 立法:发起任免案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub(crate) can_propose_personnel: bool,
    /// 立法:发起预算案(政府;Phase 4 接入时置位,本轮恒 false)。
    pub(crate) can_propose_budget: bool,
}

const EMPTY: CapabilitySet = CapabilitySet {
    can_view_citizens: false,
    can_view_institutions: false,
    can_view_private: false,
    can_view_education: false,
    can_view_federal_registry_admins: false,
    can_view_city_registry_admins: false,
    can_crud_city_registry_admins: false,
    can_view_own_admins: false,
    can_manage_institutions: false,
    can_register_institutions: false,
    can_business_write: false,
    can_view_city_registry: false,
    can_view_federal_registry: false,
    can_view_legislation: false,
    can_propose_legislation: false,
    can_cast_representative_vote: false,
    can_sign_legislation: false,
    can_propose_personnel: false,
    can_propose_budget: false,
};

// Tier1 创世注册局(FRG):是 CREG 的省级上游,能力必须是 Tier2 业务能力的超集。
// 链上 runtime 已限制 FRG 只能按本省省级组登记本省 CID;这里仅声明控制台可见能力。
const TIER1_REGISTRY: CapabilitySet = CapabilitySet {
    can_view_citizens: true,
    can_view_institutions: true,
    can_view_private: true,
    can_view_education: true,
    can_view_federal_registry_admins: true,
    can_view_city_registry_admins: true,
    can_crud_city_registry_admins: true,
    can_manage_institutions: true,
    can_register_institutions: true,
    can_business_write: true,
    can_view_city_registry: true,
    can_view_federal_registry: true,
    ..EMPTY
};

// Tier2 下级注册局(CREG):录入公民/私权/教育/公权机构 + 看本市市注册局;
// 同时只读本省联邦注册局管理员列表,不得发起联邦注册局管理员操作。
const SUBORDINATE_REGISTRY: CapabilitySet = CapabilitySet {
    can_view_citizens: true,
    can_view_institutions: true,
    can_view_private: true,
    can_view_education: true,
    can_view_federal_registry_admins: true,
    can_view_city_registry_admins: true,
    can_manage_institutions: true,
    can_register_institutions: true,
    can_business_write: true,
    can_view_city_registry: true,
    can_view_federal_registry: true,
    ..EMPTY
};

// 普通机构(NJD/公权/私权/非法人,非注册局):本期只开只读「本机构管理员」入口。
const OWN_ADMINS_READONLY: CapabilitySet = CapabilitySet {
    can_view_own_admins: true,
    ..EMPTY
};

// 立法机构:在「本机构管理员只读」基础上叠加立法能力。
// 发起/表决两个位由立法角色决定(发起院=发起+表决;参议会=只表决;教委会/自治会=只提案)。
// 签署/任免/预算位本轮恒 false,分别由行政签署线程与 Phase 4 接入时置位。
fn legislation_capabilities(role: LegislationRole) -> CapabilitySet {
    let (can_propose_legislation, can_cast_representative_vote) = match role {
        LegislationRole::ProposerHouse => (true, true),
        LegislationRole::ReviewHouse => (false, true),
        LegislationRole::ProposerOnly => (true, false),
    };
    CapabilitySet {
        can_view_own_admins: true,
        can_view_legislation: true,
        can_propose_legislation,
        can_cast_representative_vote,
        ..EMPTY
    }
}

/// 机构码文本 → 能力集。按机构类(`primitives::cid::code` 单源)分发,未知/不归控制台返回空能力。
pub(crate) fn capabilities_for(institution_code: &str) -> CapabilitySet {
    use primitives::cid::code::{
        institution_code_from_str, is_private_legal_code, is_public_legal_code,
        is_unincorporated_code, NRC, PRB, PRC,
    };
    // Tier1/Tier2 注册局保留专属能力集,具体能力必须与 runtime 登记权层级同步。
    if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        return TIER1_REGISTRY;
    }
    if crate::core::chain_runtime::is_subordinate_registry(institution_code) {
        return SUBORDINATE_REGISTRY;
    }
    let Some(code) = institution_code_from_str(institution_code) else {
        return EMPTY;
    };
    // 国家储委会/省储委会/省储行使用节点桌面端,不进入 OnChina 网页控制台。
    if matches!(code, NRC | PRC | PRB) {
        return EMPTY;
    }
    // 立法机构(众议会/参议会/教委会/自治会/市立法会):按立法角色下发立法能力。
    if let Some(role) = legislation_role(institution_code) {
        return legislation_capabilities(role);
    }
    // NJD 与其余公权/私权/非法人机构:只读本机构管理员位。
    if is_public_legal_code(&code) || is_private_legal_code(&code) || is_unincorporated_code(&code)
    {
        return OWN_ADMINS_READONLY;
    }
    EMPTY
}

#[cfg(test)]
mod tests {
    use super::capabilities_for;

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
        assert!(federal.can_view_city_registry && city.can_view_city_registry);
    }

    #[test]
    fn subordinate_registry_can_read_federal_registry_without_registry_crud_flags() {
        let federal = capabilities_for("FRG");
        let city = capabilities_for("CREG");

        // CREG 必须能进入联邦注册局入口只读本省管理员;注册局维护类写权只留给 FRG。
        assert!(city.can_view_federal_registry);
        assert!(city.can_view_federal_registry_admins);
        assert!(!city.can_crud_city_registry_admins);
        assert!(federal.can_view_federal_registry);
        assert!(federal.can_view_federal_registry_admins);
        assert!(federal.can_crud_city_registry_admins);
    }

    #[test]
    fn national_judicial_yuan_can_view_own_admins_only() {
        let judicial = capabilities_for("NJD");

        // NJD 可进入司法院工作台,但本期只给本机构管理员只读页,不获得注册局业务能力。
        assert!(judicial.can_view_own_admins);
        assert!(!judicial.can_view_citizens);
        assert!(!judicial.can_view_institutions);
        assert!(!judicial.can_view_city_registry);
        assert!(!judicial.can_view_federal_registry);
    }

    #[test]
    fn legislative_institutions_get_role_based_legislation_capabilities() {
        // 发起机构（国家众议会）：发起 + 代表机构表决；同时保留本机构管理员只读位。
        let house = capabilities_for("NRP");
        assert!(house.can_view_legislation);
        assert!(house.can_propose_legislation);
        assert!(house.can_cast_representative_vote);
        assert!(house.can_view_own_admins);

        // 参议会:只表决,无发起权(权力分离硬约束)。
        let senate = capabilities_for("NSN");
        assert!(senate.can_cast_representative_vote);
        assert!(!senate.can_propose_legislation);

        // 市教委会：只提案，不参加代表机构表决（由市立法会表决）。
        let city_education = capabilities_for("CEDU");
        assert!(city_education.can_propose_legislation);
        assert!(!city_education.can_cast_representative_vote);

        // 本轮任免/预算/签署位均未接入。
        assert!(!house.can_propose_personnel);
        assert!(!house.can_propose_budget);
        assert!(!house.can_sign_legislation);
    }

    #[test]
    fn reserve_governance_institutions_stay_out_of_onchina() {
        // 国家储委会/省储委会/省储行使用节点桌面端,不能因为同属公权码而拿到网页能力。
        for code in ["NRC", "PRC", "PRB"] {
            let capability = capabilities_for(code);
            assert!(
                !capability.can_view_own_admins,
                "{code} must stay desktop-only"
            );
            assert!(
                !capability.can_view_citizens,
                "{code} must not receive business tabs"
            );
        }
    }
}
