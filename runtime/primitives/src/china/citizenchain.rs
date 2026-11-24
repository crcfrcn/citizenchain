//! 定义公民链技术发展基金会的创世身份、协议账户及固定岗位。

use hex_literal::hex;

use crate::institution_constraints::{
    ROLE_CODE_LEGAL_REPRESENTATIVE, ROLE_NAME_LEGAL_REPRESENTATIVE,
};

/// 创世产品经理固定岗位代码。
pub const ROLE_CODE_GENESIS_PRODUCT_MANAGER: &[u8] = b"GENESIS_PRODUCT_MANAGER";
/// 创世程序员固定岗位代码。
pub const ROLE_CODE_GENESIS_PROGRAMMER: &[u8] = b"GENESIS_PROGRAMMER";
/// 创世产品经理固定岗位名称。
pub const ROLE_NAME_GENESIS_PRODUCT_MANAGER: &[u8] = "创世产品经理".as_bytes();
/// 创世程序员固定岗位名称。
pub const ROLE_NAME_GENESIS_PROGRAMMER: &[u8] = "创世程序员".as_bytes();

/// 法定代表人对应的公民 CID；注册局后续直接从链上读取该公民资料。
pub const LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER: &str = "CN220-CTZN2-198805200-2026";
/// 法定代表人的姓；全仓人员姓名只使用 `family_name` 与 `given_name` 两个字段。
pub const LEGAL_REPRESENTATIVE_FAMILY_NAME: &str = "程";
/// 法定代表人的名；禁止恢复合并姓名字段。
pub const LEGAL_REPRESENTATIVE_GIVEN_NAME: &str = "伟";

/// 公民链技术发展基金会内置身份。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChinaCitizenChain {
    pub cid_full_name: &'static str,
    pub cid_short_name: &'static str,
    pub full_name_en: &'static str,
    pub short_name_en: &'static str,
    pub cid_number: &'static str,
    pub main_account: [u8; 32],
    pub fee_account: [u8; 32],
}

/// 三个岗位的冻结规格；岗位码、岗位名、启用状态和一岗一席均受节点守卫保护。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CitizenChainFixedRole {
    pub role_code: &'static [u8],
    pub role_name: &'static [u8],
    pub seats: u32,
}

/// 公民链技术发展基金会正式创世身份。
pub const CITIZENCHAIN_FOUNDATION: ChinaCitizenChain = ChinaCitizenChain {
    cid_full_name: "公民链技术发展基金会",
    cid_short_name: "公民链基金会",
    full_name_en: "CitizenChain Technology Development Foundation",
    short_name_en: "CitizenChain Technology Foundation",
    cid_number: "GZ018-SFGYR-201206100-2026",
    main_account: hex!("aa23304c7b663ba25a9d3a2fb1efafdd650ecf2504a2caedc228fe81b46b4333"),
    fee_account: hex!("e4837d50cd5ef677c9b10ea49baf8c7d60cf11b422d748377e9e5f750640e927"),
};

/// 三个固定岗位各一席；任职人员可依法原子更换，岗位骨架本身永久不变。
pub const CITIZENCHAIN_FIXED_ROLES: &[CitizenChainFixedRole] = &[
    CitizenChainFixedRole {
        role_code: ROLE_CODE_LEGAL_REPRESENTATIVE,
        role_name: ROLE_NAME_LEGAL_REPRESENTATIVE,
        seats: 1,
    },
    CitizenChainFixedRole {
        role_code: ROLE_CODE_GENESIS_PRODUCT_MANAGER,
        role_name: ROLE_NAME_GENESIS_PRODUCT_MANAGER,
        seats: 1,
    },
    CitizenChainFixedRole {
        role_code: ROLE_CODE_GENESIS_PROGRAMMER,
        role_name: ROLE_NAME_GENESIS_PROGRAMMER,
        seats: 1,
    },
];

/// 该机构内部治理固定采用三个岗位任职票中的两票。
pub const CITIZENCHAIN_GOVERNANCE_THRESHOLD: u32 = 2;

/// 精确判断公民链技术发展基金会创世身份，禁止只按通用 `SFGY` 扩大保护范围。
pub fn is_citizenchain_foundation_identity(code: [u8; 4], cid_number: &[u8]) -> bool {
    code == *b"SFGY" && cid_number == CITIZENCHAIN_FOUNDATION.cid_number.as_bytes()
}

/// 查询该基金会受保护固定岗位；普通私权机构和自治岗位返回 `None`。
pub fn fixed_role(role_code: &[u8]) -> Option<CitizenChainFixedRole> {
    CITIZENCHAIN_FIXED_ROLES
        .iter()
        .copied()
        .find(|role| role.role_code == role_code)
}
