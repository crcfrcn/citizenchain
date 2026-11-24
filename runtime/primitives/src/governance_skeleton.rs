//! 冻结受保护机构的治理岗位及席位规格。

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
    FRG_COMMISSIONER_SEATS, NJD_JUDICIAL_SEATS, NRC_COMMITTEE_SEATS, PRB_DIRECTOR_SEATS,
    PRC_COMMITTEE_SEATS,
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
    /// 纳入治理组的固定岗位席位总数。
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
                expected_len: NRC_COMMITTEE_SEATS,
            });
        } else if code == PRC {
            out.push(FixedInstitution {
                code: PRC,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: PRC_COMMITTEE_SEATS,
            });
        }
    }
    for node in CHINA_CH.iter() {
        if institution_code_from_cid_number(node.cid_number) == Some(PRB) {
            out.push(FixedInstitution {
                code: PRB,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: PRB_DIRECTOR_SEATS,
            });
        }
    }
    for node in CHINA_SF.iter() {
        if institution_code_from_cid_number(node.cid_number) == Some(NJD) {
            out.push(FixedInstitution {
                code: NJD,
                main_account: node.main_account,
                cid_number: node.cid_number,
                expected_len: NJD_JUDICIAL_SEATS,
            });
        }
    }
    // FRG 是一个机构；43 个省专员岗位的 5 人席位由 entity 任职表达，
    // 43 个省专员岗位各有五席，任职由公权实体按人员 CID 记录。
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
                NRC_COMMITTEE_SEATS
            } else {
                PRC_COMMITTEE_SEATS
            },
        }],
        PRB => vec![FixedRoleSpec {
            role_code: ROLE_CODE_DIRECTOR,
            role_name: ROLE_NAME_DIRECTOR,
            seats: PRB_DIRECTOR_SEATS,
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
            seats: 1,
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
        return Some(1);
    }
    if code == FRG {
        return PROVINCE_CODE_INFOS.iter().find_map(|province| {
            (province_commissioner_role_code(province.province_code) == role_code)
                .then_some(FRG_COMMISSIONER_SEATS)
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
/// 普通固定岗位必须保持协议席位数。每个机构永久设有一个法定代表人岗位（LR），
/// 该岗位有一席，任职允许在空缺和一人之间依法切换。这里校验任职人数，投票阈值另行确定。
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

/// 联邦注册局固定机构规格，43 个省专员岗位分别承担辖区权限。
pub fn federal_registry_institution() -> FixedInstitution {
    let node = CHINA_ZF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(FRG))
        .expect("CHINA_ZF must contain FRG");
    FixedInstitution {
        code: FRG,
        main_account: node.main_account,
        cid_number: node.cid_number,
        expected_len: PROVINCE_CODE_INFOS.len() as u32 * FRG_COMMISSIONER_SEATS,
    }
}

/// FRG 省专员岗位冻结规格：每省一条 `(省码, 固定人数=5)`。
pub fn frg_province_groups() -> Vec<(ProvinceCode, u32)> {
    PROVINCE_CODE_INFOS
        .iter()
        .map(|p| (p.province_code, FRG_COMMISSIONER_SEATS))
        .collect()
}
