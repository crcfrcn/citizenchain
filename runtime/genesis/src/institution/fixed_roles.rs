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
mod tests {
    use super::*;
    use primitives::{
        count_const::{NJD_ADMIN_COUNT, NRC_ADMIN_COUNT, PRB_ADMIN_COUNT, PRC_ADMIN_COUNT},
        governance_skeleton::{
            ROLE_CODE_CHIEF_JUSTICE, ROLE_CODE_COMMITTEE_MEMBER, ROLE_CODE_CONSTITUTION_GUARD,
            ROLE_CODE_DEPUTY_CHIEF_JUSTICE, ROLE_CODE_DIRECTOR, ROLE_CODE_JUSTICE,
            ROLE_CODE_PROVINCE_COMMISSIONER_PREFIX,
        },
    };

    fn role_code(code: InstitutionCode, index: usize) -> Vec<u8> {
        role_for_fixed_admin(code, index).0
    }

    #[test]
    fn fixed_account_id_counts_equal_seat_totals() {
        assert_eq!(expected_admin_count(NRC), NRC_ADMIN_COUNT as usize);
        assert_eq!(expected_admin_count(PRC), PRC_ADMIN_COUNT as usize);
        assert_eq!(expected_admin_count(PRB), PRB_ADMIN_COUNT as usize);
        assert_eq!(expected_admin_count(NJD), NJD_ADMIN_COUNT as usize);
        assert_eq!(
            expected_admin_count(FRG),
            PROVINCE_CODE_INFOS.len() * FRG_PROVINCE_GROUP_ADMIN_COUNT as usize
        );
    }

    #[test]
    fn committee_and_bank_account_ids_map_to_single_role() {
        assert_eq!(role_code(NRC, 0), ROLE_CODE_COMMITTEE_MEMBER);
        assert_eq!(
            role_code(NRC, NRC_ADMIN_COUNT as usize - 1),
            ROLE_CODE_COMMITTEE_MEMBER
        );
        assert_eq!(role_code(PRC, 0), ROLE_CODE_COMMITTEE_MEMBER);
        assert_eq!(role_code(PRB, 0), ROLE_CODE_DIRECTOR);
        assert_eq!(
            role_code(PRB, PRB_ADMIN_COUNT as usize - 1),
            ROLE_CODE_DIRECTOR
        );
    }

    #[test]
    fn judicial_account_id_order_maps_to_seven_one_two_five() {
        assert_eq!(role_code(NJD, 0), ROLE_CODE_CONSTITUTION_GUARD);
        assert_eq!(role_code(NJD, 6), ROLE_CODE_CONSTITUTION_GUARD);
        assert_eq!(role_code(NJD, 7), ROLE_CODE_CHIEF_JUSTICE);
        assert_eq!(role_code(NJD, 8), ROLE_CODE_DEPUTY_CHIEF_JUSTICE);
        assert_eq!(role_code(NJD, 9), ROLE_CODE_DEPUTY_CHIEF_JUSTICE);
        assert_eq!(role_code(NJD, 10), ROLE_CODE_JUSTICE);
        assert_eq!(role_code(NJD, 14), ROLE_CODE_JUSTICE);
    }

    #[test]
    fn federal_registry_maps_five_account_ids_per_province_role() {
        let group_size = FRG_PROVINCE_GROUP_ADMIN_COUNT as usize;
        for (province_index, province) in PROVINCE_CODE_INFOS.iter().enumerate() {
            let first = province_index * group_size;
            let expected = province_commissioner_role_code(province.province_code);
            for offset in 0..group_size {
                let code = role_code(FRG, first + offset);
                assert_eq!(code, expected);
                assert!(code.starts_with(ROLE_CODE_PROVINCE_COMMISSIONER_PREFIX));
            }
        }
    }
}
