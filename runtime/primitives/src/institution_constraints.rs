//! 国家级单例机构与法定成员组成的永久约束真源。
//!
//! 本模块只固定不可由 runtime 升级改变的制度边界：六个国家机构的创世身份、
//! 参议会/众议会/国家教委会的法定成员岗位及人数区间，以及立法院的组成关系。
//! 六个单例的管理员随合法治理结果运行期增删改；它们没有账户级动态阈值。
//! 普通内部事项读取 entity 保存的机构治理阈值，并按 `VotePlan` 指定岗位的有效任职
//! 生成提案票据快照；不得从管理员人数或管理员名单推导投票资格和阈值。

extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::account_derive::InstitutionProtocolAccountKind;
use crate::cid::china::{
    china_jc::CHINA_JC, china_jy::CHINA_JY, china_lf::CHINA_LF, china_zf::CHINA_ZF,
};
use crate::cid::code::{
    institution_code_from_cid_number, InstitutionCode, FSC, NED, NLG, NRC, NRP, NSN, NSP, PRB, PRS,
    SFGF, UNIN,
};

/// 所有机构共同强制的协议账户。
pub const COMMON_PROTOCOL_ACCOUNT_KINDS: &[InstitutionProtocolAccountKind] = &[
    InstitutionProtocolAccountKind::Main,
    InstitutionProtocolAccountKind::Fee,
];

/// 国储会强制协议账户。
pub const NRC_PROTOCOL_ACCOUNT_KINDS: &[InstitutionProtocolAccountKind] = &[
    InstitutionProtocolAccountKind::Main,
    InstitutionProtocolAccountKind::Fee,
    InstitutionProtocolAccountKind::SafetyFund,
    InstitutionProtocolAccountKind::He,
];

/// 省储行强制协议账户。
pub const PRB_PROTOCOL_ACCOUNT_KINDS: &[InstitutionProtocolAccountKind] = &[
    InstitutionProtocolAccountKind::Main,
    InstitutionProtocolAccountKind::Fee,
    InstitutionProtocolAccountKind::Stake,
];

/// 清算行资格机构强制协议账户。
///
/// 在主账户、费用账户之外多一个「清算账户」,承载扫码支付 L2 清算资金。
/// 适用于 `SFGF` 私法人股份公司,以及父级为 `SFGF` 的 `UNIN` 非法人分支机构;
/// 注册局登记该机构时自动派生并登记清算账户。
pub const CORPORATION_PROTOCOL_ACCOUNT_KINDS: &[InstitutionProtocolAccountKind] = &[
    InstitutionProtocolAccountKind::Main,
    InstitutionProtocolAccountKind::Fee,
    InstitutionProtocolAccountKind::Clearing,
];

/// 联邦安全局(FSC)强制协议账户。
///
/// 联邦安全局在主账户、费用账户之外多一个「联邦公民安全基金」,由该局经投票引擎支出;
/// 该基金为 FSC 专属,其他机构一律没有。
pub const FSC_PROTOCOL_ACCOUNT_KINDS: &[InstitutionProtocolAccountKind] = &[
    InstitutionProtocolAccountKind::Main,
    InstitutionProtocolAccountKind::Fee,
    InstitutionProtocolAccountKind::FederalCitizenSecurityFund,
];

/// 清算账户资格的唯一父级判据:父级机构码是否私法人股份公司(`SFGF`)。
///
/// 机构码编码在 CID 内,纯字节判定、不读 storage。父级是否**已登记**由注册入口
/// 在 entity 层用 `Institutions` 另行校验,本模块只定规则。
fn parent_is_joint_stock(parent_cid_number: Option<&[u8]>) -> bool {
    parent_cid_number
        .and_then(|raw| core::str::from_utf8(raw).ok())
        .and_then(institution_code_from_cid_number)
        == Some(SFGF)
}

