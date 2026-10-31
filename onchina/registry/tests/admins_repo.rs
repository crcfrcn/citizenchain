//! 原所属模块回归，断言与输入保持。
use super::*;
#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn roster_counts_and_delete_guard_use_exact_institution_geography() {
    let sandbox = onchina::database::IsolatedTestDb::new();
    sandbox.db.with_client(|conn| {
            let mut users=Vec::new();
            for (index,province,city) in [(1i64,"GD","001"),(2,"GZ","001")] {
                let cid=format!("{province}{city}-CREG{}-000000001-2026",primitives::cid::number::checksum_char_m1(&format!("{province}{city}CREG0000000012026"),false));
                conn.execute("INSERT INTO subject(cid_number,record_kind,province_code,city_code,institution_code,state_code,created_at,updated_at) VALUES($1,'PUBLIC',$2,$3,'CREG','ACTIVE',now(),now())", &[&cid,&province,&city]).unwrap();
                conn.execute("INSERT INTO government(cid_number,province_code,city_code,institution_code,source_kind) VALUES($1,$2,$3,'CREG','CHAIN')", &[&cid,&province,&city]).unwrap();
                let user=AdminUser { id:index as u64,account_id:format!("0x{}",format!("{index:02x}").repeat(32)),family_name:"测".into(),given_name:"试".into(),institution_code:"CREG".into(),institution_cid_number:Some(cid),built_in:false,creator_account_id:format!("0x{}","11".repeat(32)),created_at:chrono::Utc::now(),updated_at:None,city_name:"同名市".into() };
                onchina::authentication::repo::upsert_admin_conn(conn,&user).unwrap(); users.push(user);
            }
            let (count,rows)=list_city_registry_admins_by_scope_conn(conn,"广东省",None,None,100,0)?;
            assert_eq!(count,1); assert_eq!(rows[0].id,1);
            assert_eq!(count_city_registry_admins_by_city_conn(conn,"广东省","荔湾市")?,1);
            assert!(super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&users[0]));
            assert!(!super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&users[1]));
            let mut unknown=users[1].clone(); unknown.id=3; unknown.account_id=format!("0x{}","33".repeat(32)); unknown.institution_cid_number=None;
            onchina::authentication::repo::upsert_admin_conn(conn,&unknown).unwrap();
            assert!(!super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&unknown));
            assert_eq!(list_city_registry_admins_by_scope_conn(conn,"广东省",None,None,100,0)?.0,1);
            Ok(())
        }).unwrap();
}
