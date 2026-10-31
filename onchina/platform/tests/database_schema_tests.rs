//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
#[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
fn current_schema_is_exact_and_reopens_without_schema_writes() {
    let sandbox = IsolatedTestDb::new();
    sandbox.db.with_client(|conn| {
            assert!(Db::schema_ready(conn)?);
            // 只读事务内再次初始化必须只核验，不能执行CREATE或COMMENT。
            conn.batch_execute("BEGIN READ ONLY").map_err(|e| e.to_string())?;
            Db::init_current_schema(conn)?;
            conn.batch_execute("ROLLBACK").map_err(|e| e.to_string())?;
            // 直接核对PostgreSQL目录，覆盖逻辑表、列、显式索引和全部省分区。
            let counts = conn.query_one(
                "SELECT
                    (SELECT count(*) FROM pg_class c WHERE c.relnamespace=current_schema()::regnamespace AND c.relkind IN ('r','p') AND NOT c.relispartition),
                    (SELECT count(*) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relnamespace=current_schema()::regnamespace AND c.relkind IN ('r','p') AND NOT c.relispartition AND a.attnum>0 AND NOT a.attisdropped),
                    (SELECT count(*) FROM pg_index i JOIN pg_class t ON t.oid=i.indrelid JOIN pg_class x ON x.oid=i.indexrelid WHERE t.relnamespace=current_schema()::regnamespace AND NOT t.relispartition AND NOT EXISTS(SELECT 1 FROM pg_constraint k WHERE k.conindid=x.oid)),
                    (SELECT count(*) FROM pg_class c WHERE c.relnamespace=current_schema()::regnamespace AND c.relkind='p'),
                    (SELECT count(*) FROM pg_class c WHERE c.relnamespace=current_schema()::regnamespace AND c.relispartition AND c.relkind='r')",
                &[],
            ).map_err(|e| e.to_string())?;
            for (index,expected) in [27_i64,205,54,7,301].into_iter().enumerate() {
                assert_eq!(counts.get::<_,i64>(index),expected);
            }
            for table in ["member","replay","request","nonce","private"] {
                let count:i64=conn.query_one("SELECT count(*) FROM pg_class WHERE relnamespace=current_schema()::regnamespace AND relname=$1", &[&table]).map_err(|e| e.to_string())?.get(0);
                assert_eq!(count,0,"退役表不得重建: {table}");
            }
            let old_columns:i64=conn.query_one("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND ((table_name='subject' AND column_name IN ('issuer_cid','proposal_id')) OR (table_name='government' AND column_name IN ('home_province','home_city')) OR (table_name='sequence' AND column_name='next_seq'))", &[]).map_err(|e| e.to_string())?.get(0);
            assert_eq!(old_columns,0);
            for (table,expected) in [("challenge",1_i64),("grant",1),("login",1),("enrollment",1),("session",2),("result",2)] {
                let count:i64=conn.query_one("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name=$1", &[&table]).map_err(|e| e.to_string())?.get(0);
                assert_eq!(count,expected,"{table}");
            }
            Ok(())
        }).unwrap();
}

#[test]
#[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
fn legacy_and_damaged_schema_are_rejected_without_repair_or_data_loss() {
    let sandbox = IsolatedTestDb::new();
    sandbox.db.with_client(|conn| {
            conn.batch_execute("CREATE TABLE admin_sessions(legacy_value TEXT); INSERT INTO admin_sessions VALUES('synthetic retained record')").map_err(|e| e.to_string())?;
            assert!(Db::init_current_schema(conn).is_err());
            let value:String=conn.query_one("SELECT legacy_value FROM admin_sessions", &[]).map_err(|e| e.to_string())?.get(0);
            assert_eq!(value,"synthetic retained record");
            conn.batch_execute("DROP TABLE admin_sessions; ALTER TABLE subject RENAME COLUMN full_name TO legacy").map_err(|e| e.to_string())?;
            assert!(Db::init_current_schema(conn).is_err());
            let count:i64=conn.query_one("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='subject' AND column_name='legacy'", &[]).map_err(|e| e.to_string())?.get(0);
            assert_eq!(count,1);
            Ok(())
        }).unwrap();
}

