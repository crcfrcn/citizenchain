//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::citizens::chain_identity::IdentityOperations;

fn scope() -> NodeScope {
    NodeScope {
        province_code: "GZ".to_string(),
        city_code: "018".to_string(),
    }
}

fn chain_voting_citizen() -> ChainCitizen {
    ChainCitizen {
        cid_number: format!(
            "CN000-CTZN{}-100000000-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
        ),
        province_code: "GZ".to_string(),
        city_code: "018".to_string(),
        town_code: "018001".to_string(),
        account_id: Some("0x".to_string() + &"11".repeat(32)),
        binding_revision: 1,
        binding_block_number: 7,
        binding_block_hash: "0x".to_string() + &"22".repeat(32),
        status_normal: true,
        passport_valid_from: Some("2026-01-01".to_string()),
        passport_valid_until: Some("2036-01-01".to_string()),
        // 投票身份链上无姓名:
        family_name: None,
        given_name: None,
        citizen_sex: None,
        birth_date: None,
        birth_province_code: None,
        birth_city_code: None,
        birth_town_code: None,
    }
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_782_950_400, 0).expect("valid ts")
}

#[test]
fn existing_citizen_move_out_refreshes_scope_and_preserves_originals() {
    let chain = chain_voting_citizen();
    let mut existing = merge_citizen_record(&chain, None, &scope(), now()).unwrap();
    existing.id = 17;
    existing.family_name = "保留".into();
    existing.archive_hash = Some("original".into());
    existing.passport_no = "original-passport".into();
    let mut moved = chain.clone();
    moved.province_code = "GD".into();
    moved.city_code = "001".into();
    moved.binding_block_number += 1;
    assert!(merge_citizen_record(&moved, None, &scope(), now()).is_none());
    let refreshed = merge_citizen_record(&moved, Some(&existing), &scope(), now()).unwrap();
    assert_eq!(refreshed.province_code, "GD");
    assert_eq!(refreshed.city_code, "001");
    assert_eq!(refreshed.id, 17);
    assert_eq!(refreshed.family_name, "保留");
    assert_eq!(refreshed.archive_hash, existing.archive_hash);
    assert_eq!(refreshed.passport_no, existing.passport_no);
    assert!(merge_citizen_record(
        &moved,
        None,
        &NodeScope {
            province_code: "GD".into(),
            city_code: "001".into()
        },
        now()
    )
    .is_some());
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn entity_failure_rolls_back_rows_and_cursor_and_old_binding_cannot_write() {
    use onchina::authentication::{login::NodeInstitutionBinding, repo};
    let sandbox = onchina::database::IsolatedTestDb::new();
    let institution = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let binding = NodeInstitutionBinding {
        binding_id: "binding-a".into(),
        candidate_id: "candidate-a".into(),
        institution_code: "CREG".into(),
        institution_cid_number: institution,
        bound_account_id: format!("0x{}", "11".repeat(32)),
        bound_at: now(),
        status: "ACTIVE".into(),
    };
    sandbox
        .db
        .with_client(|conn| repo::upsert_active_node_binding_conn(conn, &binding))
        .unwrap();
    let binding = projection_binding(&sandbox.db).unwrap().unwrap();
    let hash = format!("0x{}", "22".repeat(32));
    let chain = chain_voting_citizen();
    let batch = ProjectionBatch {
        citizens: vec![chain.clone()],
        institutions: vec![],
    };
    assert!(
        commit_projection_batch(&sandbox.db, &binding, "chain-a", None, 7, &hash, &batch).unwrap()
    );
    let original = sandbox
        .db
        .find_citizen_by_cid(&chain.cid_number)
        .unwrap()
        .unwrap();
    assert!(original.id > 0 && original.id <= i64::MAX as u64);
    sandbox.db.with_client(|conn| { conn.execute("UPDATE citizen SET family_name='保留',archive_hash='local-original' WHERE cid_number=$1", &[&chain.cid_number]).unwrap(); Ok(()) }).unwrap();
    let mut first = chain.clone();
    first.cid_number = format!(
        "CN000-CTZN{}-100000001-2026",
        primitives::cid::number::checksum_char_m1("CN000CTZN1000000012026", true)
    );
    first.account_id = Some(format!("0x{}", "33".repeat(32)));
    first.binding_block_number = 8;
    let mut bad = chain.clone();
    bad.cid_number = format!(
        "CN000-CTZN{}-100000002-2026",
        primitives::cid::number::checksum_char_m1("CN000CTZN1000000022026", true)
    );
    bad.account_id = Some(format!("0x{}", "44".repeat(32)));
    bad.binding_block_number = 8;
    bad.binding_revision = u64::MAX;
    let failed = ProjectionBatch {
        citizens: vec![first.clone(), bad],
        institutions: vec![],
    };
    assert!(
        commit_projection_batch(&sandbox.db, &binding, "chain-a", Some(7), 8, &hash, &failed)
            .is_err()
    );
    assert!(sandbox
        .db
        .find_citizen_by_cid(&first.cid_number)
        .unwrap()
        .is_none());
    assert_eq!(
        projection_cursor(&sandbox.db, &binding.key("chain-a"), "chain-a")
            .unwrap()
            .unwrap()
            .0,
        7
    );
    assert!(
        commit_projection_batch(&sandbox.db, &binding, "chain-a", Some(7), 8, &hash, &batch)
            .unwrap()
    );
    let reread = sandbox
        .db
        .find_citizen_by_cid(&chain.cid_number)
        .unwrap()
        .unwrap();
    assert_eq!(reread.id, original.id);
    assert_eq!(reread.family_name, "保留");
    assert_eq!(reread.archive_hash.as_deref(), Some("local-original"));
    assert!(
        commit_projection_batch(&sandbox.db, &binding, "chain-a", Some(7), 8, &hash, &batch)
            .is_err()
    );
    sandbox
        .db
        .with_client(|conn| repo::deactivate_active_node_binding_conn(conn).map(|_| ()))
        .unwrap();
    assert!(
        !commit_projection_batch(&sandbox.db, &binding, "chain-a", Some(8), 9, &hash, &batch)
            .unwrap()
    );
    assert_eq!(
        projection_cursor(&sandbox.db, &binding.key("chain-a"), "chain-a")
            .unwrap()
            .unwrap()
            .0,
        8
    );
}

#[test]
fn projection_cursor_requires_contiguous_success_and_is_scoped() {
    assert!(valid_projection_advance(None, 100));
    assert!(valid_projection_advance(Some(100), 101));
    for next in [-1, 99, 100, 102] {
        assert!(!valid_projection_advance(Some(100), next));
    }
    assert!(!valid_projection_advance(Some(i64::MAX), i64::MAX));
    let binding = ProjectionBinding {
        binding_id: "binding-a".into(),
        institution_cid_number: format!(
            "GZ018-CREG{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
        ),
        scope: scope(),
    };
    assert_ne!(binding.key("chain-a"), binding.key("chain-b"));
    let other = ProjectionBinding {
        institution_cid_number: "another".into(),
        ..binding.clone()
    };
    assert_ne!(binding.key("chain-a"), other.key("chain-a"));
}

#[test]
fn chain_dates_validate_calendar_and_keep_missing_values() {
    assert_eq!(chain_date(None).unwrap(), None);
    assert_eq!(
        chain_date(Some(20240229)).unwrap().as_deref(),
        Some("2024-02-29")
    );
    for value in [0, 20230229, 20261301, 20260100, 20260431] {
        assert!(chain_date(Some(value)).is_err());
    }
    let row = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
    assert_eq!(
        row.computed_identity_status_on_date(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        CitizenStatus::Normal
    );
    assert_eq!(
        row.computed_identity_status_on_date(chrono::NaiveDate::from_ymd_opt(2036, 1, 2).unwrap()),
        CitizenStatus::Revoked
    );
    assert_eq!(row.id, 0); // 新记录的真实 ID 只由数据库正式写入分配。
}

#[test]
fn out_of_scope_city_is_skipped() {
    let mut chain = chain_voting_citizen();
    chain.city_code = "999".to_string();
    assert!(merge_citizen_record(&chain, None, &scope(), now()).is_none());
}

#[test]
fn out_of_scope_province_is_skipped() {
    let mut chain = chain_voting_citizen();
    chain.province_code = "LN".to_string();
    assert!(merge_citizen_record(&chain, None, &scope(), now()).is_none());
}

#[test]
fn r5_province_city_reads_institution_scope() {
    // 机构 CID:R5 = 省2 + 市3。
    assert_eq!(
        r5_province_city("GZ018-SFGYR-201206100-2026").expect("institution scope"),
        ("GZ".to_string(), "018".to_string())
    );
}

#[test]
fn r5_province_city_rejects_person_cid() {
    // 人主体 CN 号去地域化、R5 不载省市 → fail-closed,不把号段误读成区划码。
    assert!(r5_province_city("CN220-CTZN2-198805200-2026").is_err());
}

#[test]
fn fresh_projection_maps_chain_fields_and_blank_offchain() {
    let out =
        merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).expect("in scope");
    assert_eq!(out.city_code, "018");
    assert_eq!(out.town_code, "018001");
    assert_eq!(out.passport_valid_from, "2026-01-01");
    assert_eq!(out.citizen_status, CitizenStatus::Normal);
    assert!(out.voting_eligible);
    // 投票身份链上无姓名 → 空;链下正本 passport_no 空。
    assert_eq!(out.family_name, "");
    assert_eq!(out.passport_no, "");
    // 创建人回落到公民自身账户(来源锚点)。
    assert_eq!(out.creator_account_id, "0x".to_string() + &"11".repeat(32));
}

#[test]
fn reprojection_preserves_offchain_original_fields() {
    // 本地已有一条注册局补录过链下正本的行(护照号 + 姓名 + 档案哈希 + 创建人)。
    let existing = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now())
        .map(|mut row| {
            row.passport_no = "P12345678".to_string(); // 链下正本
            row.family_name = "王".to_string(); // 注册局补录姓名(投票身份链上仍无)
            row.given_name = "五".to_string();
            row.archive_hash = Some("hash-abc".to_string());
            row.creator_account_id = "0x".to_string() + &"ee".repeat(32);
            row.created_at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
            row
        })
        .unwrap();

    // 链上后来更新了居住地/护照(仍是投票身份,链上无姓名)。
    let mut chain = chain_voting_citizen();
    chain.town_code = "018002".to_string();
    chain.passport_valid_until = Some("2040-01-01".to_string());

    let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).expect("in scope");
    // 链上来源列被更新:
    assert_eq!(out.town_code, "018002");
    assert_eq!(out.passport_valid_until, "2040-01-01");
    // 链下正本绝不被覆盖:
    assert_eq!(out.passport_no, "P12345678");
    assert_eq!(out.archive_hash.as_deref(), Some("hash-abc"));
    assert_eq!(out.creator_account_id, "0x".to_string() + &"ee".repeat(32));
    assert_eq!(out.created_at, existing.created_at);
    // 投票身份链上无姓名 → 保留注册局补录的姓名,不被链上空值抹掉。
    assert_eq!(out.family_name, "王");
    assert_eq!(out.given_name, "五");
}

