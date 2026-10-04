//! 固定治理骨架冻结规格(档 A 单源)。
//!
//! `public-admins::AdminAccounts` 保存固定机构管理员账户，`public-manage` 保存岗位与任职。
//! 本模块把**永不合法变更的结构骨架**收敛为编译常量清单，供三端共读、防止规格漂移:
//!   1. 创世播种(`runtime/genesis`):写入固定机构、岗位、任职与管理员账户;
//!   2. runtime 查询(`public-manage`):按稳定岗位代码读取有效任职;
//!   3. 节点守卫(`node/src/core/node_guard/governance_skeleton.rs`):读取固定账户骨架；
//!      entity 岗位与席位的逐块 RAW 守卫按后续步骤接入。
//!
//! 本文件是跨 runtime、genesis 和 native node 的协议清单，不负责写入 storage。
//! 实际创世写入唯一发生在 `runtime/genesis::institution`；runtime/node 只消费这里的
//! 编译期规格。这样 Node Guard 不需要依赖可升级 runtime，也不会因 runtime/genesis
//! 的实现变化而手抄另一份席位常量。
//!
//! **只冻结构,不冻成员**:固定机构的存在性/机构码/名额/护宪席位数是永不合法变更的
//! 结构量;而"座位上坐的是谁"由普选/互选/阈值票合法轮换,不在本规格内(等长换座即过)。
//! **不含阈值**:固定治理阈值是 `fixed_governance_pass_threshold` 计票逻辑、不落 state,守卫
//! 锚不到,故不在本规格。成员劫持(保持席位数、整体换攻击者密钥)属档 B(创世根验签链),不在此。

extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::cid::china::china_cb::CHINA_CB;
use crate::cid::china::china_ch::CHINA_CH;
use crate::cid::china::{china_sf::CHINA_SF, china_zf::CHINA_ZF};
use crate::cid::code::{
    institution_code_from_cid_number, InstitutionCode, ProvinceCode, FRG, NJD, NRC, PRB, PRC,
    PROVINCE_CODE_INFOS,
};
use crate::count_const::{
    FRG_PROVINCE_GROUP_ADMIN_COUNT, NJD_ADMIN_COUNT, NRC_ADMIN_COUNT, PRB_ADMIN_COUNT,
    PRC_ADMIN_COUNT,
};
use crate::institution_constraints::{
    ROLE_CODE_LEGAL_REPRESENTATIVE, ROLE_NAME_LEGAL_REPRESENTATIVE,
};

/// 护宪大法官公开岗位名单源。创世与节点守卫逐字节共用，禁止各处手写字符串。
pub const ROLE_CONSTITUTION_GUARD: &[u8] = "护宪大法官".as_bytes();

/// 固定创世岗位代码。岗位代码是授权键，中文岗位名只用于公开展示。
pub const ROLE_CODE_COMMITTEE_MEMBER: &[u8] = b"COMMITTEE_MEMBER";
pub const ROLE_CODE_DIRECTOR: &[u8] = b"DIRECTOR";
pub const ROLE_CODE_CONSTITUTION_GUARD: &[u8] = b"CONSTITUTION_GUARD";
pub const ROLE_CODE_CHIEF_JUSTICE: &[u8] = b"CHIEF_JUSTICE";
pub const ROLE_CODE_DEPUTY_CHIEF_JUSTICE: &[u8] = b"DEPUTY_CHIEF_JUSTICE";
pub const ROLE_CODE_JUSTICE: &[u8] = b"JUSTICE";
pub const ROLE_CODE_PROVINCE_COMMISSIONER_PREFIX: &[u8] = b"PROVINCE_COMMISSIONER_";

/// 固定创世岗位公开名称。
pub const ROLE_NAME_COMMITTEE_MEMBER: &[u8] = "委员".as_bytes();
pub const ROLE_NAME_DIRECTOR: &[u8] = "董事".as_bytes();
pub const ROLE_NAME_CHIEF_JUSTICE: &[u8] = "首席大法官".as_bytes();
pub const ROLE_NAME_DEPUTY_CHIEF_JUSTICE: &[u8] = "次席大法官".as_bytes();
pub const ROLE_NAME_JUSTICE: &[u8] = "大法官".as_bytes();

/// 联邦注册局省专员岗位代码：`PROVINCE_COMMISSIONER_<两位省码>`。
pub fn province_commissioner_role_code(province_code: ProvinceCode) -> Vec<u8> {
    let mut out = Vec::with_capacity(ROLE_CODE_PROVINCE_COMMISSIONER_PREFIX.len() + 2);
    out.extend_from_slice(ROLE_CODE_PROVINCE_COMMISSIONER_PREFIX);
    out.extend_from_slice(&province_code);
    out
}

