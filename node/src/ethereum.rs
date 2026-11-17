//! 公民链 Ethereum 接入；交易/事件始终绑定执行状态，状态查询使用请求指定块。
use codec::{DecodeAll, Encode};
use ::runtime;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use jsonrpsee::core::{async_trait, RpcResult};
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage};
use pallet_revive::evm::BlockNumberOrTagOrHash;
use pallet_revive::evm::BlockTag;
use pallet_revive::evm::SubscriptionKind;
use pallet_revive::evm::SubscriptionOptions;
use pallet_revive::evm::*;
use pallet_revive::AddressMapper;
use pallet_revive_eth_rpc::client::ClientError;
use pallet_revive_eth_rpc::client::{SubscriptionType, SubstrateBlock};
use pallet_revive_eth_rpc::subxt_client::SrcChainConfig;
use pallet_revive_eth_rpc::EthRpcServer;
use pallet_revive_eth_rpc::{
    BlockInfoProvider, DbContext, EthRpcError, FeeHistoryProvider, ReceiptExtractor,
    ReceiptProvider,
};
use sp_core::H160;
use sp_core::U256;
use std::sync::Arc;
use subxt::backend::legacy::LegacyRpcMethods;
use subxt::backend::rpc::RpcClient;
use subxt::{Metadata, OnlineClient};
use tokio::sync::{broadcast, Mutex, RwLock};

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
async fn account_at(
    native: &RpcModule<()>,
    address: H160,
    at: Option<H256>,
) -> Result<sp_runtime::AccountId32, ErrorObjectOwned> {
    let mut key = frame_support::storage::storage_prefix(b"Revive", b"OriginalAccount").to_vec();
    key.extend_from_slice(address.as_bytes());
    let mut params = vec![serde_json::json!(format!("0x{}", hex::encode(key)))];
    if let Some(hash) = at {
        params.push(serde_json::json!(hash));
    }
    let result = native_request(native, "state_getStorage", serde_json::Value::Array(params))
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

type Result<T, E = ClientError> = std::result::Result<T, E>;
type Raw = Box<serde_json::value::RawValue>;
type RpcError = subxt::ext::subxt_rpcs::Error;

fn failure(message: impl Into<String>) -> ClientError {
    ClientError::Jsonrpsee(jsonrpsee::core::ClientError::Call(ErrorObjectOwned::owned(
        -32603,
        message.into(),
        None::<()>,
    )))
}

/// 只使用宿主既有进程内模块；不增加端口、公开Unsafe方法或另建网络连接。
#[derive(Clone)]
struct Native(Arc<RpcModule<()>>);

impl Native {
    async fn request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        native_request(&self.0, method, params)
            .await
            .map_err(|error| ClientError::Jsonrpsee(jsonrpsee::core::ClientError::Call(error)))
    }

    async fn runtime<T: DecodeAll>(&self, at: H256, method: &str, args: Vec<u8>) -> Result<T> {
        let raw = self
            .request(
                "state_call",
                serde_json::json!([
                    format!("ReviveApi_{method}"),
                    format!("0x{}", hex::encode(args)),
                    at,
                ]),
            )
            .await?;
        let raw: sp_core::Bytes = serde_json::from_value(raw)
            .map_err(|error| failure(format!("Runtime API {method} @{at:?}: {error}")))?;
        T::decode_all(&mut raw.0.as_slice())
            .map_err(|error| failure(format!("Runtime API {method} @{at:?} 解码失败: {error}")))
    }
}

/// JSON原始值沿用公开RPC合同，保留大整数、null和错误data。
fn raw_result(response: &str) -> std::result::Result<Raw, RpcError> {
    let response: jsonrpsee::types::Response<'_, Raw> =
        serde_json::from_str(response).map_err(RpcError::Deserialization)?;
    match response.payload {
        jsonrpsee::types::ResponsePayload::Success(value) => Ok(value.into_owned()),
        jsonrpsee::types::ResponsePayload::Error(error) => Err(subxt::ext::subxt_rpcs::UserError {
            code: error.code(),
            message: error.message().to_owned(),
            data: error.data().map(ToOwned::to_owned),
        }
        .into()),
    }
}

struct NativeSubscription {
    native: Native,
    receiver: tokio::sync::mpsc::Receiver<String>,
    id: Raw,
    unsubscribe: String,
}

impl Drop for NativeSubscription {
    fn drop(&mut self) {
        let native = self.native.clone();
        let method = self.unsubscribe.clone();
        let id = self.id.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = native.request(&method, serde_json::json!([id])).await;
            });
        }
    }
}

