//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn empty_identity_is_absent_and_database_failure_is_preserved() {
    let db = Db::empty_for_route_test();
    assert!(db
        .find_governance_citizen_by_account_id("  ")
        .unwrap()
        .is_none());
    assert!(db.find_governance_citizen_by_cid("\t").unwrap().is_none());
    assert_eq!(
        db.find_governance_citizen_by_account_id("account")
            .unwrap_err(),
        "postgres client pool is empty"
    );
    assert_eq!(
        db.find_governance_citizen_by_cid("cid").unwrap_err(),
        "postgres client pool is empty"
    );
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn governance_identity_keeps_fields_latest_order_and_missing_values() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let actor = format!("0x{}", "11".repeat(32));
    // 只写本测试独占schema，原分区、字段约束和实际查询均参与回归。
    sandbox.db.with_client(|conn| {
            conn.execute(
                "INSERT INTO citizen(cid_number, family_name, given_name, account_id,
                 province_code, city_code, citizen_status, voting_eligible, creator_account, created_at)
                 VALUES ('old', '旧姓', '旧名', $1, 'GZ', '001', 'NORMAL', true, $1, now() - interval '1 second'),
                        ('latest', '新姓', '新名', $1, 'GZ', '001', 'NORMAL', true, $1, now()),
                        ('unbound', '未绑定', '公民', NULL, 'GZ', '001', 'NORMAL', false, $1, now())",
                &[&actor],
            ).map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
    let expected = crate::institutions::model::GovernanceCitizen {
        cid_number: "latest".into(),
        family_name: "新姓".into(),
        given_name: "新名".into(),
        account_id: Some(actor.clone()),
    };
    assert_eq!(
        sandbox
            .db
            .find_governance_citizen_by_account_id(&format!(" {actor} "))
            .unwrap(),
        Some(expected)
    );
    let old = sandbox
        .db
        .find_governance_citizen_by_cid(" old ")
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            old.cid_number.as_str(),
            old.family_name.as_str(),
            old.given_name.as_str(),
            old.account_id.as_deref()
        ),
        ("old", "旧姓", "旧名", Some(actor.as_str()))
    );
    assert!(sandbox
        .db
        .find_governance_citizen_by_cid("unbound")
        .unwrap()
        .unwrap()
        .account_id
        .is_none());
    assert!(sandbox
        .db
        .find_governance_citizen_by_cid("missing")
        .unwrap()
        .is_none());
    assert!(sandbox
        .db
        .find_governance_citizen_by_account_id(&format!("0x{}", "22".repeat(32)))
        .unwrap()
        .is_none());
    // schema内临时隐藏源表，确保真实SQL失败不会伪装为未找到。
    sandbox
        .db
        .with_client(|conn| {
            conn.batch_execute("ALTER TABLE citizen RENAME TO citizens_unavailable")
                .map_err(|e| e.to_string())
        })
        .unwrap();
    assert!(sandbox
        .db
        .find_governance_citizen_by_cid("latest")
        .unwrap_err()
        .contains("query governance citizen by cid failed"));
    assert!(sandbox
        .db
        .find_governance_citizen_by_account_id(&actor)
        .unwrap_err()
        .contains("query governance citizen by account failed"));
}