/// 联邦注册局省专员岗位名称：`<省行政区名称>专员`。
pub fn province_commissioner_role_name(province_name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(province_name.len() + "专员".len());
    out.extend_from_slice(province_name.as_bytes());
    out.extend_from_slice("专员".as_bytes());
    out
}

/// NJD 护宪大法官法庭固定席位数 = 公民宪法第 21 条 4/7 终审的「7」。
///
/// 创世 `fixed_roles` 的账户索引 0..=6 落 7 名护宪，本常量即其规格单源；
/// 创世岗位映射与后续 entity 岗位守卫都使用本值，补上宪法守卫
/// `guard_review_passed(approve>=4)` 里从未被锚定的「7」(见 ADR-027 §6.3)。
pub const NJD_CONSTITUTION_GUARD_SEATS: u32 = 7;

/// `InstitutionRoleStatus::Active` 与 `InstitutionAssignmentStatus::Active` 的判别值。
pub const ROLE_STATUS_ACTIVE: u8 = 0;
pub const ASSIGNMENT_STATUS_ACTIVE: u8 = 0;

/// 固定机构岗位与席位规格。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FixedRoleSpec {
    /// 稳定岗位代码。
    pub role_code: &'static [u8],
    /// 公开岗位名。
    pub role_name: &'static [u8],
    /// 固定席位数。
    pub seats: u32,
}

/// 单个固定治理机构的冻结规格。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FixedInstitution {
    /// 机构码(NRC/PRC/PRB/NJD/FRG)。
    pub code: InstitutionCode,
    /// 机构主账户是协议账户完整性事实，不是 `AdminAccounts` 键或机构身份。
    pub main_account: [u8; 32],
    /// 机构 CID = entity 岗位与任职双映射的第一层键。
    pub cid_number: &'static str,
    /// 固定管理员总数(仅允许等长换人)。
    pub expected_len: u32,
}

/// 枚举全部 89 个受保护创世治理机构(纯编译常量,不读链)。
///
/// NRC/PRC 来自 `CHINA_CB`,PRB 来自 `CHINA_CH`,NJD 来自 `CHINA_SF`;三者创世即
/// `insert_fixed_admins` 写入 `AdminAccounts`,故 block#0 state 与本规格双锚。
pub fn fixed_institutions() -> Vec<FixedInstitution> {
    let mut out = Vec::new();
    for node in CHINA_CB.iter() {
        let Some(code) = institution_code_from_cid_number(node.cid_number) else {
            continue;
        };
        if code == NRC {
            out.push(FixedInstitution {
                code: NRC,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: NRC_ADMIN_COUNT,
            });
        } else if code == PRC {
            out.push(FixedInstitution {
                code: PRC,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: PRC_ADMIN_COUNT,
            });
        }
    }
    for node in CHINA_CH.iter() {
        if institution_code_from_cid_number(node.cid_number) == Some(PRB) {
            out.push(FixedInstitution {
                code: PRB,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: PRB_ADMIN_COUNT,
            });
        }
    }
    for node in CHINA_SF.iter() {
        if institution_code_from_cid_number(node.cid_number) == Some(NJD) {
            out.push(FixedInstitution {
                code: NJD,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: NJD_ADMIN_COUNT,
            });
        }
    }
    // FRG 是一个机构；43 个省专员岗位的 5 人席位由 entity 任职表达，
    // admins 账户集合在 public-admins 中聚合为 215 人，不再生成虚拟省组账户。
    out.push(federal_registry_institution());
    out
}