impl subxt::ext::subxt_rpcs::client::RpcClientT for Native {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Raw>,
    ) -> subxt::ext::subxt_rpcs::client::RawRpcFuture<'a, Raw> {
        Box::pin(async move {
            let params = params.unwrap_or(
                serde_json::value::RawValue::from_string("[]".into())
                    .map_err(RpcError::Deserialization)?,
            );
            let request = serde_json::json!({"jsonrpc":"2.0", "id":1,
                "method":method,"params":params})
            .to_string();
            let (response, _) = self
                .0
                .raw_json_request(&request, 1)
                .await
                .map_err(RpcError::Deserialization)?;
            raw_result(&response)
        })
    }

    fn subscribe_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Raw>,
        unsubscribe: &'a str,
    ) -> subxt::ext::subxt_rpcs::client::RawRpcFuture<
        'a,
        subxt::ext::subxt_rpcs::client::RawRpcSubscription,
    > {
        Box::pin(async move {
            let params = params.unwrap_or(
                serde_json::value::RawValue::from_string("[]".into())
                    .map_err(RpcError::Deserialization)?,
            );
            let request = serde_json::json!({"jsonrpc":"2.0", "id":1,
                "method":method,"params":params})
            .to_string();
            let (response, receiver) = self
                .0
                .raw_json_request(&request, RETAINED_BLOCKS)
                .await
                .map_err(RpcError::Deserialization)?;
            let id = raw_result(&response)?;
            let id_text = id.get().to_owned();
            let state = NativeSubscription {
                native: self.clone(),
                receiver,
                id,
                unsubscribe: unsubscribe.to_owned(),
            };
            let stream = futures::stream::unfold(state, |mut state| async move {
                let message = state.receiver.recv().await?;
                #[derive(serde::Deserialize)]
                struct Notification {
                    params: NotificationData,
                }
                #[derive(serde::Deserialize)]
                struct NotificationData {
                    result: Raw,
                }
                let result = serde_json::from_str::<Notification>(&message)
                    .map(|message| message.params.result)
                    .map_err(RpcError::Deserialization);
                Some((result, state))
            });
            Ok(subxt::ext::subxt_rpcs::client::RawRpcSubscription {
                stream: Box::pin(stream),
                id: Some(id_text),
            })
        })
    }
}

/// 区块正文和事件使用父状态的唯一合同；0块没有父执行状态。
fn execution_hash(number: u32, hash: H256, parent: H256) -> H256 {
    if number == 0 {
        hash
    } else {
        parent
    }
}

#[derive(Clone)]
struct Blocks {
    rpc: RpcClient,
    legacy: LegacyRpcMethods<SrcChainConfig>,
    genesis: H256,
    heads: Arc<RwLock<(Arc<SubstrateBlock>, Arc<SubstrateBlock>)>>,
}

impl Blocks {
    async fn bind(rpc: &RpcClient, genesis: H256, hash: H256) -> Result<SubstrateBlock> {
        let legacy = LegacyRpcMethods::<SrcChainConfig>::new(rpc.clone());
        let header = legacy
            .chain_get_header(Some(hash))
            .await?
            .ok_or(ClientError::BlockNotFound)?;
        let at = execution_hash(header.number, hash, header.parent_hash);
        let (metadata, version) = tokio::try_join!(
            legacy.state_get_metadata(Some(at)),
            legacy.state_get_runtime_version(Some(at)),
        )
        .map_err(|error| {
            failure(format!(
                "区块 #{} {hash:?}，执行状态 {at:?} metadata/版本读取失败: {error}",
                header.number
            ))
        })?;
        let metadata =
            Metadata::decode_all(&mut metadata.into_raw().as_slice()).map_err(|error| {
                failure(format!(
                    "区块 #{} {hash:?}，父状态 {at:?} metadata: {error}",
                    header.number
                ))
            })?;
        // 每块新建独立Inner，不修改其他请求或全局客户端的metadata。
        // 长驻客户端的旧metadata不含V5扩展版本1，会使回执解码失败并冻结latest。
        // 正文使用执行父状态；余额、模拟等状态查询仍使用请求自身的区块。
        let client = OnlineClient::<SrcChainConfig>::from_rpc_client_with(
            genesis,
            subxt::client::RuntimeVersion {
                spec_version: version.spec_version,
                transaction_version: version.transaction_version,
            },
            metadata,
            rpc.clone(),
        )?;
        client.blocks().at(hash).await.map_err(|error| {
            failure(format!(
                "读取区块 #{} {hash:?}，执行状态 {at:?}: {error}",
                header.number
            ))
        })
    }

    async fn new(native: &Native) -> Result<Self> {
        let rpc = RpcClient::new(native.clone());
        let legacy = LegacyRpcMethods::<SrcChainConfig>::new(rpc.clone());
        let genesis = legacy.genesis_hash().await?;
        let finalized = legacy.chain_get_finalized_head().await?;
        let block = Arc::new(Self::bind(&rpc, genesis, finalized).await?);
        Ok(Self {
            rpc,
            legacy,
            genesis,
            heads: Arc::new(RwLock::new((block.clone(), block))),
        })
    }
}