/// 返回机构必须完整拥有的协议账户集合。
///
/// CID 与机构码必须互相匹配；不匹配时返回 `None`，调用方不得自行回落到普通机构规则。
///
/// **清算账户资格(唯二)**:`SFGF` 私法人股份公司本身,以及**父级机构码为 `SFGF` 的
/// `UNIN` 非法人组织**(股份公司的非法人分支机构,如银行分支行)。其余机构一律没有
/// 清算账户 —— 含 `SFGT` 个体经营、`SFGP` 无限合伙(两者是独立经营主体、无父级概念)、
/// 父级不是 `SFGF` 的 `UNIN`,以及全部公权与其他私权机构。
///
/// `parent_cid_number` 只对 `UNIN` 有意义,其余机构码一律传 `None`。
pub fn required_protocol_account_kinds(
    code: InstitutionCode,
    cid_number: &[u8],
    parent_cid_number: Option<&[u8]>,
) -> Option<&'static [InstitutionProtocolAccountKind]> {
    if institution_code_from_cid_number(core::str::from_utf8(cid_number).ok()?) != Some(code) {
        return None;
    }
    Some(match code {
        NRC => NRC_PROTOCOL_ACCOUNT_KINDS,
        PRB => PRB_PROTOCOL_ACCOUNT_KINDS,
        // 联邦安全局:唯一持有「联邦公民安全基金」的机构。
        FSC => FSC_PROTOCOL_ACCOUNT_KINDS,
        // 股份公司本身。
        SFGF => CORPORATION_PROTOCOL_ACCOUNT_KINDS,
        // 股份公司的非法人分支机构:父级码必须是 SFGF 才配清算账户。
        UNIN if parent_is_joint_stock(parent_cid_number) => CORPORATION_PROTOCOL_ACCOUNT_KINDS,
        // 其余(含父级非 SFGF 的 UNIN)一律 {主, 费}，没有清算账户。
        _ => COMMON_PROTOCOL_ACCOUNT_KINDS,
    })
}

/// 机构**允许**出现的协议账户集合（链上校验/节点守卫用）。
///
/// 与 `required_protocol_account_kinds` 的区别:后者是登记时**必须完整拥有**的集合
/// (需要父级才能判定 `UNIN` 是否股份公司分支);本函数是校验既有链上状态时**允许出现**
/// 的集合。因为**链上不保存 `UNIN` 的父级**,守卫无从判定它是不是股份公司分支,
/// 故 `UNIN` 的清算账户必须「可有可无」—— 两种账户集都得接受,否则会把合法的
/// 银行分支行判成非法。其余机构码与 `required_protocol_account_kinds` 完全一致。
pub fn allowed_protocol_account_kinds(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<&'static [InstitutionProtocolAccountKind]> {
    if institution_code_from_cid_number(core::str::from_utf8(cid_number).ok()?) != Some(code) {
        return None;
    }
    if code == UNIN {
        // CORPORATION 集是 COMMON 集的超集,故可同时容纳「带清算账户的分支行」
        // 与「不带清算账户的普通非法人组织」。
        return Some(CORPORATION_PROTOCOL_ACCOUNT_KINDS);
    }
    required_protocol_account_kinds(code, cid_number, None)
}

/// 国家参议会法定成员岗位代码。
pub const ROLE_CODE_SENATOR: &[u8] = b"SENATOR";
/// 国家众议会法定成员岗位代码。
pub const ROLE_CODE_REPRESENTATIVE: &[u8] = b"REPRESENTATIVE";
/// 国家教委会法定委员岗位代码。
pub const ROLE_CODE_COMMITTEE_MEMBER: &[u8] = b"COMMITTEE_MEMBER";
/// 每个机构自动拥有且只能拥有一个的法定代表人岗位代码。
pub const ROLE_CODE_LEGAL_REPRESENTATIVE: &[u8] = b"LR";
/// 联邦安全局局长岗位代码（该局创世岗位之一，与自带的 LR 并存）。
pub const ROLE_CODE_DIRECTOR: &[u8] = b"DIRECTOR";

/// 法定成员岗位公开名称。
pub const ROLE_NAME_SENATOR: &[u8] = "参议员".as_bytes();
pub const ROLE_NAME_REPRESENTATIVE: &[u8] = "众议员".as_bytes();
pub const ROLE_NAME_COMMITTEE_MEMBER: &[u8] = "委员".as_bytes();
/// 默认法定代表人岗位公开名称。
pub const ROLE_NAME_LEGAL_REPRESENTATIVE: &[u8] = "法定代表人".as_bytes();
/// 联邦安全局局长岗位公开名称。
pub const ROLE_NAME_DIRECTOR: &[u8] = "局长".as_bytes();