#[test]
#[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
fn old_format_is_rejected_without_changing_counter_or_schema() {
    let sandbox = IsolatedTestDb::new();
    sandbox
        .db
        .with_client(|conn| {
            conn.execute(
                "INSERT INTO sequence(seq_key,last_value) VALUES('synthetic',9)",
                &[],
            )
            .map_err(|e| e.to_string())?;
            let comment: String = conn
                .query_one(
                    "SELECT format('COMMENT ON SCHEMA %I IS %L',current_schema(),'onchina:2')",
                    &[],
                )
                .map_err(|e| e.to_string())?
                .get(0);
            conn.batch_execute(&comment).map_err(|e| e.to_string())?;
            conn.batch_execute("BEGIN READ ONLY")
                .map_err(|e| e.to_string())?;
            assert!(Db::init_current_schema(conn).is_err());
            conn.batch_execute("ROLLBACK").map_err(|e| e.to_string())?;
            let value: i64 = conn
                .query_one(
                    "SELECT last_value FROM sequence WHERE seq_key='synthetic'",
                    &[],
                )
                .map_err(|e| e.to_string())?
                .get(0);
            assert_eq!(value, 9);
            let format: String = conn
                .query_one(
                    "SELECT obj_description(current_schema()::regnamespace,'pg_namespace')",
                    &[],
                )
                .map_err(|e| e.to_string())?
                .get(0);
            assert_eq!(format, "onchina:2");
            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
fn private_subject_is_one_record_with_nullable_personality_and_atomic_constraints() {
    use crate::institutions::subjects::Institution;
    let sandbox = IsolatedTestDb::new();
    let db = &sandbox.db;
    let cid = format!(
        "GZ018-SFGQ{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018SFGQ0000000012026", false)
    );
    let mut inst: Institution = serde_json::from_value(serde_json::json!({
        "status":"ACTIVE","subjectProperty":"S","chainStatus":"ACTIVE_ON_CHAIN",
        "cidNumber":cid,"cidFullName":"测试公司","cidShortName":"测试",
        "category":"PRIVATE_INSTITUTION","p1":"1","provinceName":"贵州省",
        "cityName":"绥阳市","provinceCode":"GZ","cityCode":"018",
        "institutionCode":"SFGQ","privateType":"COMPANY","createdAt":chrono::Utc::now()
    }))
    .unwrap();
    // NULL、false、true均保持主体正本的实际读取语义。
    for personality in [None, Some(false), Some(true)] {
        inst.has_legal_personality = personality;
        db.upsert_institution_row(&inst).unwrap();
        let actual = db.get_institution_with_accounts(&cid).unwrap().unwrap().0;
        assert_eq!(actual.private_type.as_deref(), Some("COMPANY"));
        assert_eq!(actual.has_legal_personality, personality);
    }
    // 每个失败都经过真实事务，身份索引和旧省主体不得被部分更新。
    for case in 0..5 {
        let mut invalid = inst.clone();
        match case {
            0 => invalid.private_type = Some("UNKNOWN".into()),
            1 => invalid.partnership_kind = Some("UNKNOWN".into()),
            2 => invalid.p1 = "2".into(),
            3 => invalid.city_code = "000".into(),
            _ => {
                invalid.province_code = "GD".into();
                invalid.city_code = String::new();
            }
        }
        assert!(db.upsert_institution_row(&invalid).is_err(), "case={case}");
        let actual = db.get_institution_with_accounts(&cid).unwrap().unwrap().0;
        assert_eq!(actual.province_code, "GZ");
        assert_eq!(actual.city_code, "018");
        assert_eq!(actual.private_type.as_deref(), Some("COMPANY"));
        db.with_client(|conn| {
            let province: String = conn
                .query_one(
                    "SELECT province_code FROM identity WHERE cid_number=$1",
                    &[&cid],
                )
                .map_err(|e| e.to_string())?
                .get(0);
            assert_eq!(province, "GZ");
            Ok(())
        })
        .unwrap();
    }
    // 未分类记录原本不写私权副表，不新增全部私权都必须分类的规则。
    inst.private_type = None;
    inst.city_code = "000".into();
    db.upsert_institution_row(&inst).unwrap();
    assert!(db
        .get_institution_with_accounts(&cid)
        .unwrap()
        .unwrap()
        .0
        .private_type
        .is_none());
}

#[test]
#[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
fn private_maintenance_uses_subject_for_backup_and_cleanup() {
    let sandbox = IsolatedTestDb::new();
    let db = &sandbox.db;
    let inst: crate::institutions::subjects::Institution =
        serde_json::from_value(serde_json::json!({
            "status":"ACTIVE","subjectProperty":"S","cidNumber":"synthetic-invalid-cid",
            "cidFullName":"维护测试","category":"PRIVATE_INSTITUTION","p1":"1",
            "provinceName":"贵州省","cityName":"绥阳市","provinceCode":"GZ",
            "cityCode":"018","institutionCode":"SFGQ","privateType":"COMPANY",
            "createdAt":chrono::Utc::now()
        }))
        .unwrap();
    db.upsert_institution_row(&inst).unwrap();
    // dry_run真实遍历动态清理清单并回滚，不能引用已退役副表。
    db.purge_legacy_cid_rows(true).unwrap();
    assert!(db
        .get_institution_with_accounts(&inst.cid_number)
        .unwrap()
        .is_some());
    let work = std::env::var("CITIZENCHAIN_WORK_DIR").expect("控制台必须提供本轮独立现场");
    let backup = std::path::Path::new(&work).join(format!("orphan-{}.sql", uuid::Uuid::new_v4()));
    let scope =
        std::collections::BTreeMap::from([("GZ".to_string(), vec![inst.cid_number.clone()])]);
    db.export_orphan_backup(&scope, backup.to_str().unwrap())
        .unwrap();
    let exported = std::fs::read_to_string(&backup).unwrap();
    std::fs::remove_file(backup).unwrap();
    assert!(exported.contains("TABLE=subject"));
    assert!(exported.contains(&inst.cid_number));
    assert!(!exported.contains("TABLE=private"));
    assert_eq!(
        db.delete_orphan_institutions_by_province("GZ", &[], &[inst.cid_number.clone()])
            .unwrap(),
        1
    );
    assert!(db
        .get_institution_with_accounts(&inst.cid_number)
        .unwrap()
        .is_none());
}