#[async_trait]
impl BlockInfoProvider for Blocks {
    async fn latest_block(&self) -> Arc<SubstrateBlock> {
        self.heads.read().await.0.clone()
    }
    async fn latest_finalized_block(&self) -> Arc<SubstrateBlock> {
        self.heads.read().await.1.clone()
    }
    async fn update_latest(&self, block: Arc<SubstrateBlock>, kind: SubscriptionType) {
        let mut heads = self.heads.write().await;
        match kind {
            SubscriptionType::BestBlocks => heads.0 = block,
            SubscriptionType::FinalizedBlocks => heads.1 = block,
        }
    }
    async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<SubstrateBlock>>> {
        let heads = self.heads.read().await;
        for block in [&heads.0, &heads.1] {
            if block.hash() == *hash {
                return Ok(Some(block.clone()));
            }
        }
        drop(heads);
        if self.legacy.chain_get_header(Some(*hash)).await?.is_none() {
            return Ok(None);
        }
        Ok(Some(Arc::new(
            Self::bind(&self.rpc, self.genesis, *hash).await?,
        )))
    }
    async fn block_by_number(&self, number: u32) -> Result<Option<Arc<SubstrateBlock>>> {
        match self
            .legacy
            .chain_get_block_hash(Some(number.into()))
            .await?
        {
            Some(hash) => self.block_by_hash(&hash).await,
            None => Ok(None),
        }
    }
}

#[derive(Clone)]
struct Ethereum {
    native: Native,
    blocks: Blocks,
    receipts: ReceiptProvider<Blocks>,
    fees: FeeHistoryProvider,
    prefix: Arc<Mutex<Option<(u32, H256)>>>,
    // 最佳块发布块头，最终块发布日志；SDK公开组件负责回执、分叉裁剪及费用统计。
    heads: broadcast::Sender<Block>,
    logs: broadcast::Sender<Log>,
}

fn checked_block(
    block: Block,
    number: u32,
    hash: Option<H256>,
    absent: bool,
) -> Result<Option<Block>> {
    if absent && hash.is_none() && block == Block::default() {
        return Ok(None);
    }
    if absent
        || block.hash == H256::zero()
        || block.number != U256::from(number)
        || hash != Some(block.hash)
    {
        return Err(ClientError::SyncBoundaryMismatch);
    }
    Ok(Some(block))
}

impl Ethereum {
    async fn checked(&self, block: &SubstrateBlock) -> Result<Option<Block>> {
        let eth: Block = self
            .native
            .runtime(block.hash(), "eth_block", vec![])
            .await?;
        let hash: Option<H256> = self
            .native
            .runtime(
                block.hash(),
                "eth_block_hash",
                U256::from(block.number()).encode(),
            )
            .await?;
        let mut key = sp_crypto_hashing::twox_128(b"Revive").to_vec();
        key.extend_from_slice(&sp_crypto_hashing::twox_128(b"EthereumBlock"));
        let absent = if eth == Block::default() {
            self.native
                .request(
                    "state_getStorage",
                    serde_json::json!([format!("0x{}", hex::encode(key)), block.hash(),]),
                )
                .await?
                .is_null()
        } else {
            false
        };
        let result = checked_block(eth, block.number(), hash, absent)?;
        if result.is_none()
            && self
                .receipts
                .first_evm_block()
                .is_some_and(|n| block.number() >= n)
        {
            return Err(ClientError::SyncBoundaryMismatch);
        }
        Ok(result)
    }

    async fn index(&self, block: &SubstrateBlock) -> Result<Option<(Block, Vec<Log>)>> {
        let Some(eth) = self.checked(block).await? else {
            *self.prefix.lock().await = Some((block.number(), block.hash()));
            return Ok(None);
        };
        let receipts = self
            .receipts
            .receipts_from_block(block, eth.hash)
            .await
            .map_err(|error| {
                failure(format!(
                    "Ethereum区块 #{} {:?}，执行父状态 {:?}: {error}",
                    block.number(),
                    block.hash(),
                    block.header().parent_hash
                ))
            })?;
        self.receipts
            .insert_block_receipts(block, &receipts, &eth.hash)
            .await?;
        if self.receipts.first_evm_block().is_none() && self.prefix.lock().await.is_some() {
            self.receipts.set_first_evm_block(block.number()).await?;
        }
        let infos = receipts
            .into_iter()
            .map(|(_, receipt)| receipt)
            .collect::<Vec<_>>();
        self.fees.update_fee_history(&eth, &infos).await;
        let logs = infos.into_iter().flat_map(|receipt| receipt.logs).collect();
        Ok(Some((eth, logs)))
    }

    async fn is_indexed(&self, block: &SubstrateBlock) -> Result<bool> {
        Ok(self
            .receipts
            .get_processed_eth_block_hash(block.number(), block.hash())
            .await
            .is_some()
            || self.prefix.lock().await.as_ref() == Some(&(block.number(), block.hash())))
    }

    async fn index_path(&self, tip: Arc<SubstrateBlock>) -> Result<()> {
        let mut path = Vec::new();
        let mut next = tip.clone();
        while !self.is_indexed(&next).await? {
            if next.number() == 0 || path.len() >= RETAINED_BLOCKS {
                return Err(ClientError::SyncBoundaryMismatch);
            }
            let parent = self
                .blocks
                .block_by_hash(&next.header().parent_hash)
                .await?
                .ok_or(ClientError::BlockNotFound)?;
            if parent.number().checked_add(1) != Some(next.number()) {
                return Err(ClientError::SyncBoundaryMismatch);
            }
            path.push(next);
            next = parent;
        }
        for block in path.into_iter().rev() {
            self.index(&block).await?;
        }
        Ok(())
    }