#[test]
fn candidate_chain_name_overrides_local() {
    // 已有本地行(姓名空);链上升为竞选身份带姓名 → 用链上姓名。
    let existing = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
    let mut chain = chain_voting_citizen();
    chain.family_name = Some("李".to_string());
    chain.given_name = Some("四".to_string());
    chain.citizen_sex = Some("MALE".to_string());
    chain.birth_date = Some("2000-01-01".to_string());

    let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).unwrap();
    assert_eq!(out.family_name, "李");
    assert_eq!(out.given_name, "四");
    assert_eq!(out.citizen_sex, "MALE");
    // 链上日期已映射为标准日历格式，投影保持该格式。
    assert_eq!(out.citizen_birth_date, "2000-01-01");
}

#[test]
fn revoked_status_projects_and_clears_voting_eligible() {
    let mut chain = chain_voting_citizen();
    chain.status_normal = false;
    let out = merge_citizen_record(&chain, None, &scope(), now()).unwrap();
    assert_eq!(out.citizen_status, CitizenStatus::Revoked);
    assert!(!out.voting_eligible);
}

#[test]
fn finalized_missing_account_never_falls_back_to_local_account() {
    let existing = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
    let mut chain = chain_voting_citizen();
    chain.binding_revision = 2;
    chain.account_id = None;
    chain.status_normal = false;
    let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).unwrap();
    assert_eq!(out.account_id, None);
    assert_eq!(out.binding_revision, 2);
}

