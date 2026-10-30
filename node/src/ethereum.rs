use codec::DecodeAll;
use futures::future::BoxFuture;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use pallet_revive::evm::BlockNumberOrTagOrHash;
use pallet_revive::evm::BlockTag;
use pallet_revive::evm::SubscriptionKind;
use pallet_revive::evm::SubscriptionOptions;
use pallet_revive::AddressMapper;
use pallet_revive_eth_rpc::client::Client;
use pallet_revive_eth_rpc::client::ClientError;
use pallet_revive_eth_rpc::EthRpcServer;
use pallet_revive_eth_rpc::EthRpcServerImpl;
use sp_core::H160;
use sp_core::U256;

/// 近期索引上限与官方分叉缓存一致，重启时从现有链状态重建。
const RETAINED_BLOCKS: usize = 256;

/// 内部请求只调用节点已有原生模块；错误及缺失结果不得被当作空存储或零 nonce。
async fn native_request(
    native: &RpcModule<()>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, ErrorObjectOwned> {
    let request =
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    let (response, _) = native
        .raw_json_request(&request, 1)
        .await
        .map_err(|_| ErrorObjectOwned::owned(-32603, "原生 RPC 请求失败", None::<()>))?;
    let response: serde_json::Value = serde_json::from_str(&response)
        .map_err(|_| ErrorObjectOwned::owned(-32603, "原生 RPC 响应无效", None::<()>))?;
    if let Some(error) = response.get("error") {
        return Err(
            serde_json::from_value::<ErrorObjectOwned>(error.clone()).unwrap_or_else(|_| {
                ErrorObjectOwned::owned(-32603, "原生 RPC 错误响应无效", None::<()>)
            }),
        );
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| ErrorObjectOwned::owned(-32603, "原生 RPC 响应缺少结果", None::<()>))
}

/// OriginalAccount 使用官方 Identity 存储键；通过真实链后端读取，禁止在宿主线程调用 Runtime 存储。
async fn pending_account(
    native: &RpcModule<()>,
    address: H160,
) -> Result<sp_runtime::AccountId32, ErrorObjectOwned> {
    let mut key = frame_support::storage::storage_prefix(b"Revive", b"OriginalAccount").to_vec();
    key.extend_from_slice(address.as_bytes());
    let result = native_request(
        native,
        "state_getStorage",
        serde_json::json!([format!("0x{}", hex::encode(key))]),
    )
    .await?;
    let mapped: Option<sp_core::Bytes> = serde_json::from_value(result).map_err(|_| {
        ErrorObjectOwned::owned(-32603, "Ethereum 地址映射存储响应无效", None::<()>)
    })?;
    match mapped {
        Some(bytes) => sp_runtime::AccountId32::decode_all(&mut &bytes.0[..])
            .map_err(|_| ErrorObjectOwned::owned(-32603, "Ethereum 地址映射存储无效", None::<()>)),
        None => Ok(
            pallet_revive::AccountId32Mapper::<runtime::Runtime>::to_fallback_account_id(&address),
        ),
    }
}

/// 标准 Ethereum 订阅参数直接绑定官方类型，未知类型及多余参数必须拒绝。
fn subscription_request(
    params: &jsonrpsee::types::Params<'_>,
) -> Result<(SubscriptionKind, Option<SubscriptionOptions>), ErrorObjectOwned> {
    let arguments: Vec<serde_json::Value> = params.parse()?;
    let invalid = || ErrorObjectOwned::owned(-32602, "无效 Ethereum 订阅参数", None::<()>);
    if arguments.is_empty() || arguments.len() > 2 {
        return Err(invalid());
    }
    let kind = match arguments[0].as_str() {
        Some("newHeads") => SubscriptionKind::NewBlockHeaders,
        Some("logs") => SubscriptionKind::Logs,
        _ => return Err(invalid()),
    };
    let options = arguments
        .get(1)
        .cloned()
        .map(serde_json::from_value::<Option<SubscriptionOptions>>)
        .transpose()
        .map_err(|_| invalid())?
        .flatten();
    Ok((kind, options))
}

/// 初始化官方 Ethereum 方法；返回的同步任务必须由节点 TaskManager 持有。
pub(crate) async fn initialize(
    native: RpcModule<()>,
) -> Result<
    (RpcModule<()>, BoxFuture<'static, Result<(), ClientError>>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let (client, synchronization) =
        Client::from_in_process_rpc(native.clone(), RETAINED_BLOCKS).await?;
    if client.chain_id() != primitives::core_const::ETHEREUM_CHAIN_ID {
        return Err("Ethereum RPC 与 Runtime 的 ChainId 不一致".into());
    }
    let mut module = EthRpcServerImpl::new(client.clone()).into_rpc();
    // 使用 Ethereum 标准 newHeads 参数，通知和取消仍由同一官方处理器负责。
    module.remove_method("eth_subscribe");
    module.remove_method("eth_unsubscribe");
    module.register_subscription(
        "eth_subscribe",
        "eth_subscription",
        "eth_unsubscribe",
        |params, pending, server, _| async move {
            match subscription_request(&params) {
                Ok((kind, options)) => server.eth_subscribe(pending, kind, options).await,
                Err(error) => pending.reject(error).await,
            }
        },
    )?;
    // pending 使用同一交易池的原生 nonce 计算，防止连续签名重复使用 best-state nonce。
    module.remove_method("eth_getTransactionCount");
    module.register_async_method("eth_getTransactionCount", move |params, _, _| {
        let client = client.clone();
        let native = native.clone();
        async move {
            let (address, block): (H160, BlockNumberOrTagOrHash) = params.parse()?;
            if matches!(block, BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending)) {
                let account = pending_account(&native, address).await?;
                let result = native_request(
                    &native,
                    "system_accountNextIndex",
                    serde_json::json!([account]),
                )
                .await?;
                let nonce: runtime::Nonce = serde_json::from_value(result).map_err(|_| {
                    ErrorObjectOwned::owned(-32603, "交易池 nonce 响应无效", None::<()>)
                })?;
                return Ok::<U256, ErrorObjectOwned>(nonce.into());
            }
            let hash = client
                .block_hash_for_tag(block)
                .await
                .map_err(ErrorObjectOwned::from)?;
            client
                .runtime_api(hash)
                .nonce(address)
                .await
                .map_err(ErrorObjectOwned::from)
        }
    })?;
    Ok((module.remove_context(), synchronization))
}
#[cfg(test)]
#[path = "tests/ethereum.rs"]
mod tests;