    async fn resolve_number(&self, tag: BlockNumberOrTag) -> Result<u32> {
        match tag {
            BlockNumberOrTag::U256(n) => n.try_into().map_err(|_| ClientError::ConversionFailed),
            BlockNumberOrTag::BlockTag(BlockTag::Earliest) => {
                Ok(self.receipts.first_evm_block().unwrap_or(0))
            }
            BlockNumberOrTag::BlockTag(BlockTag::Finalized | BlockTag::Safe) => {
                Ok(self.blocks.latest_finalized_block().await.number())
            }
            BlockNumberOrTag::BlockTag(BlockTag::Latest | BlockTag::Pending) => {
                Ok(self.blocks.latest_block().await.number())
            }
        }
    }

    async fn resolve_hash(&self, at: BlockNumberOrTagOrHash) -> Result<H256> {
        match at {
            BlockNumberOrTagOrHash::BlockHash(hash) => self
                .receipts
                .get_substrate_hash(&hash)
                .await
                .ok_or(ClientError::EthereumBlockNotFound),
            BlockNumberOrTagOrHash::BlockNumber(number) => {
                let n: u32 = number
                    .try_into()
                    .map_err(|_| ClientError::ConversionFailed)?;
                self.blocks
                    .legacy
                    .chain_get_block_hash(Some(n.into()))
                    .await?
                    .ok_or(ClientError::BlockNotFound)
            }
            BlockNumberOrTagOrHash::BlockTag(tag) => {
                let n = self.resolve_number(tag.into()).await?;
                self.blocks
                    .legacy
                    .chain_get_block_hash(Some(n.into()))
                    .await?
                    .ok_or(ClientError::BlockNotFound)
            }
        }
    }

    async fn by_number(&self, at: BlockNumberOrTag) -> Result<Option<Arc<SubstrateBlock>>> {
        self.blocks
            .block_by_number(self.resolve_number(at).await?)
            .await
    }

    async fn by_ethereum_hash(&self, hash: H256) -> Result<Option<Arc<SubstrateBlock>>> {
        match self.receipts.get_substrate_hash(&hash).await {
            Some(hash) => self.blocks.block_by_hash(&hash).await,
            None => self.blocks.block_by_hash(&hash).await,
        }
    }

    async fn block(&self, block: Arc<SubstrateBlock>, hydrated: bool) -> Result<Option<Block>> {
        if self
            .receipts
            .first_evm_block()
            .is_some_and(|first| block.number() < first)
        {
            return Ok(None);
        }
        let Some(mut eth) = self.checked(&block).await? else {
            return Ok(None);
        };
        if hydrated {
            eth.transactions = HashesOrTransactionInfos::TransactionInfos(
                self.receipts
                    .receipts_from_block(&block, eth.hash)
                    .await?
                    .into_iter()
                    .map(|(signed, receipt)| TransactionInfo::new(&receipt, signed))
                    .collect(),
            );
        }
        Ok(Some(eth))
    }

    async fn transaction(
        &self,
        block: Arc<SubstrateBlock>,
        index: U256,
    ) -> Result<Option<TransactionInfo>> {
        let index: usize = index
            .try_into()
            .map_err(|_| ClientError::ConversionFailed)?;
        let Some(receipt) = self
            .receipts
            .receipt_by_block_hash_and_index(&block.hash(), index)
            .await
        else {
            return Ok(None);
        };
        Ok(self
            .receipts
            .signed_tx_by_hash(&receipt.transaction_hash)
            .await
            .map(|signed| TransactionInfo::new(&receipt, signed)))
    }
}

/// 查询按指定状态执行；链上API负责执行、收费和资源检查。
fn dry_run_config(
    pending: bool,
    overrides: Option<StateOverrideSet>,
) -> pallet_revive::DryRunConfig<u64> {
    pallet_revive::DryRunConfig::default()
        .with_timestamp_override(pending.then(|| sp_timestamp::Timestamp::current().as_millis()))
        .with_state_overrides(overrides)
}