/// 按创世完整身份查询受保护治理机构。
///
/// 机构码只表示业务类别；固定治理身份必须同时匹配创世 CID。
pub fn fixed_institution_by_identity(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<FixedInstitution> {
    fixed_institutions().into_iter().find(|institution| {
        institution.code == code && institution.cid_number.as_bytes() == cid_number
    })
}

/// 按创世 CID 查询受保护治理机构，供节点解析岗位与任职双映射 key。
pub fn fixed_institution_by_cid(cid_number: &[u8]) -> Option<FixedInstitution> {
    fixed_institutions()
        .into_iter()
        .find(|institution| institution.cid_number.as_bytes() == cid_number)
}

/// 固定机构的岗位与席位。FRG 的 43 个省专员岗位由省码动态生成，不走本函数。
pub fn fixed_role_specs(code: InstitutionCode) -> Vec<FixedRoleSpec> {
    let mut roles = match code {
        NRC | PRC => vec![FixedRoleSpec {
            role_code: ROLE_CODE_COMMITTEE_MEMBER,
            role_name: ROLE_NAME_COMMITTEE_MEMBER,
            seats: if code == NRC {
                NRC_ADMIN_COUNT
            } else {
                PRC_ADMIN_COUNT
            },
        }],
        PRB => vec![FixedRoleSpec {
            role_code: ROLE_CODE_DIRECTOR,
            role_name: ROLE_NAME_DIRECTOR,
            seats: PRB_ADMIN_COUNT,
        }],
        NJD => vec![
            FixedRoleSpec {
                role_code: ROLE_CODE_CONSTITUTION_GUARD,
                role_name: ROLE_CONSTITUTION_GUARD,
                seats: NJD_CONSTITUTION_GUARD_SEATS,
            },
            FixedRoleSpec {
                role_code: ROLE_CODE_CHIEF_JUSTICE,
                role_name: ROLE_NAME_CHIEF_JUSTICE,
                seats: 1,
            },
            FixedRoleSpec {
                role_code: ROLE_CODE_DEPUTY_CHIEF_JUSTICE,
                role_name: ROLE_NAME_DEPUTY_CHIEF_JUSTICE,
                seats: 2,
            },
            FixedRoleSpec {
                role_code: ROLE_CODE_JUSTICE,
                role_name: ROLE_NAME_JUSTICE,
                seats: 5,
            },
        ],
        _ => Vec::new(),
    };
    if matches!(code, NRC | PRC | PRB | NJD) {
        roles.push(FixedRoleSpec {
            role_code: ROLE_CODE_LEGAL_REPRESENTATIVE,
            role_name: ROLE_NAME_LEGAL_REPRESENTATIVE,
            seats: 0,
        });
    }
    roles
}

/// 查询固定机构某岗位的法定席位数；非固定机构或清单外岗位返回 `None`。
///
/// entity 在应用选举结果前使用本函数执行 runtime 侧前置拒绝，Node Guard 再从节点层
/// 独立复核同一清单。FRG 的 43 个省专员岗位按稳定省码逐一匹配。
pub fn fixed_role_seats(code: InstitutionCode, role_code: &[u8]) -> Option<u32> {
    if role_code == ROLE_CODE_LEGAL_REPRESENTATIVE {
        return Some(0);
    }
    if code == FRG {
        return PROVINCE_CODE_INFOS.iter().find_map(|province| {
            (province_commissioner_role_code(province.province_code) == role_code)
                .then_some(FRG_PROVINCE_GROUP_ADMIN_COUNT)
        });
    }
    fixed_role_specs(code)
        .into_iter()
        .find(|role| role.role_code == role_code)
        .map(|role| role.seats)
}

/// 查询具体受保护创世机构的岗位席位；仅机构码相同不会命中。
pub fn fixed_role_seats_by_identity(
    code: InstitutionCode,
    cid_number: &[u8],
    role_code: &[u8],
) -> Option<u32> {
    fixed_institution_by_identity(code, cid_number)?;
    fixed_role_seats(code, role_code)
}

/// 查询受保护创世岗位允许的有效任职人数区间。
///
/// 普通固定岗位必须保持协议席位数；`LR / 法定代表人` 是每个机构唯一且永久存在的
/// 岗位，但任职允许在空缺和一人之间依法切换。阈值不属于岗位，不在此处表达。
pub fn fixed_role_assignment_bounds_by_identity(
    code: InstitutionCode,
    cid_number: &[u8],
    role_code: &[u8],
) -> Option<(u32, u32)> {
    let seats = fixed_role_seats_by_identity(code, cid_number, role_code)?;
    if role_code == ROLE_CODE_LEGAL_REPRESENTATIVE {
        Some((0, 1))
    } else {
        Some((seats, seats))
    }
}

/// 联邦注册局固定机构规格。FRG 只有一个管理员账户，省级边界由 43 个岗位表达。
pub fn federal_registry_institution() -> FixedInstitution {
    let node = CHINA_ZF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(FRG))
        .expect("CHINA_ZF must contain FRG");
    FixedInstitution {
        code: FRG,
        main_account: node.main_account,
        cid_number: node.cid_number,
        expected_len: PROVINCE_CODE_INFOS.len() as u32 * FRG_PROVINCE_GROUP_ADMIN_COUNT,
    }
}

/// FRG 省专员岗位冻结规格：每省一条 `(省码, 固定人数=5)`。
pub fn frg_province_groups() -> Vec<(ProvinceCode, u32)> {
    PROVINCE_CODE_INFOS
        .iter()
        .map(|p| (p.province_code, FRG_PROVINCE_GROUP_ADMIN_COUNT))
        .collect()
}