#[test]
fn projection_rejects_revision_regression_and_same_revision_account_change() {
    let mut existing =
        merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
    existing.binding_revision = 3;

    let mut regressed = chain_voting_citizen();
    regressed.binding_revision = 2;
    assert!(ensure_binding_projection_monotonic(&regressed, Some(&existing)).is_err());

    let mut conflicting = chain_voting_citizen();
    conflicting.binding_revision = 3;
    conflicting.account_id = Some("0x".to_string() + &"22".repeat(32));
    assert!(ensure_binding_projection_monotonic(&conflicting, Some(&existing)).is_err());

    let mut advanced = conflicting;
    advanced.binding_revision = 4;
    assert!(ensure_binding_projection_monotonic(&advanced, Some(&existing)).is_ok());
}

fn chain_institution() -> ChainInstitution {
    ChainInstitution {
        cid_number: "GZ018-SFGYR-201206100-2026".to_string(),
        province_code: "GZ".to_string(),
        city_code: "018".to_string(),
        town_code: String::new(),
        cid_full_name: "公民链技术发展基金会".to_string(),
        cid_short_name: "公民链基金会".to_string(),
        institution_code: "SFGY".to_string(),
        profit: false,
        is_private: true,
        legal_representative: Some(LegalRepresentative {
            family_name: "程".to_string(),
            given_name: "伟".to_string(),
            cid_number: "CN220-CTZN2-198805200-2026".to_string(),
            account_id: "0x".to_string() + &"9c".repeat(32),
        }),
    }
}