#[async_trait]
impl EthRpcServer for Ethereum {
    async fn accounts(&self) -> RpcResult<Vec<H160>> {
        Ok(vec![])
    }
    async fn chain_id(&self) -> RpcResult<U256> {
        Ok(primitives::core_const::ETHEREUM_CHAIN_ID.into())
    }
    async fn net_version(&self) -> RpcResult<String> {
        Ok(primitives::core_const::ETHEREUM_CHAIN_ID.to_string())
    }
    async fn web3_client_version(&self) -> RpcResult<String> {
        Ok(format!(
            "citizenchain/{}",
            env!("CITIZENCHAIN_RELEASE_VERSION")
        ))
    }
    async fn block_number(&self) -> RpcResult<U256> {
        Ok(self.blocks.latest_block().await.number().into())
    }
    async fn syncing(&self) -> RpcResult<SyncingStatus> {
        let health = self
            .blocks
            .legacy
            .system_health()
            .await
            .map_err(ClientError::from)?;
        if !health.is_syncing {
            return Ok(SyncingStatus::Bool(false));
        }
        let state = self
            .native
            .request("system_syncState", serde_json::json!([]))
            .await?;
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SyncState {
            current_block: u32,
            highest_block: u32,
            starting_block: u32,
        }
        let state: SyncState =
            serde_json::from_value(state).map_err(|error| failure(format!("同步状态: {error}")))?;
        Ok(SyncingProgress {
            current_block: Some(state.current_block.into()),
            highest_block: Some(state.highest_block.into()),
            starting_block: Some(state.starting_block.into()),
        }
        .into())
    }
    async fn net_listening(&self) -> RpcResult<bool> {
        Ok(matches!(self.syncing().await?, SyncingStatus::Bool(false)))
    }
    async fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
        Ok(U256::zero())
    }
    async fn gas_price(&self) -> RpcResult<U256> {
        Ok(self
            .native
            .runtime(
                self.resolve_hash(BlockTag::Latest.into()).await?,
                "gas_price",
                vec![],
            )
            .await?)
    }
    async fn get_balance(&self, address: H160, at: BlockNumberOrTagOrHash) -> RpcResult<U256> {
        // 映射及账户必须读取同一请求块；仅展示总余额，不改变 Runtime 的可支配余额 API。
        let at = self.resolve_hash(at).await?;
        let account = account_at(&self.native.0, address, Some(at)).await?;
        let key = nodeclient::query::system_account_key(&format!("0x{}", hex::encode(account)))
            .map_err(failure)?;
        let raw = native_request(&self.native.0, "state_getStorage", serde_json::json!([key, at]))
            .await?;
        let raw: Option<sp_core::Bytes> = serde_json::from_value(raw)
            .map_err(|error| failure(format!("账户总余额存储响应无效: {error}")))?;
        let total = match raw {
            Some(bytes) => {
                let account = nodeclient::codec::decode_account(&bytes.0).map_err(failure)?;
                nodeclient::codec::account_total(&account).map_err(failure)?
            }
            None => 0,
        };
        Ok(U256::from(total) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO))
    }
    async fn get_code(&self, address: H160, at: BlockNumberOrTagOrHash) -> RpcResult<Bytes> {
        let code: Vec<u8> = self
            .native
            .runtime(self.resolve_hash(at).await?, "code", address.encode())
            .await?;
        Ok(code.into())
    }
    async fn get_transaction_count(
        &self,
        address: H160,
        at: BlockNumberOrTagOrHash,
    ) -> RpcResult<U256> {
        if matches!(at, BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending)) {
            let account = account_at(&self.native.0, address, None).await?;
            let value = self
                .native
                .request("system_accountNextIndex", serde_json::json!([account]))
                .await?;
            let nonce: u32 = serde_json::from_value(value)
                .map_err(|error| failure(format!("交易池nonce: {error}")))?;
            return Ok(nonce.into());
        }
        let nonce: u32 = self
            .native
            .runtime(self.resolve_hash(at).await?, "nonce", address.encode())
            .await?;
        Ok(nonce.into())
    }
    async fn get_storage_at(
        &self,
        address: H160,
        slot: U256,
        at: BlockNumberOrTagOrHash,
    ) -> RpcResult<Bytes> {
        let value: pallet_revive::GetStorageResult = self
            .native
            .runtime(
                self.resolve_hash(at).await?,
                "get_storage",
                (address, slot.to_big_endian()).encode(),
            )
            .await?;
        // Ethereum读取非合约或未设置槽位时保持32字节零值；传输及解码错误仍拒绝。
        Ok(value.ok().flatten().unwrap_or_else(|| vec![0; 32]).into())
    }
    async fn estimate_gas(
        &self,
        tx: GenericTransaction,
        at: Option<BlockNumberOrTag>,
    ) -> RpcResult<U256> {
        let at: BlockNumberOrTagOrHash = at.unwrap_or_default().into();
        let config = dry_run_config(
            matches!(at, BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending)),
            None,
        );
        let value: Result<U256, pallet_revive::EthTransactError> = self
            .native
            .runtime(
                self.resolve_hash(at).await?,
                "eth_estimate_gas",
                (tx, config).encode(),
            )
            .await?;
        Ok(value.map_err(ClientError::TransactError)?)
    }
    async fn call(
        &self,
        tx: GenericTransaction,
        at: Option<BlockNumberOrTagOrHash>,
        overrides: Option<StateOverrideSet>,
    ) -> RpcResult<Bytes> {
        let at = at.unwrap_or_default();
        let config = dry_run_config(
            matches!(at, BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending)),
            overrides,
        );
        let value: Result<pallet_revive::EthTransactInfo<u128>, pallet_revive::EthTransactError> =
            self.native
                .runtime(
                    self.resolve_hash(at).await?,
                    "eth_transact_with_config",
                    (tx, config).encode(),
                )
                .await?;
        Ok(value.map_err(ClientError::TransactError)?.data.into())
    }
    async fn send_raw_transaction(&self, payload: Bytes) -> RpcResult<H256> {
        let signed =
            TransactionSigned::decode(&payload.0).map_err(|_| EthRpcError::InvalidTransaction)?;
        let transaction = GenericTransaction::from_signed(signed, U256::zero(), None);
        // 维持禁止未保护交易的既有规则；不同Ethereum交易类型按公开EIP编码识别。
        if transaction.chain_id.is_none()
            || (payload.0.first().is_some_and(|b| *b < 0x80)
                && transaction.chain_id == Some(U256::zero()))
        {
            return Err(EthRpcError::InvalidTransaction.into());
        }
        let hash = H256(sp_crypto_hashing::keccak_256(&payload.0));
        let call =
            runtime::RuntimeCall::Revive(pallet_revive::Call::eth_transact { payload: payload.0 });
        let xt: runtime::UncheckedExtrinsic =
            sp_runtime::generic::UncheckedExtrinsic::new_bare(call).into();
        use subxt::backend::legacy::rpc_methods::TransactionStatus;
        use subxt::ext::subxt_rpcs::rpc_params;
        let mut status = self
            .blocks
            .rpc
            .subscribe::<TransactionStatus<H256>>(
                "author_submitAndWatchExtrinsic",
                rpc_params![format!("0x{}", hex::encode(xt.encode()))],
                "author_unwatchExtrinsic",
            )
            .await
            .map_err(ClientError::from)?;
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), status.next())
            .await
            .map_err(|_| ClientError::Timeout)?
            .ok_or(ClientError::SubmitError(
                pallet_revive_eth_rpc::client::SubmitError::StreamEnded,
            ))?
            .map_err(ClientError::from)?;
        if matches!(
            event,
            TransactionStatus::Usurped(_) | TransactionStatus::Dropped | TransactionStatus::Invalid
        ) {
            return Err(ClientError::SubmitError(event.into()).into());
        }
        Ok(hash)
    }
    async fn send_transaction(&self, transaction: GenericTransaction) -> RpcResult<H256> {
        match transaction.from {
            Some(account) => Err(EthRpcError::AccountNotFound(account).into()),
            None => Err(EthRpcError::InvalidTransaction.into()),
        }
    }
    async fn get_block_by_number(
        &self,
        at: BlockNumberOrTag,
        full: bool,
    ) -> RpcResult<Option<Block>> {
        match self.by_number(at).await? {
            Some(block) => Ok(self.block(block, full).await?),
            None => Ok(None),
        }
    }
    async fn get_block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<Block>> {
        match self.by_ethereum_hash(hash).await? {
            Some(block) => Ok(self.block(block, full).await?),
            None => Ok(None),
        }
    }
    async fn get_block_transaction_count_by_hash(
        &self,
        hash: Option<H256>,
    ) -> RpcResult<Option<U256>> {
        let hash = match hash {
            Some(hash) => hash,
            None => self.blocks.latest_block().await.hash(),
        };
        let Some(hash) = self.receipts.get_substrate_hash(&hash).await else {
            return Ok(None);
        };
        Ok(self
            .receipts
            .receipts_count_per_block(&hash)
            .await
            .map(U256::from))
    }
    async fn get_block_transaction_count_by_number(
        &self,
        at: Option<BlockNumberOrTag>,
    ) -> RpcResult<Option<U256>> {
        match self.by_number(at.unwrap_or_default()).await? {
            Some(block) => Ok(self
                .receipts
                .receipts_count_per_block(&block.hash())
                .await
                .map(U256::from)),
            None => Ok(None),
        }
    }
    async fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<ReceiptInfo>> {
        Ok(self.receipts.receipt_by_hash(&hash).await)
    }
    async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<TransactionInfo>> {
        let Some(receipt) = self.receipts.receipt_by_hash(&hash).await else {
            return Ok(None);
        };
        Ok(self
            .receipts
            .signed_tx_by_hash(&hash)
            .await
            .map(|signed| TransactionInfo::new(&receipt, signed)))
    }
    async fn get_transaction_by_block_hash_and_index(
        &self,
        hash: H256,
        index: U256,
    ) -> RpcResult<Option<TransactionInfo>> {
        let Some(hash) = self.receipts.get_substrate_hash(&hash).await else {
            return Ok(None);
        };
        match self.blocks.block_by_hash(&hash).await? {
            Some(block) => Ok(self.transaction(block, index).await?),
            None => Ok(None),
        }
    }
    async fn get_transaction_by_block_number_and_index(
        &self,
        at: BlockNumberOrTag,
        index: U256,
    ) -> RpcResult<Option<TransactionInfo>> {
        match self.by_number(at).await? {
            Some(block) => Ok(self.transaction(block, index).await?),
            None => Ok(None),
        }
    }
    async fn get_logs(&self, filter: Option<Filter>) -> RpcResult<FilterResults> {
        // 把标签一次解析到当前快照，SQL查询与SDK公开索引使用同一高度口径。
        let best = self.blocks.latest_block().await.number();
        let finalized = self.blocks.latest_finalized_block().await.number();
        let first = self.receipts.first_evm_block().unwrap_or(0);
        let logs = self
            .receipts
            .logs(filter, move |at| {
                let n = match at {
                    BlockNumberOrTag::U256(n) => return Ok(n),
                    BlockNumberOrTag::BlockTag(BlockTag::Earliest) => first,
                    BlockNumberOrTag::BlockTag(BlockTag::Finalized | BlockTag::Safe) => finalized,
                    BlockNumberOrTag::BlockTag(BlockTag::Latest | BlockTag::Pending) => best,
                };
                Ok(n.into())
            })
            .await
            .map_err(ClientError::from)?;
        // 通知处理可能落后于真实最佳链；与回执查询一样，逐高度核对规范链，
        // 不把尚未裁剪的旧分支日志暴露给调用方。读取失败必须传播。
        let mut canonical = std::collections::BTreeMap::new();
        let mut result = Vec::with_capacity(logs.len());
        for log in logs {
            let number: u32 = log
                .block_number
                .try_into()
                .map_err(|_| ClientError::ConversionFailed)?;
            let hash = match canonical.get(&number) {
                Some(hash) => *hash,
                None => {
                    let block = self
                        .blocks
                        .block_by_number(number)
                        .await?
                        .ok_or(ClientError::BlockNotFound)?;
                    let hash = self.checked(&block).await?.map(|block| block.hash);
                    canonical.insert(number, hash);
                    hash
                }
            };
            if hash == Some(log.block_hash) {
                result.push(log);
            }
        }
        Ok(FilterResults::Logs(result))
    }
    async fn fee_history(
        &self,
        count: U256,
        at: BlockNumberOrTag,
        percentiles: Option<Vec<f64>>,
    ) -> RpcResult<FeeHistoryResult> {
        let count: u32 = count
            .try_into()
            .map_err(|_| ClientError::ConversionFailed)?;
        Ok(self
            .fees
            .fee_history(count, self.resolve_number(at).await?, percentiles)
            .await?)
    }
    async fn eth_subscribe(
        &self,
        pending: PendingSubscriptionSink,
        kind: SubscriptionKind,
        options: Option<SubscriptionOptions>,
    ) {
        let Some(parameters) = SubscriptionParameters::new(kind, options) else {
            pending
                .reject(ErrorObjectOwned::owned(
                    -32602,
                    "无效 Ethereum 订阅参数",
                    None::<()>,
                ))
                .await;
            return;
        };
        let mut receiver: futures::stream::BoxStream<
            'static,
            Result<SubscriptionItem, broadcast::error::RecvError>,
        > = match parameters {
            SubscriptionParameters::NewBlockHeaders => notices(self.heads.subscribe())
                .map(|item| {
                    item.map(|block| SubscriptionItem::BlockHeader(BlockHeader::from(block)))
                })
                .boxed(),
            SubscriptionParameters::Logs(filter) => notices(self.logs.subscribe())
                .filter_map(move |item| {
                    futures::future::ready(match item {
                        Ok(log) if filter.matches(&log) => Some(Ok(SubscriptionItem::Log(log))),
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                })
                .boxed(),
        };
        let Ok(sink) = pending.accept().await else {
            return;
        };
        loop {
            let message = tokio::select! {
                _ = sink.closed() => break,
                message = receiver.next() => message,
            };
            let item = match message {
                Some(Ok(message)) => message,
                None | Some(Err(broadcast::error::RecvError::Closed)) => break,
                Some(Err(broadcast::error::RecvError::Lagged(n))) => {
                    log::warn!("Ethereum最终块订阅落后{n}条通知");
                    continue;
                }
            };
            let message = match SubscriptionMessage::from_json(&item) {
                Ok(message) => message,
                Err(error) => {
                    log::error!("Ethereum通知编码失败: {error}");
                    return;
                }
            };
            if sink.send(message).await.is_err() {
                return;
            }
        }
    }
}

fn notices<T: Clone + Send + 'static>(
    receiver: broadcast::Receiver<T>,
) -> impl futures::Stream<Item = Result<T, broadcast::error::RecvError>> + Send {
    futures::stream::unfold(receiver, |mut receiver| async move {
        match receiver.recv().await {
            Err(broadcast::error::RecvError::Closed) => None,
            item => Some((item, receiver)),
        }
    })
}

