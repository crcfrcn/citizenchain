//! 原所属模块回归，断言与输入保持。
use super::*;
use chrono::NaiveDate;

fn normal_record() -> CitizenRecord {
    let now = DateTime::from_timestamp(1_779_580_800, 0).unwrap();
    CitizenRecord {
        id: 1,
        cid_number: format!(
            "CN000-CTZN{}-100000000-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
        ),
        passport_no: "GD12345678A".to_string(),
        family_name: "测".to_string(),
        given_name: "试公民".to_string(),
        citizen_sex: "FEMALE".to_string(),
        citizen_birth_date: "2000-01-01".to_string(),
        account_id: Some(format!("0x{}", "ab".repeat(32))),
        binding_revision: 1,
        binding_block_number: Some(7),
        binding_block_hash: Some(format!("0x{}", "11".repeat(32))),
        citizen_status: CitizenStatus::Normal,
        voting_eligible: true,
        passport_valid_from: "2026-05-24".to_string(),
        passport_valid_until: "2036-05-23".to_string(),
        status_updated_at: Some(1_779_580_800),
        province_code: "GD".to_string(),
        city_code: "001".to_string(),
        town_code: "001001".to_string(),
        birth_province_code: "GD".to_string(),
        birth_city_code: "001".to_string(),
        birth_town_code: "001001".to_string(),
        archive_hash: None,
        onchain_tx_hash: None,
        onchain_block_number: None,
        onchain_at: None,
        creator_account_id: format!("0x{}", "cd".repeat(32)),
        created_at: now,
        updater_account_id: None,
        updated_at: now,
    }
}

#[test]
fn citizen_record_computes_identity_status_from_status_and_validity() {
    let record = normal_record();

    assert_eq!(
        record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2026, 5, 24).unwrap()),
        CitizenStatus::Normal
    );
    assert_eq!(
        record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2036, 5, 23).unwrap()),
        CitizenStatus::Normal
    );
    assert_eq!(
        record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2036, 5, 24).unwrap()),
        CitizenStatus::Revoked
    );
    assert_eq!(record.computed_vote_status(), CitizenStatus::Normal);
}
