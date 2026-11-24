//! 验证创世固定岗位直接从法定席位枚举。

use super::*;
use primitives::{
    cid::code::{FRG, NJD, NRC, PRB, PRC},
    count_const::{
        FRG_COMMISSIONER_SEATS, NJD_JUDICIAL_SEATS, NRC_COMMITTEE_SEATS, PRB_DIRECTOR_SEATS,
        PRC_COMMITTEE_SEATS,
    },
    governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER,
    institution_constraints::{ROLE_CODE_DIRECTOR, ROLE_CODE_LEGAL_REPRESENTATIVE},
};

#[test]
fn fixed_positions_match_explicit_seat_counts() {
    for (institution, role, seats) in [
        (NRC, ROLE_CODE_COMMITTEE_MEMBER, NRC_COMMITTEE_SEATS),
        (PRC, ROLE_CODE_COMMITTEE_MEMBER, PRC_COMMITTEE_SEATS),
        (PRB, ROLE_CODE_DIRECTOR, PRB_DIRECTOR_SEATS),
    ] {
        let rows = fixed_seats(institution);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.as_slice(), role);
        assert_eq!(rows[0].2, seats);
    }
    let judicial = fixed_seats(NJD);
    assert_eq!(
        judicial.iter().map(|row| row.2).collect::<Vec<_>>(),
        vec![7, 1, 2, 5]
    );
    assert_eq!(
        judicial.iter().map(|row| row.2).sum::<u32>(),
        NJD_JUDICIAL_SEATS
    );
    assert!(judicial
        .iter()
        .all(|row| row.0.as_slice() != ROLE_CODE_LEGAL_REPRESENTATIVE));
}

#[test]
fn commissioner_positions_keep_each_province_code_and_capacity() {
    let rows = fixed_seats(FRG);
    assert_eq!(rows.len(), PROVINCE_CODE_INFOS.len());
    for (row, province) in rows.iter().zip(PROVINCE_CODE_INFOS) {
        assert_eq!(
            row.0,
            province_commissioner_role_code(province.province_code)
        );
        assert_eq!(
            row.1,
            province_commissioner_role_name(province.province_name)
        );
        assert_eq!(row.2, FRG_COMMISSIONER_SEATS);
    }
}
