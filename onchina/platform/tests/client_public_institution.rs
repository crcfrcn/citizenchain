//! 原所属模块回归，断言与输入保持。
use super::*;

fn sample_row() -> PublicInstitutionRow {
    PublicInstitutionRow {
        cid_number: format!(
            "AH001-PGV0{}-123456789-2026",
            primitives::cid::number::checksum_char_mod36("AH001PGV01234567892026")
        ),
        cid_full_name: Some("安徽省人民政府".to_string()),
        cid_short_name: Some("皖府".to_string()),
        category: InstitutionCategory::GovInstitution,
        p1: "0".to_string(),
        province_code: "AH".to_string(),
        city_code: "001".to_string(),
        town_code: String::new(),
        institution_code: "PGV".to_string(),
        has_legal_personality: Some(true),
        legal_representative: Some(crate::institutions::subjects::model::LegalRepresentative {
            family_name: "张".to_string(),
            given_name: "三".to_string(),
            cid_number: format!(
                "CN000-CTZN{}-100000000-2026",
                primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
            ),
            account_id: format!("0x{}", "11".repeat(32)),
        }),
        parent_cid_number: None,
        account_count: 2,
        custom_account_names: vec!["业务专户A".to_string()],
        created_at: chrono::DateTime::from_timestamp(1_779_580_800, 0).unwrap(),
    }
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 隔离数据库后执行"]
fn public_catalog_reads_real_columns_and_keeps_published_contract() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let row = sample_row();
    let representative = row.legal_representative.as_ref().unwrap();
    let other = format!(
        "AH001-PGV0{}-123456790-2026",
        primitives::cid::number::checksum_char_mod36("AH001PGV01234567902026")
    );
    sandbox.db.with_client(|conn| {
            for cid in [&row.cid_number, &other] {
                conn.execute("INSERT INTO subject(cid_number,record_kind,province_code,city_code,subject_kind,profit_kind,institution_code,full_name,short_name,family_name,given_name,representative_cid,representative_account,created_at,updated_at) VALUES($1,'PUBLIC','AH','001','GOV_INSTITUTION','0','PGV',$2,$3,$4,$5,$6,$7,$8,now())",
                    &[cid,&row.cid_full_name,&row.cid_short_name,&representative.family_name,&representative.given_name,&representative.cid_number,&representative.account_id,&row.created_at]).map_err(|e| e.to_string())?;
                conn.execute("INSERT INTO government(cid_number,province_code,city_code,institution_code) VALUES($1,'AH','001','PGV')", &[cid]).map_err(|e| e.to_string())?;
            }
            conn.execute("INSERT INTO account(cid_number,province_code,city_code,account_name,created_at) VALUES($1,'AH','001','测试专户',now())", &[&row.cid_number]).map_err(|e| e.to_string())?;
            conn.execute("INSERT INTO projection(projection_key,genesis_hash,block_hash,block_number,item_count,account_count,state_code) VALUES('public-gov','synthetic-genesis','synthetic-block',1,2,1,'OK')", &[]).map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
    let state = AppState {
        db: sandbox.db.clone(),
        rate_limiter: std::sync::Arc::new(LocalRateLimiter::new()),
        modules: std::sync::Arc::new(crate::modules::ModuleCatalog::new(crate::local_module())),
    };
    let (items, more, next) =
        query_public_institutions(&state, "AH", Some("001"), None, None, 1).unwrap();
    assert_eq!(items.len(), 1);
    assert!(more);
    assert_eq!(next.as_deref(), Some(items[0].cid_number.as_str()));
    let json = serde_json::to_value(&items[0]).unwrap();
    assert_eq!(json["legal_representative"]["family_name"], "张");
    assert!(json.get("cidNumber").is_none());
    assert!(json.get("creator_account_id").is_none());
    let (last, more, next) =
        query_public_institutions(&state, "AH", Some("001"), next, None, 1).unwrap();
    assert_eq!(last.len(), 1);
    assert!(!more);
    assert!(next.is_none());
    let version = current_chain_projection_version(&state.db).unwrap();
    assert!(
        query_public_institutions(&state, "AH", None, None, Some(version), 1)
            .unwrap()
            .0
            .is_empty()
    );
    let names = custom_account_names_for(&state, "AH", &[row.cid_number.clone()]).unwrap();
    assert_eq!(names[&row.cid_number], vec!["测试专户"]);
    assert_eq!(
        scope_version_and_count(&state, "AH", Some("001"))
            .unwrap()
            .1,
        2
    );
    assert!(serde_json::from_value::<PublicInstitutionListQuery>(
        serde_json::json!({"province_name":"安徽省","page_size":1})
    )
    .is_ok());
    assert!(serde_json::from_value::<PublicInstitutionListQuery>(
        serde_json::json!({"provinceName":"安徽省"})
    )
    .is_err());
}

#[test]
fn public_dto_excludes_sensitive_admin_fields() {
    let json = serde_json::to_string(&sample_row()).expect("serialize public row");
    for forbidden in ["creator_account_id", "private_type", "partnership_kind"] {
        assert!(
            !json.contains(forbidden),
            "公开 DTO 泄露敏感字段: {forbidden}"
        );
    }
    assert!(json.contains("custom_account_names"));
    assert!(json.contains("业务专户A"));
    assert!(json.contains("安徽省人民政府"));
    // 法定代表人姓名属已确认可公开的目录字段,必须随公开行下发。
    assert!(json.contains("legal_representative"));
    assert!(json.contains("family_name"));
    assert!(json.contains("given_name"));
    assert!(json.contains("张"));
    assert!(json.contains("三"));
}

#[test]
fn public_dto_keeps_directory_fields() {
    let row = sample_row();
    assert!(crate::codes::validate_cid_number_format(&row.cid_number).is_ok());
    assert!(crate::codes::validate_cid_number_format(
        &row.legal_representative.as_ref().unwrap().cid_number
    )
    .is_ok());
    assert_eq!(row.account_count, 2);
    assert_eq!(row.institution_code, "PGV");
}

#[test]
fn citizen_public_filter_keeps_all_public_institutions() {
    assert!(GOV_FROM_WHERE.contains("s.subject_kind = 'GOV_INSTITUTION'"));
    assert!(GOV_FROM_WHERE.contains("s.institution_code IN ('SFGT', 'SFGP', 'UNIN')"));
    assert!(GOV_FROM_WHERE.contains("par.subject_kind = 'GOV_INSTITUTION'"));
    assert!(!GOV_FROM_WHERE.contains("CITY_POLICE"));
    // 公民端公权视图不排除教育机构(教育也在此展示)。
    assert!(!GOV_FROM_WHERE.contains("institution_code NOT IN ('NED'"));
}