#[test]
fn institution_out_of_scope_is_skipped() {
    let mut chain = chain_institution();
    chain.city_code = "999".to_string();
    assert!(merge_institution_record(&chain, None, &scope(), now()).is_none());
}

#[test]
fn institution_fresh_projection_maps_chain_fields() {
    let out =
        merge_institution_record(&chain_institution(), None, &scope(), now()).expect("in scope");
    let dto = serde_json::to_value(&out).unwrap();
    assert_eq!(dto["status"], "ACTIVE");
    assert_eq!(dto["chainStatus"], "ACTIVE_ON_CHAIN");
    assert_eq!(dto["subjectProperty"], "S");
    assert_eq!(out.category, InstitutionCategory::PrivateInstitution);
    assert_eq!(out.cid_short_name.as_deref(), Some("公民链基金会"));
    assert_eq!(out.institution_code, "SFGY");
    assert_eq!(out.has_legal_personality, Some(true));
    assert_eq!(
        out.legal_representative
            .as_ref()
            .map(|l| l.given_name.as_str()),
        Some("伟")
    );
    // 链下证件照:新建为空。
    assert!(out.photo_path.is_none());
    // private_type 无既有行时按机构码确定性派生(SFGY → WELFARE),否则私权列表 SQL
    // 的 `private_type IS NOT NULL` 会漏掉纯链上投影的私权机构。
    assert_eq!(out.private_type.as_deref(), Some("WELFARE"));
}

#[test]
fn institution_reprojection_preserves_photo_and_local_classification() {
    // 市已补录证件照 + 私权业务分类 + 创建人。
    let existing = merge_institution_record(&chain_institution(), None, &scope(), now())
        .map(|mut inst| {
            inst.photo_path = Some("/photos/foundation.jpg".to_string());
            inst.photo_size = Some(20480);
            inst.private_type = Some("FOUNDATION".to_string());
            inst.creator_account_id = Some("0x".to_string() + &"ee".repeat(32));
            inst.created_at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
            inst
        })
        .unwrap();

    // 链上后来改了简称(全称/简称是链上来源)。
    let mut chain = chain_institution();
    chain.cid_short_name = "基金会新简称".to_string();

    let out = merge_institution_record(&chain, Some(&existing), &scope(), now()).expect("in scope");
    // 链上来源列被更新:
    assert_eq!(out.cid_short_name.as_deref(), Some("基金会新简称"));
    // 链下正本绝不被覆盖:
    assert_eq!(out.photo_path.as_deref(), Some("/photos/foundation.jpg"));
    assert_eq!(out.photo_size, Some(20480));
    assert_eq!(out.private_type.as_deref(), Some("FOUNDATION"));
    assert_eq!(
        out.creator_account_id.as_deref(),
        Some(("0x".to_string() + &"ee".repeat(32)).as_str())
    );
    assert_eq!(out.created_at, existing.created_at);
}
