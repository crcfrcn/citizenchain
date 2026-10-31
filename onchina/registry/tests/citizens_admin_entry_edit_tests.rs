//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::citizens::admin_entry::CitizenWrites;
use crate::citizens::chain_identity::IdentityOperations;

fn base_record() -> CitizenRecord {
    CitizenRecord {
        id: 1,
        cid_number: format!(
            "CN000-CTZN{}-100000000-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
        ),
        passport_no: String::new(),
        family_name: String::new(),
        given_name: String::new(),
        citizen_sex: String::new(),
        citizen_birth_date: String::new(),
        account_id: None,
        binding_revision: 0,
        binding_block_number: None,
        binding_block_hash: None,
        citizen_status: CitizenStatus::Normal,
        voting_eligible: false,
        passport_valid_from: String::new(),
        passport_valid_until: String::new(),
        status_updated_at: None,
        province_code: "GZ".to_string(),
        city_code: "018".to_string(),
        town_code: String::new(),
        birth_province_code: String::new(),
        birth_city_code: String::new(),
        birth_town_code: String::new(),
        archive_hash: None,
        onchain_tx_hash: None,
        onchain_block_number: None,
        onchain_at: None,
        creator_account_id: "0x".to_string() + &"9c".repeat(32),
        created_at: chrono::DateTime::from_timestamp(1_779_580_800, 0).unwrap(),
        updater_account_id: None,
        updated_at: chrono::DateTime::from_timestamp(1_779_580_800, 0).unwrap(),
    }
}

#[test]
fn citizen_outputs_only_expose_canonical_account_identity() {
    let mut record = base_record();
    let account_id = format!("0x{}", "11".repeat(32));
    record.account_id = Some(account_id.clone());
    for output in [
        serde_json::to_value(create_output_from_record(record.clone())).unwrap(),
        serde_json::to_value(crate::citizens::model::citizen_row_from_record(&record)).unwrap(),
    ] {
        assert_eq!(output["accountId"], account_id);
        assert!(output.get("ss58Address").is_none());
        assert!(output.get("ss58_address").is_none());
    }
    assert_eq!(
        resolve_citizen_account(&account_id).unwrap().account_id,
        account_id
    );
    for invalid in [
        "",
        "0x11",
        "11",
        "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        assert!(resolve_citizen_account(invalid).is_err());
    }
}

