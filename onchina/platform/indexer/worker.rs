//! Indexer 后台 worker：持续订阅链上区块，解析事件，写入数据库。

use std::time::Duration;

use subxt::backend::legacy::LegacyRpcMethods;
use subxt::ext::scale_value::At;
use subxt::{OnlineClient, PolkadotConfig};
use tracing::{error, info, warn};

use crate::Db;

use super::db;
use super::event_parser;

/// Indexer 后台任务入口。在 main.rs 中通过 `tokio::spawn` 启动。
pub(crate) async fn indexer_worker(db_pool: Db) {
    let ws_url = match crate::core::chain_url::chain_ws_url() {
        Ok(url) => url,
        Err(err) => {
            warn!("indexer disabled: {err}");
            return;
        }
    };

    info!(url = %ws_url, "indexer worker starting");

    // 带 backoff 的重连循环
    let mut backoff_secs = 2u64;
    loop {
        match run_indexer_loop(&ws_url, &db_pool).await {
            Ok(()) => {
                info!("indexer loop exited normally, restarting in {backoff_secs}s");
            }
            Err(err) => {
                // 链节点未启动是允许的降级状态,只提示后台索引稍后重试。
                if err.contains("connect rpc") {
                    warn!(
                        error = %err,
                        retry_in = backoff_secs,
                        "indexer chain rpc unavailable, will retry"
                    );
                } else {
                    error!(
                        error = %err,
                        retry_in = backoff_secs,
                        "indexer loop failed, will retry"
                    );
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(60);
    }
}

/// 主索引循环：连接链，追赶历史区块，然后订阅新区块。
async fn run_indexer_loop(ws_url: &str, db_pool: &Db) -> Result<(), String> {
    let rpc_client = crate::core::chain_url::rpc_client(ws_url)
        .await
        .map_err(|e| format!("connect rpc: {e}"))?;
    let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
    let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client)
        .await
        .map_err(|e| format!("connect to chain: {e}"))?;

    info!("indexer connected to chain");

    let genesis = format!("{:#x}", client.genesis_hash());
    db_pool.with_client(|conn| db::ensure_indexer_chain(conn, &genesis))?;

    // 读取已核实的当前索引进度
    let last_indexed = db_pool.with_client(db::read_last_indexed_block)?;
    info!(last_indexed_block = last_indexed, "indexer resuming");

    // 获取链上最新已最终化区块
    let finalized_hash = rpc
        .chain_get_finalized_head()
        .await
        .map_err(|e| format!("fetch finalized head: {e}"))?;
    let finalized_header = rpc
        .chain_get_header(Some(finalized_hash))
        .await
        .map_err(|e| format!("fetch finalized header: {e}"))?
        .ok_or("finalized header not found")?;
    let latest_number = i64::from(finalized_header.number);
    if last_indexed > latest_number { return Err("wallet index progress is ahead of finalized chain".into()); }
    info!(latest_finalized = latest_number, "chain tip");

    // 追赶历史区块
    let mut next_block = last_indexed + 1;
    if next_block <= latest_number {
        info!(
            from = next_block,
            to = latest_number,
            "catching up historical blocks"
        );
    }
    while next_block <= latest_number {
        let hash = rpc
            .chain_get_block_hash(Some((next_block as u32).into()))
            .await
            .map_err(|e| format!("fetch block hash #{next_block}: {e}"))?
            .ok_or_else(|| format!("block #{next_block} not found"))?;
        process_block_at_hash(
            &client,
            db_pool,
            next_block,
            hash,
            &genesis,
        )
        .await?;
        if next_block % 1000 == 0 {
            info!(block = next_block, "indexer catch-up progress");
        }
        next_block += 1;
    }

    info!("indexer caught up, subscribing to new blocks");

    // 订阅已最终化的区块
    let mut block_sub = client
        .blocks()
        .subscribe_finalized()
        .await
        .map_err(|e| format!("subscribe finalized blocks: {e}"))?;

    loop {
        let block = match block_sub.next().await {
            Some(Ok(b)) => b,
            Some(Err(e)) => return Err(format!("block subscription: {e}")),
            None => return Err("block subscription ended unexpectedly".to_string()),
        };
        let block_num = block.number() as i64;

        // 跳过已索引的区块
        let current_last = db_pool.with_client(db::read_last_indexed_block)?;
        if block_num <= current_last {
            continue;
        }

        // 按顺序处理缺失的区块
        let mut n = current_last + 1;
        while n < block_num {
            let hash = rpc
                .chain_get_block_hash(Some((n as u32).into()))
                .await
                .map_err(|e| format!("fetch block hash #{n}: {e}"))?
                .ok_or_else(|| format!("block #{n} not found"))?;
            process_block_at_hash(&client, db_pool, n, hash, &genesis).await?;
            n += 1;
        }

        // 处理当前区块（已有 block 对象）
        let events = block
            .events()
            .await
            .map_err(|e| format!("fetch events #{block_num}: {e}"))?;
        let block_ts = extract_block_timestamp_from_block(&block).await;
        let records = event_parser::parse_block_events(&events, block_num, block_ts)?;
        db_pool.with_client(|conn| db::insert_block_records(conn, block_num, &genesis, &format!("{:#x}", block.hash()), &format!("{:#x}", block.header().parent_hash), &records))?;
    }
}

/// 通过 block hash 处理单个区块。
async fn process_block_at_hash(
    client: &OnlineClient<PolkadotConfig>,
    db_pool: &Db,
    block_number: i64,
    block_hash: subxt::utils::H256,
    genesis: &str,
) -> Result<(), String> {
    let block = client
        .blocks()
        .at(block_hash)
        .await
        .map_err(|e| format!("fetch block #{block_number}: {e}"))?;

    let events = block
        .events()
        .await
        .map_err(|e| format!("fetch events #{block_number}: {e}"))?;

    let block_ts = extract_block_timestamp_from_block(&block).await;
    let records = event_parser::parse_block_events(&events, block_number, block_ts)?;
    db_pool.with_client(|conn| db::insert_block_records(conn, block_number, genesis, &format!("{block_hash:#x}"), &format!("{:#x}", block.header().parent_hash), &records))?;


    Ok(())
}

/// 绑定后无需等断链或新区块；独立投影循环每五秒重新确认当前绑定和持久游标。




/// 从区块的 extrinsics 中提取 Timestamp::set 的值。
async fn extract_block_timestamp_from_block(
    block: &subxt::blocks::Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Option<u64> {
    use subxt::ext::scale_value::Value;

    let extrinsics = block.extrinsics().await.ok()?;
    for ext in extrinsics.iter() {
        let pallet = ext.pallet_name().ok()?;
        let variant = ext.variant_name().ok()?;
        if pallet == "Timestamp" && variant == "set" {
            if let Ok(fields) = ext.field_values() {
                let now_val: Option<&Value<u32>> = fields.at("now");
                return now_val.and_then(|v| v.as_u128()).map(|v| v as u64);
            }
        }
    }
    None
}

/// 公共绑定机构每五秒刷新，与注册局模块是否装载无关。
pub(crate) async fn bound_institution_projection_worker(db: Db) {
    loop {
        if let Err(error) = crate::institutions::catalog::projection::refresh_bound_institution(&db).await {
            warn!(error = %error, "bound institution projection paused");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