/// 岗位码是否为不可删除、停用、改码或改名的默认法定代表人岗位。
pub fn is_legal_representative_role(role_code: &[u8]) -> bool {
    role_code == ROLE_CODE_LEGAL_REPRESENTATIVE
}

/// 国家级永久单例机构的精确创世身份。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SingletonInstitution {
    pub code: InstitutionCode,
    pub cid_number: &'static str,
    pub main_account: [u8; 32],
}

/// 指定机构的法定成员组成区间。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MemberCompositionSpec {
    pub institution: SingletonInstitution,
    pub role_code: &'static [u8],
    pub role_name: &'static [u8],
    pub min_members: u32,
    pub max_members: u32,
}

fn singleton_from_lf(code: InstitutionCode) -> SingletonInstitution {
    let item = CHINA_LF
        .iter()
        .find(|item| institution_code_from_cid_number(item.cid_number) == Some(code))
        .expect("CHINA_LF must contain national legislature singleton");
    SingletonInstitution {
        code,
        cid_number: item.cid_number,
        main_account: item.main_account,
    }
}

/// 六个国家级永久单例，身份直接引用创世 CID 常量表。
pub fn singleton_institutions() -> Vec<SingletonInstitution> {
    let prs = &CHINA_ZF[0];
    let nsp = &CHINA_JC[0];
    let ned = &CHINA_JY[0];
    vec![
        SingletonInstitution {
            code: PRS,
            cid_number: prs.cid_number,
            main_account: prs.main_account,
        },
        singleton_from_lf(NLG),
        singleton_from_lf(NSN),
        singleton_from_lf(NRP),
        SingletonInstitution {
            code: NSP,
            cid_number: nsp.cid_number,
            main_account: nsp.main_account,
        },
        SingletonInstitution {
            code: NED,
            cid_number: ned.cid_number,
            main_account: ned.main_account,
        },
    ]
}

/// 机构码是否属于只能由创世身份占用的六个国家级单例码。
pub fn is_permanent_singleton_code(code: &InstitutionCode) -> bool {
    matches!(*code, PRS | NLG | NSN | NRP | NSP | NED)
}

