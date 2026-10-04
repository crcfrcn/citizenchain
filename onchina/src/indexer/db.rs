//! Indexer 数据库读写操作。

use chrono::{DateTime, Utc};
use postgres::Client;

/// 一条待写入的交易记录。
pub(crate) struct TxRecordInsert {
    pub block_number: i64,
    pub extrinsic_index: Option<i16>,
    pub event_index: i16,
    pub tx_type: &'static str,
    pub sender_account_id: Option<String>,
    pub recipient_account_id: Option<String>,
    pub amount_fen: i64,
    pub fee_fen: Option<i64>,
    pub block_timestamp: Option<DateTime<Utc>>,
}

/// 读取当前索引进度（last_indexed_block）。
pub(crate) fn read_last_indexed_block(conn: &mut Client) -> Result<i64, String> {
    let row = conn
        .query_one(
            "SELECT last_indexed_block FROM tx_indexer_state WHERE id=1",
            &[],
        )
        .map_err(|e| format!("read tx_indexer_state: {e}"))?;
    Ok(row.get(0))
}

/// 在一个事务中批量写入一个区块的所有交易记录，并更新索引进度。
pub(crate) fn insert_block_records(
    conn: &mut Client,
    block_number: i64,
    records: &[TxRecordInsert],
) -> Result<(), String> {
    let mut tx = conn.transaction().map_err(|e| format!("begin tx: {e}"))?;

    for r in records {
        tx.execute(
            "INSERT INTO tx_records(block_number, extrinsic_index, event_index, tx_type, sender_account_id, recipient_account_id, amount_fen, fee_fen, block_timestamp)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            &[
                &r.block_number,
                &r.extrinsic_index,
                &r.event_index,
                &r.tx_type,
                &r.sender_account_id,
                &r.recipient_account_id,
                &r.amount_fen,
                &r.fee_fen,
                &r.block_timestamp,
            ],
        )
        .map_err(|e| format!("insert tx_record: {e}"))?;
    }

    tx.execute(
        "UPDATE tx_indexer_state SET last_indexed_block=$1, updated_at=now() WHERE id=1",
        &[&block_number],
    )
    .map_err(|e| format!("update indexer state: {e}"))?;

    tx.commit().map_err(|e| format!("commit: {e}"))?;
    Ok(())
}

/// 查询某账户的交易记录（游标分页）。
pub(crate) fn query_tx_records(
    conn: &mut Client,
    account_id: &str,
    before_id: Option<i64>,
    tx_type_filter: Option<&str>,
    limit: i64,
) -> Result<Vec<TxRecordRow>, String> {
    // 动态构建 SQL 以支持可选的 tx_type 筛选
    let mut sql = String::from(
        "SELECT id, block_number, extrinsic_index, event_index, tx_type, \
         sender_account_id, recipient_account_id, amount_fen, fee_fen, block_timestamp \
         FROM tx_records WHERE (sender_account_id=$1 OR recipient_account_id=$1)",
    );
    let mut param_idx = 2u32;

    if before_id.is_some() {
        sql.push_str(&format!(" AND id < ${param_idx}"));
        param_idx += 1;
    }

    // tx_type_filter 是逗号分隔的多值，展开为 IN 子句
    let type_values: Vec<String>;
    if let Some(filter) = tx_type_filter {
        type_values = filter.split(',').map(|s| s.trim().to_string()).collect();
        if !type_values.is_empty() {
            let placeholders: Vec<String> = type_values
                .iter()
                .enumerate()
                .map(|(i, _)| format!("${}", param_idx + i as u32))
                .collect();
            sql.push_str(&format!(" AND tx_type IN ({})", placeholders.join(",")));
        }
    } else {
        type_values = Vec::new();
    }

    sql.push_str(&format!(
        " ORDER BY id DESC LIMIT ${0}",
        param_idx + type_values.len() as u32
    ));

    // 构建参数列表
    let mut params: Vec<Box<dyn postgres::types::ToSql + Sync>> = Vec::new();
    params.push(Box::new(account_id.to_string()));
    if let Some(bid) = before_id {
        params.push(Box::new(bid));
    }
    for tv in &type_values {
        params.push(Box::new(tv.clone()));
    }
    params.push(Box::new(limit));

    let param_refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
        params.iter().map(|p| p.as_ref()).collect();

    let rows = conn
        .query(&sql, &param_refs)
        .map_err(|e| format!("query tx_records: {e}"))?;

    Ok(rows
        .iter()
        .map(|row| TxRecordRow {
            id: row.get(0),
            block_number: row.get(1),
            extrinsic_index: row.get(2),
            event_index: row.get(3),
            tx_type: row.get(4),
            sender_account_id: row.get(5),
            recipient_account_id: row.get(6),
            amount_fen: row.get(7),
            fee_fen: row.get(8),
            block_timestamp: row.get(9),
        })
        .collect())
}

// DB SELECT 列序投影:字段按 row.get(索引) 位置读入,保留以对齐查询列顺序。
#[allow(dead_code)]
pub(crate) struct TxRecordRow {
    pub id: i64,
    pub block_number: i64,
    pub extrinsic_index: Option<i16>,
    pub event_index: i16,
    pub tx_type: String,
    pub sender_account_id: Option<String>,
    pub recipient_account_id: Option<String>,
    pub amount_fen: i64,
    pub fee_fen: Option<i64>,
    pub block_timestamp: Option<DateTime<Utc>>,
}
