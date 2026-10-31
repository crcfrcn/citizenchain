//! 原所属模块回归，断言与输入保持。
use super::*;
#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn block_rows_progress_and_chain_anchor_commit_or_roll_back_together() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let record = |block, index| TxRecordInsert {
        block_number: block,
        extrinsic_index: Some(0),
        event_index: index,
        tx_type: "transfer",
        sender_account_id: None,
        recipient_account_id: None,
        amount_fen: 1,
        fee_fen: None,
        block_timestamp: None,
    };
    sandbox
        .db
        .with_client(|conn| {
            ensure_indexer_chain(conn, "genesis-a")?;
            insert_block_records(
                conn,
                1,
                "genesis-a",
                "block-1",
                "genesis-a",
                &[record(1, 0)],
            )?;
            insert_block_records(
                conn,
                1,
                "genesis-a",
                "block-1",
                "genesis-a",
                &[record(1, 0)],
            )?;
            assert!(insert_block_records(
                conn,
                2,
                "genesis-a",
                "block-2",
                "block-1",
                &[record(2, 0), record(2, 0)]
            )
            .is_err());
            assert!(insert_block_records(conn, 3, "genesis-a", "block-3", "block-1", &[]).is_err());
            assert!(insert_block_records(conn, 2, "genesis-b", "block-2", "block-1", &[]).is_err());
            assert!(
                insert_block_records(conn, 2, "genesis-a", "block-2", "wrong-parent", &[]).is_err()
            );
            assert_eq!(read_last_indexed_block(conn)?, 1);
            assert_eq!(
                conn.query_one("SELECT COUNT(*) FROM \"transaction\"", &[])
                    .unwrap()
                    .get::<_, i64>(0),
                1
            );
            let anchor = conn
                .query_one(
                    "SELECT block_hash,block_number FROM projection WHERE projection_key=$1",
                    &[&WALLET_INDEX_KEY],
                )
                .unwrap();
            assert_eq!(anchor.get::<_, String>(0), "block-1");
            assert_eq!(anchor.get::<_, Option<i64>>(1), Some(1));
            assert!(ensure_indexer_chain(conn, "genesis-b").is_err());
            conn.execute(
                "DELETE FROM projection WHERE projection_key=$1",
                &[&WALLET_INDEX_KEY],
            )
            .unwrap();
            assert!(ensure_indexer_chain(conn, "genesis-a").is_err());
            assert!(query_tx_records(
                conn,
                "genesis-a",
                &format!("0x{}", "11".repeat(32)),
                None,
                None,
                10
            )
            .is_err());
            Ok(())
        })
        .unwrap();
}