/// 按完整身份查询国家级永久单例；仅机构码相同不会命中。
pub fn singleton_by_identity(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<SingletonInstitution> {
    singleton_institutions().into_iter().find(|institution| {
        institution.code == code && institution.cid_number.as_bytes() == cid_number
    })
}

/// 按 CID 查询国家级永久单例。
pub fn singleton_by_cid(cid_number: &[u8]) -> Option<SingletonInstitution> {
    singleton_institutions()
        .into_iter()
        .find(|institution| institution.cid_number.as_bytes() == cid_number)
}

/// 三个法定成员机构的人数区间和唯一成员岗位。
pub fn member_composition_specs() -> Vec<MemberCompositionSpec> {
    vec![
        MemberCompositionSpec {
            institution: singleton_from_lf(NSN),
            role_code: ROLE_CODE_SENATOR,
            role_name: ROLE_NAME_SENATOR,
            min_members: 105,
            max_members: 155,
        },
        MemberCompositionSpec {
            institution: singleton_from_lf(NRP),
            role_code: ROLE_CODE_REPRESENTATIVE,
            role_name: ROLE_NAME_REPRESENTATIVE,
            min_members: 305,
            max_members: 355,
        },
        MemberCompositionSpec {
            institution: singleton_institutions()
                .into_iter()
                .find(|item| item.code == NED)
                .expect("singleton list must contain NED"),
            role_code: ROLE_CODE_COMMITTEE_MEMBER,
            role_name: ROLE_NAME_COMMITTEE_MEMBER,
            min_members: 105,
            max_members: 155,
        },
    ]
}

/// 按完整身份查询法定成员组成约束。
pub fn member_composition_by_identity(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<MemberCompositionSpec> {
    member_composition_specs().into_iter().find(|spec| {
        spec.institution.code == code && spec.institution.cid_number.as_bytes() == cid_number
    })
}

/// 国家立法院由国家参议会和国家众议会组成，不额外冻结 NLG 自身岗位。
pub fn national_legislature_components() -> [SingletonInstitution; 2] {
    [singleton_from_lf(NSN), singleton_from_lf(NRP)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_identities_and_codes_are_unique() {
        let list = singleton_institutions();
        assert_eq!(list.len(), 6);
        for (index, item) in list.iter().enumerate() {
            assert!(is_permanent_singleton_code(&item.code));
            assert!(!list[..index].iter().any(|other| other.code == item.code
                || other.cid_number == item.cid_number
                || other.main_account == item.main_account));
        }
    }

    #[test]
    fn member_composition_ranges_match_institutional_design() {
        let specs = member_composition_specs();
        assert_eq!(specs.len(), 3);
        assert_eq!((specs[0].min_members, specs[0].max_members), (105, 155));
        assert_eq!((specs[1].min_members, specs[1].max_members), (305, 355));
        assert_eq!((specs[2].min_members, specs[2].max_members), (105, 155));
        assert_eq!(national_legislature_components()[0].code, NSN);
        assert_eq!(national_legislature_components()[1].code, NRP);
    }

    #[test]
    fn required_protocol_accounts_follow_institution_design() {
        let nrc = crate::cid::china::china_cb::CHINA_CB
            .iter()
            .find(|item| institution_code_from_cid_number(item.cid_number) == Some(NRC))
            .expect("NRC genesis institution");
        assert_eq!(
            required_protocol_account_kinds(NRC, nrc.cid_number.as_bytes(), None),
            Some(NRC_PROTOCOL_ACCOUNT_KINDS)
        );

        let prb = crate::cid::china::china_ch::CHINA_CH
            .iter()
            .find(|item| institution_code_from_cid_number(item.cid_number) == Some(PRB))
            .expect("PRB genesis institution");
        assert_eq!(
            required_protocol_account_kinds(PRB, prb.cid_number.as_bytes(), None),
            Some(PRB_PROTOCOL_ACCOUNT_KINDS)
        );

        assert_eq!(
            required_protocol_account_kinds(NRC, prb.cid_number.as_bytes(), None),
            None
        );
    }

    /// 清算账户资格唯二：SFGF 本身、以及父级为 SFGF 的 UNIN；其余一律无清算账户。
    #[test]
    fn clearing_account_only_for_joint_stock_and_its_unincorporated_branch() {
        const SFGF_CID: &[u8] = b"GD001-SFGF0-123456789-2026";
        const UNIN_CID: &[u8] = b"GD001-UNIN0-123456789-2026";
        const SFLP_CID: &[u8] = b"GD001-SFLP0-123456789-2026";
        const SFGT_CID: &[u8] = b"GD001-SFGT0-123456789-2026";
        const SFGP_CID: &[u8] = b"GD001-SFGP0-123456789-2026";

        let has_clearing = |kinds: Option<&'static [InstitutionProtocolAccountKind]>| {
            kinds
                .expect("CID 与机构码一致")
                .contains(&InstitutionProtocolAccountKind::Clearing)
        };

        // ① 股份公司本身。
        assert!(has_clearing(required_protocol_account_kinds(
            SFGF, SFGF_CID, None
        )));
        // ② 父级为 SFGF 的非法人分支机构。
        assert!(has_clearing(required_protocol_account_kinds(
            UNIN,
            UNIN_CID,
            Some(SFGF_CID)
        )));

        // 父级不是股份公司的 UNIN：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            UNIN,
            UNIN_CID,
            Some(SFLP_CID)
        )));
        // 无父级的 UNIN：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            UNIN, UNIN_CID, None
        )));
        // 个体经营 / 无限合伙是独立经营主体、无父级概念：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFGT", SFGT_CID, None
        )));
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFGP", SFGP_CID, None
        )));
        // 普通私法人：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFLP", SFLP_CID, None
        )));
    }
}
