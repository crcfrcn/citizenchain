//! 89 个受保护创世机构的岗位、席位与账户索引映射。
//!
//! 本模块不写 storage，也不创建账户。岗位协议常量来自
//! `primitives::governance_skeleton`，账户来自既有 `CHINA_*` 常量；这里仅负责在
//! 创世阶段把第 `index` 个既有管理员账户确定性映射到固定岗位。

extern crate alloc;

use alloc::vec::Vec;
use primitives::{
    cid::code::{InstitutionCode, FRG, NJD, NRC, PRB, PRC, PROVINCE_CODE_INFOS},
    count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT,
    governance_skeleton::{
        fixed_role_specs, province_commissioner_role_code, province_commissioner_role_name,
    },
};

pub(super) type GenesisRole = (Vec<u8>, Vec<u8>);

/// 固定机构全部岗位席位的总数。
pub(super) fn expected_admin_count(code: InstitutionCode) -> usize {
    match code {
        NRC | PRC | PRB | NJD => fixed_role_specs(code)
            .iter()
            .map(|role| role.seats as usize)
            .sum(),
        FRG => PROVINCE_CODE_INFOS.len() * FRG_PROVINCE_GROUP_ADMIN_COUNT as usize,
        _ => panic!("genesis institution: 非固定治理机构不能建立创世岗位映射"),
    }
}

/// 断言现有账户常量数量与固定席位总数完全一致。
pub(super) fn assert_fixed_admin_count(code: InstitutionCode, actual: usize) {
    let expected = expected_admin_count(code);
    assert_eq!(
        actual, expected,
        "genesis institution: 固定机构管理员账户数量与岗位席位不一致"
    );
}

/// 将固定机构第 `index` 个既有管理员账户映射到创世岗位。
pub(super) fn role_for_fixed_admin(code: InstitutionCode, index: usize) -> GenesisRole {
    let expected = expected_admin_count(code);
    assert!(
        index < expected,
        "genesis institution: 管理员账户索引超出固定岗位席位"
    );

    if code == FRG {
        let group_size = FRG_PROVINCE_GROUP_ADMIN_COUNT as usize;
        let province = &PROVINCE_CODE_INFOS[index / group_size];
        return (
            province_commissioner_role_code(province.province_code),
            province_commissioner_role_name(province.province_name),
        );
    }

    let mut offset = 0usize;
    for spec in fixed_role_specs(code) {
        let end = offset + spec.seats as usize;
        if index < end {
            return (spec.role_code.to_vec(), spec.role_name.to_vec());
        }
        offset = end;
    }
    panic!("genesis institution: 固定岗位映射缺失")
}

#[cfg(test)]
#[path = "../tests/roles.rs"]
mod tests;