fn input() -> AdminEditCitizenInput {
    AdminEditCitizenInput {
        family_name: "程".to_string(),
        given_name: "伟".to_string(),
        city_code: "018".to_string(),
        // 唯一 china.sqlite 中 GZ/018 的真实三位镇码（坪乐镇）。
        town_code: "013".to_string(),
        voting_eligible: true,
        citizen_sex: "MALE".to_string(),
        citizen_birth_date: "1988-05-20".to_string(),
        birth_province_code: "GZ".to_string(),
        birth_city_code: "018".to_string(),
        birth_town_code: "013".to_string(),
    }
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn concurrent_edits_lock_identity_and_stale_projection_rolls_back_all_indexes() {
    let sandbox = onchina::database::IsolatedTestDb::new();
    let owner = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let ctx = onchina::authentication::login::AdminAuthContext {
        account_id: format!("0x{}", "9c".repeat(32)),
        institution_cid_number: owner.clone(),
        institution_code: "CREG".into(),
        admin_level: Some("CITY".into()),
        family_name: "测".into(),
        given_name: "试".into(),
        scope_province_name: Some("贵州省".into()),
        scope_city_name: Some("绥阳市".into()),
        scope_town_name: None,
        cid_short_name: None,
    };
    let record = base_record();
    let binding = onchina::authentication::login::NodeInstitutionBinding {
        binding_id: "edit-binding".into(),
        candidate_id: "edit-candidate".into(),
        institution_code: "CREG".into(),
        institution_cid_number: owner,
        bound_account_id: ctx.account_id.clone(),
        bound_at: record.created_at,
        status: "ACTIVE".into(),
    };
    sandbox
        .db
        .with_client(|conn| {
            onchina::authentication::repo::upsert_active_node_binding_conn(conn, &binding)
        })
        .unwrap();
    sandbox.db.upsert_citizen_row(&record).unwrap();
    let handles = ["1988-05-20", "1989-05-20"]
        .into_iter()
        .map(|birth| {
            let db = sandbox.db.clone();
            let ctx = ctx.clone();
            let cid = record.cid_number.clone();
            std::thread::spawn(move || {
                let mut edit = input();
                edit.citizen_birth_date = birth.into();
                db.with_client(|conn| {
                    apply_citizen_edit_conn(conn, &ctx, "edit-binding", &cid, &edit)
                })
                .unwrap()
                .is_ok()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|success| *success)
            .count(),
        1
    );
    let saved = sandbox
        .db
        .find_citizen_by_cid(&record.cid_number)
        .unwrap()
        .unwrap();
    assert!(!saved.passport_no.is_empty());
    assert!(saved.id > 0);
    let mut current = saved.clone();
    current.binding_revision = 2;
    current.account_id = Some(format!("0x{}", "33".repeat(32)));
    sandbox.db.upsert_citizen_row(&current).unwrap();
    let mut stale = saved;
    stale.province_code = "GD".into();
    stale.city_code = "001".into();
    assert!(sandbox.db.upsert_citizen_row(&stale).is_err());
    let reread = sandbox
        .db
        .find_citizen_by_cid(&record.cid_number)
        .unwrap()
        .unwrap();
    assert_eq!(reread.province_code, "GZ");
    assert_eq!(reread.binding_revision, 2);
    sandbox
        .db
        .with_client(|conn| {
            let province: String = conn
                .query_one(
                    "SELECT province_code FROM identity WHERE cid_number=$1",
                    &[&record.cid_number],
                )
                .unwrap()
                .get(0);
            assert_eq!(province, "GZ");
            let passports: i64 = conn
                .query_one(
                    "SELECT COUNT(*) FROM passport WHERE cid_number=$1",
                    &[&record.cid_number],
                )
                .unwrap()
                .get(0);
            assert_eq!(passports, 1);
            Ok(())
        })
        .unwrap();
    sandbox
        .db
        .with_client(|conn| {
            onchina::authentication::repo::deactivate_active_node_binding_conn(conn).map(|_| ())
        })
        .unwrap();
    assert!(sandbox
        .db
        .with_client(|conn| apply_citizen_edit_conn(
            conn,
            &ctx,
            "edit-binding",
            &record.cid_number,
            &input()
        ))
        .unwrap()
        .is_err());
}

#[test]
fn empty_immutable_fields_are_initialized() {
    let locked = resolve_locked_identity(&base_record(), &input()).expect("ok");
    assert_eq!(locked.citizen_sex, "MALE");
    assert_eq!(locked.citizen_birth_date, "1988-05-20");
    assert_eq!(locked.birth_town_code, "013");
}

#[test]
fn immutable_birth_date_cannot_change() {
    let mut existing = base_record();
    existing.citizen_birth_date = "1988-05-20".to_string(); // 已初始化
    let mut edit = input();
    edit.citizen_birth_date = "1990-01-01".to_string(); // 试图改出生日期
    let err = resolve_locked_identity(&existing, &edit).unwrap_err();
    assert!(err.contains("出生日期"));
}

#[test]
fn name_is_mutable_not_locked() {
    // 姓名可变:即便现存已有姓名,改名也不应触发锁定拒绝(姓名不进 LockedIdentity)。
    let mut existing = base_record();
    existing.citizen_sex = "MALE".to_string();
    let mut edit = input();
    edit.family_name = "王".to_string(); // 改姓
    edit.given_name = "五".to_string(); // 改名
    edit.citizen_sex = "MALE".to_string(); // 不可变一致 → 保持
    let locked = resolve_locked_identity(&existing, &edit).expect("ok");
    assert_eq!(locked.citizen_sex, "MALE"); // 不可变字段仍锁定
                                            // 姓名的实际采用在 handler 直接取 input,不受 lock 影响。
}

#[test]
fn immutable_field_kept_when_input_matches_or_empty() {
    let mut existing = base_record();
    existing.citizen_birth_date = "1988-05-20".to_string();
    existing.citizen_sex = "MALE".to_string();
    let mut edit = input();
    edit.citizen_birth_date = String::new(); // 入参空 → 保持
    edit.citizen_sex = "MALE".to_string(); // 一致 → 保持
    let locked = resolve_locked_identity(&existing, &edit).expect("ok");
    assert_eq!(locked.citizen_birth_date, "1988-05-20");
    assert_eq!(locked.citizen_sex, "MALE");
}

#[test]
fn voting_eligible_requires_birth_date() {
    let existing = base_record(); // 出生日期空
    let mut edit = input();
    edit.citizen_birth_date = String::new();
    // 隔离出生日期校验:清空出生地三段与居住市镇,避免行政区校验先行拦截。
    edit.birth_province_code = String::new();
    edit.birth_city_code = String::new();
    edit.birth_town_code = String::new();
    edit.voting_eligible = true;
    let locked = resolve_locked_identity(&existing, &edit).expect("lock ok");
    let err = validate_locked_identity(&existing, &locked, true, "", "").unwrap_err();
    assert!(err.contains("出生日期"));
}

#[test]
fn illegal_sex_is_rejected() {
    let existing = base_record();
    let mut edit = input();
    edit.citizen_sex = "X".to_string();
    let locked = resolve_locked_identity(&existing, &edit).expect("lock ok");
    let err = validate_locked_identity(&existing, &locked, false, "", "").unwrap_err();
    assert!(err.contains("性别"));
}
