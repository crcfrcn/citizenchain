//! 私有岗位映射单元测试，由所属源码模块挂载。

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
