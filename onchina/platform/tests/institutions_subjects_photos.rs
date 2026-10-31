//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
#[ignore = "由控制台交付本机 onchina_test 隔离数据库后执行"]
fn institution_photo_uses_one_record_and_checks_ownership() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let db = &sandbox.db;
    let actor = format!("0x{}", "11".repeat(32));
    let cid = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let mut inst: Institution = serde_json::from_value(serde_json::json!({
        "status":"ACTIVE", "subjectProperty":"G", "chainStatus":"ACTIVE_ON_CHAIN",
        "cidNumber":cid, "cidFullName":"测试机构", "cidShortName":"测试",
        "category":"GOV_INSTITUTION", "p1":"0", "provinceName":"贵州省",
        "cityName":"绥阳市", "provinceCode":"GZ", "cityCode":"018",
        "institutionCode":"CREG", "createdAt":chrono::Utc::now()
    }))
    .unwrap();
    db.upsert_institution_row(&inst).unwrap();
    let photo = db
        .insert_legal_representative_photo(
            "test.png".into(),
            "image/png".into(),
            b"synthetic-image".to_vec(),
            actor.clone(),
        )
        .unwrap();
    inst.photo_path = Some(photo.file_path.clone());
    inst.photo_name = Some("forged-name".into());
    inst.photo_mime = Some("image/jpeg".into());
    inst.photo_size = Some(999);
    assert!(db
        .save_institution_with_photo(inst.clone(), "other-owner".into())
        .unwrap()
        .is_none());
    let saved = db
        .save_institution_with_photo(inst, actor.clone())
        .unwrap()
        .unwrap();
    assert_eq!(saved.photo_name.as_deref(), Some("test.png"));
    assert_eq!(saved.photo_mime.as_deref(), Some("image/png"));
    assert_eq!(saved.photo_size, Some(15));
    let actual = db.get_institution_with_accounts(&cid).unwrap().unwrap().0;
    assert_eq!(actual.photo_path, saved.photo_path);
    assert_eq!(actual.photo_size, saved.photo_size);
    let replacement = db
        .insert_legal_representative_photo(
            "next.webp".into(),
            "image/webp".into(),
            vec![1, 2],
            actor.clone(),
        )
        .unwrap();
    let mut next = actual;
    next.photo_path = Some(replacement.file_path.clone());
    let saved = db
        .save_institution_with_photo(next, actor)
        .unwrap()
        .unwrap();
    assert_eq!(saved.photo_size, Some(2));
    assert!(db
        .read_legal_representative_photo(photo.file_path)
        .unwrap()
        .is_none());
    let stored = db
        .read_legal_representative_photo(replacement.file_path)
        .unwrap()
        .unwrap();
    assert_eq!(stored.institution_cid_number.as_deref(), Some(cid.as_str()));
    assert_eq!(stored.content, vec![1, 2]);
}

#[test]
fn photo_reference_must_be_a_database_record_url() {
    let id = "e46f0324-1275-42b4-916a-78824e66d320";
    assert!(valid_photo_path(&format!("{PHOTO_URL_PREFIX}{id}")));
    for path in [
        "data/legal-rep-photos/202607/photo.jpg",
        "/etc/passwd",
        "",
        PHOTO_URL_PREFIX,
    ] {
        assert!(!valid_photo_path(path));
    }
    assert!(!valid_photo_path(&format!(
        "{PHOTO_URL_PREFIX}{id}/../photo"
    )));
}
