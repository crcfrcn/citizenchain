//! Indexer 后台 worker：持续订阅链上区块，解析事件，写入数据库。

use std::time::Duration;

use subxt::backend::legacy::LegacyRpcMethods;
use subxt::backend::rpc::RpcClient;
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
    let rpc_client = RpcClient::from_insecure_url(ws_url)
        .await
        .map_err(|e| format!("connect rpc: {e}"))?;
    let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
    let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client)
        .await
        .map_err(|e| format!("connect to chain: {e}"))?;

    info!("indexer connected to chain");

    // 本节点投影作用域:仅城市/省注册局(非联邦)在 indexer 里按 residence/CID 市码增量投影本市
    // 公民/私权机构;联邦节点走 M3 drill-in,不在此投影。未绑定/解析失败则关闭投影(不影响索引)。
    let projection_scope = match crate::domains::projection::resolve_node_scope(db_pool) {
        Ok(Some((false, scope))) => {
            info!(
                province = %scope.province_code,
                city = %scope.city_code,
                "indexer entity projection enabled for city registry scope"
            );
            Some(scope)
        }
        Ok(Some((true, _))) | Ok(None) => None,
        Err(err) => {
            warn!(error = %err, "indexer entity projection disabled: cannot resolve node scope");
            None
        }
    };

    // 读取当前索引进度
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
    let latest_number = finalized_header.number as i64;
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
            projection_scope.as_ref(),
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
            process_block_at_hash(&client, db_pool, n, hash, projection_scope.as_ref()).await?;
            n += 1;
        }

        // 处理当前区块（已有 block 对象）
        let events = block
            .events()
            .await
            .map_err(|e| format!("fetch events #{block_num}: {e}"))?;
        let block_ts = extract_block_timestamp_from_block(&block).await;
        let records = event_parser::parse_block_events(&events, block_num, block_ts);
        db_pool.with_client(|conn| db::insert_block_records(conn, block_num, &records))?;
        if let Some(scope) = projection_scope.as_ref() {
            project_block_entities(db_pool, &events, block.hash(), scope).await;
        }
    }
}

/// 通过 block hash 处理单个区块。
async fn process_block_at_hash(
    client: &OnlineClient<PolkadotConfig>,
    db_pool: &Db,
    block_number: i64,
    block_hash: subxt::utils::H256,
    projection_scope: Option<&crate::domains::projection::NodeScope>,
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
    let records = event_parser::parse_block_events(&events, block_number, block_ts);
    db_pool.with_client(|conn| db::insert_block_records(conn, block_number, &records))?;

    if let Some(scope) = projection_scope {
        project_block_entities(db_pool, &events, block_hash, scope).await;
    }

    Ok(())
}

/// 对一个区块内涉及的公民/私权机构 CID 做增量投影(城市/省注册局)。
/// 单条失败仅告警,不中断索引;投影层内部再按作用域过滤(不在本市则跳过)。
async fn project_block_entities(
    db_pool: &Db,
    events: &subxt::events::Events<PolkadotConfig>,
    block_hash: subxt::utils::H256,
    scope: &crate::domains::projection::NodeScope,
) {
    let (citizen_cids, institution_cids) = event_parser::collect_entity_projection_cids(events);
    for cid in citizen_cids {
        if let Err(err) = crate::domains::projection::project_citizen_by_cid_at(
            db_pool,
            &cid,
            scope,
            Some(block_hash.0),
        )
        .await
        {
            warn!(cid = %cid, error = %err, "indexer citizen projection failed");
        }
    }
    for cid in institution_cids {
        if let Err(err) =
            crate::domains::projection::project_private_institution_by_cid(db_pool, &cid, scope)
                .await
        {
            warn!(cid = %cid, error = %err, "indexer private institution projection failed");
        }
    }
}

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
