//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::citizens::admin_entry::CitizenWrites;

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn concurrent_ids_are_unique_and_capacity_exhaustion_is_explicit() {
    let sandbox = onchina::database::IsolatedTestDb::new();
    let handles = (0..8)
        .map(|_| {
            let db = sandbox.db.clone();
            std::thread::spawn(move || {
                (0..16)
                    .map(|_| db.with_client(Db::allocate_citizen_id_conn).unwrap())
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let ids = handles
        .into_iter()
        .flat_map(|handle| handle.join().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 128);
    assert_eq!(ids.first(), Some(&1));
    assert_eq!(ids.last(), Some(&128));
    sandbox
        .db
        .with_client(|conn| {
            conn.execute(
                "UPDATE sequence SET last_value=9223372036854775807 WHERE seq_key='citizen_id'",
                &[],
            )
            .unwrap();
            assert!(Db::allocate_citizen_id_conn(conn).is_err());
            Ok(())
        })
        .unwrap();
}
