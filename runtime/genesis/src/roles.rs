//! 由治理规格枚举固定岗位及席位。

extern crate alloc;

use alloc::vec::Vec;
use primitives::{
    cid::code::{InstitutionCode, FRG, PROVINCE_CODE_INFOS},
    count_const::FRG_COMMISSIONER_SEATS,
    governance_skeleton::{
        fixed_role_specs, province_commissioner_role_code, province_commissioner_role_name,
    },
    institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
};

pub(super) type GenesisRole = (Vec<u8>, Vec<u8>, u32);

/// 法定代表人的默认岗位由实体建档时写入，固定治理初始化枚举其余席位。
pub(super) fn fixed_seats(code: InstitutionCode) -> Vec<GenesisRole> {
    if code == FRG {
        return PROVINCE_CODE_INFOS
            .iter()
            .map(|province| {
                (
                    province_commissioner_role_code(province.province_code),
                    province_commissioner_role_name(province.province_name),
                    FRG_COMMISSIONER_SEATS,
                )
            })
            .collect();
    }
    fixed_role_specs(code)
        .into_iter()
        .filter(|role| role.role_code != ROLE_CODE_LEGAL_REPRESENTATIVE)
        .map(|role| (role.role_code.to_vec(), role.role_name.to_vec(), role.seats))
        .collect()
}

#[cfg(test)]
#[path = "../tests/roles.rs"]
mod tests;