#[cfg(test)]
mod tests {
    // 测试代码沿用 expect() 断言(工作区 expect_used=warn 面向生产码;测试内 expect 是惯用法)。
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn fixed_institutions_cover_fixed_codes_with_expected_counts() {
        let list = fixed_institutions();

        assert_eq!(list.len(), 89, "受保护创世治理机构必须精确为 89 个");
        assert_eq!(list.iter().filter(|item| item.code == NRC).count(), 1);
        assert_eq!(list.iter().filter(|item| item.code == PRC).count(), 43);
        assert_eq!(list.iter().filter(|item| item.code == PRB).count(), 43);
        assert_eq!(list.iter().filter(|item| item.code == NJD).count(), 1);
        assert_eq!(list.iter().filter(|item| item.code == FRG).count(), 1);

        let mut cids = list.iter().map(|item| item.cid_number).collect::<Vec<_>>();
        cids.sort_unstable();
        cids.dedup();
        assert_eq!(cids.len(), list.len(), "保护清单 CID 不得重复");
        let mut accounts = list
            .iter()
            .map(|item| item.main_account)
            .collect::<Vec<_>>();
        accounts.sort_unstable();
        accounts.dedup();
        assert_eq!(accounts.len(), list.len(), "保护清单主账户不得重复");

        let nrc = list.iter().find(|f| f.code == NRC).expect("NRC 必须在册");
        assert_eq!(nrc.expected_len, NRC_ADMIN_COUNT);

        let njd = list.iter().find(|f| f.code == NJD).expect("NJD 必须在册");
        assert_eq!(njd.expected_len, NJD_ADMIN_COUNT);
        let court = fixed_role_specs(NJD)
            .into_iter()
            .find(|role| role.role_code == ROLE_CODE_CONSTITUTION_GUARD)
            .expect("NJD 必须有护宪岗位");
        assert_eq!(court.role_name, ROLE_CONSTITUTION_GUARD);
        assert_eq!(court.seats, NJD_CONSTITUTION_GUARD_SEATS);

        assert!(list
            .iter()
            .any(|f| f.code == PRC && f.expected_len == PRC_ADMIN_COUNT));
        assert!(list
            .iter()
            .any(|f| f.code == PRB && f.expected_len == PRB_ADMIN_COUNT));
        let frg = list
            .iter()
            .find(|f| f.code == FRG)
            .expect("FRG 必须作为受保护创世机构在册");
        assert_eq!(
            frg.expected_len,
            PROVINCE_CODE_INFOS.len() as u32 * FRG_PROVINCE_GROUP_ADMIN_COUNT
        );
    }

    #[test]
    fn fixed_identity_requires_code_and_cid_to_match() {
        let institution = fixed_institutions()[0];
        assert!(
            fixed_institution_by_identity(institution.code, institution.cid_number.as_bytes(),)
                .is_some()
        );
        assert!(fixed_institution_by_identity(institution.code, b"not-genesis-cid",).is_none());
        assert!(
            fixed_institution_by_identity(*b"CGOV", institution.cid_number.as_bytes(),).is_none()
        );
    }

    #[test]
    fn legal_representative_has_one_role_and_zero_or_one_assignment() {
        let institution = fixed_institutions()[0];
        assert_eq!(
            fixed_role_assignment_bounds_by_identity(
                institution.code,
                institution.cid_number.as_bytes(),
                ROLE_CODE_LEGAL_REPRESENTATIVE,
            ),
            Some((0, 1))
        );
        assert_eq!(
            fixed_role_assignment_bounds_by_identity(
                institution.code,
                institution.cid_number.as_bytes(),
                fixed_role_specs(institution.code)[0].role_code,
            ),
            Some((institution.expected_len, institution.expected_len))
        );
    }

    #[test]
    fn njd_guard_seats_is_seven() {
        // 4/7 终审的「7」;与创世 role-by-index 0..=6 一致。
        assert_eq!(NJD_CONSTITUTION_GUARD_SEATS, 7);
    }

    #[test]
    fn frg_groups_cover_all_provinces_with_five_seats() {
        let groups = frg_province_groups();
        assert_eq!(groups.len(), PROVINCE_CODE_INFOS.len());
        assert!(groups
            .iter()
            .all(|(_, n)| *n == FRG_PROVINCE_GROUP_ADMIN_COUNT));
    }

    #[test]
    fn role_literal_is_stable() {
        assert_eq!(ROLE_CONSTITUTION_GUARD, "护宪大法官".as_bytes());
    }

    #[test]
    fn fixed_role_seat_lookup_covers_judicial_and_federal_registry_roles() {
        assert_eq!(fixed_role_seats(NJD, ROLE_CODE_CONSTITUTION_GUARD), Some(7));
        let first_province = PROVINCE_CODE_INFOS[0];
        assert_eq!(
            fixed_role_seats(
                FRG,
                &province_commissioner_role_code(first_province.province_code)
            ),
            Some(FRG_PROVINCE_GROUP_ADMIN_COUNT)
        );
        assert_eq!(fixed_role_seats(NJD, b"UNKNOWN"), None);
    }
}