/// 旧前缀只认可公开合同中的准确缺API错误；其他错误继续向必要任务传播。
fn before_revive(error: &ClientError) -> bool {
    match error {
        ClientError::Jsonrpsee(jsonrpsee::core::ClientError::Call(error)) =>
            error.code() == 4003 && error.data().is_none() && matches!(error.message(),
                "Exported method ReviveApi_eth_block is not found" |
                "Client error: Execution failed: Other: Exported method ReviveApi_eth_block is not found"),
        _ => false,
    }
}

impl Ethereum {
    async fn rebuild(&self) -> Result<()> {
        let last = self.blocks.latest_finalized_block().await.number();
        let first = last.saturating_sub((RETAINED_BLOCKS - 1) as u32);
        for number in first..=last {
            let block = self
                .blocks
                .block_by_number(number)
                .await?
                .ok_or(ClientError::BlockNotFound)?;
            match self.index(&block).await {
                Ok(_) => {}
                Err(error)
                    if number < last
                        && self.receipts.first_evm_block().is_none()
                        && before_revive(&error) =>
                {
                    *self.prefix.lock().await = Some((number, block.hash()));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn finalized(&self, block: Arc<SubstrateBlock>) -> Result<()> {
        let previous = self.blocks.latest_finalized_block().await;
        if block.number() < previous.number() {
            return Ok(());
        }
        if block.number() == previous.number() {
            return if block.hash() == previous.hash() {
                Ok(())
            } else {
                Err(ClientError::SyncBoundaryMismatch)
            };
        }
        self.index_path(block.clone()).await?;
        if self.blocks.latest_block().await.number() < block.number() {
            self.blocks
                .update_latest(block.clone(), SubscriptionType::BestBlocks)
                .await;
        }
        let mut parent = previous.hash();
        // 最终性跳高逐块核实并发布，循环不收集整段数据；近期存储仍由SDK限定256块。
        for number in previous.number() + 1..=block.number() {
            let next = self
                .blocks
                .block_by_number(number)
                .await?
                .ok_or(ClientError::BlockNotFound)?;
            if next.header().parent_hash != parent
                || (number == block.number() && next.hash() != block.hash())
            {
                return Err(ClientError::SyncBoundaryMismatch);
            }
            let eth = self.checked(&next).await?;
            let publication = if let Some(eth) = eth {
                let receipts = self.receipts.receipts_from_block(&next, eth.hash).await?;
                Some((
                    eth,
                    receipts
                        .into_iter()
                        .flat_map(|(_, receipt)| receipt.logs)
                        .collect::<Vec<_>>(),
                ))
            } else {
                None
            };
            parent = next.hash();
            self.blocks
                .update_latest(next, SubscriptionType::FinalizedBlocks)
                .await;
            if let Some((_, logs)) = publication {
                for log in logs {
                    let _ = self.logs.send(log);
                }
            }
        }
        Ok(())
    }
}

/// 公民链只接管读取上下文与RPC编排；框架公开的回执、分叉裁剪及费用组件保持原样。
pub(crate) async fn initialize(
    native: RpcModule<()>,
) -> std::result::Result<
    (RpcModule<()>, BoxFuture<'static, Result<()>>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    use subxt::ext::subxt_rpcs::rpc_params;
    let native = Native(Arc::new(native));
    let blocks = Blocks::new(&native).await?;
    let api = OnlineClient::<SrcChainConfig>::from_rpc_client(blocks.rpc.clone()).await?;
    // Native会话内保持同一链身份；正式冻结身份已由原NodeGuard核验。
    // Ethereum公开适配同样用于隔离验收链，不另加固定创世限制。
    if api.genesis_hash() != blocks.genesis {
        return Err(ClientError::ChainMismatch.into());
    }
    let id: u64 = api
        .constants()
        .at(&pallet_revive_eth_rpc::subxt_client::constants()
            .revive()
            .chain_id()
            .unvalidated())?;
    if id != primitives::core_const::ETHEREUM_CHAIN_ID {
        return Err(ClientError::ChainMismatch.into());
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await?;
    let receipts = ReceiptProvider::new(
        DbContext::new(pool, DbContext::DEFAULT_MAX_VARIABLE_NUMBER),
        blocks.clone(),
        ReceiptExtractor::new(api).await?,
        Some(RETAINED_BLOCKS),
    )
    .await?;
    let ethereum = Ethereum {
        native,
        blocks,
        receipts,
        fees: FeeHistoryProvider::default(),
        prefix: Arc::new(Mutex::new(None)),
        heads: broadcast::channel(16).0,
        logs: broadcast::channel(16).0,
    };
    // 先接收通知，再重建快照；重建期间的新块由同一有界订阅保留。
    let mut best = ethereum
        .blocks
        .rpc
        .subscribe::<runtime::opaque::Header>(
            "chain_subscribeNewHeads",
            rpc_params![],
            "chain_unsubscribeNewHeads",
        )
        .await?;
    let mut finalized = ethereum
        .blocks
        .rpc
        .subscribe::<runtime::opaque::Header>(
            "chain_subscribeFinalizedHeads",
            rpc_params![],
            "chain_unsubscribeFinalizedHeads",
        )
        .await?;
    ethereum.rebuild().await?;
    let mut module = ethereum.clone().into_rpc();
    module.remove_method("eth_subscribe");
    module.remove_method("eth_unsubscribe");
    let subscriptions = ethereum.clone();
    module.register_subscription(
        "eth_subscribe",
        "eth_subscription",
        "eth_unsubscribe",
        move |params, pending, _, _| {
            let subscriptions = subscriptions.clone();
            async move {
                match subscription_request(&params) {
                    Ok((kind, options)) => {
                        subscriptions.eth_subscribe(pending, kind, options).await
                    }
                    Err(error) => pending.reject(error).await,
                }
            }
        },
    )?;
    let module = module.remove_context();
    let task = async move {
        loop {
            let (header, finality) = tokio::select! {
                header = best.next() => (header,false),
                header = finalized.next() => (header,true),
            };
            let header = header.ok_or(ClientError::BlockNotFound)??;
            let block = ethereum
                .blocks
                .block_by_hash(&header.hash())
                .await?
                .ok_or(ClientError::BlockNotFound)?;
            if finality {
                ethereum.finalized(block).await?;
            } else {
                ethereum.index_path(block.clone()).await?;
                let head = ethereum.checked(&block).await?;
                ethereum
                    .blocks
                    .update_latest(block, SubscriptionType::BestBlocks)
                    .await;
                if let Some(head) = head {
                    let _ = ethereum.heads.send(head);
                }
            }
        }
    }
    .boxed();
    Ok((module, task))
}

#[cfg(test)]
#[path = "tests/ethereum.rs"]
mod tests;
