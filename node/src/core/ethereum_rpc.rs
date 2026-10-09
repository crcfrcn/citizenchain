//! 官方 Ethereum RPC 的节点内接入。原生 RPC 是唯一链后端，不另建监听或交易池。

use codec::DecodeAll;
use futures::future::BoxFuture;
use jsonrpsee::{types::ErrorObjectOwned, RpcModule};
use pallet_revive::{
    evm::{BlockNumberOrTagOrHash, BlockTag, SubscriptionKind, SubscriptionOptions},
    AddressMapper,
};
use pallet_revive_eth_rpc::{
    client::{Client, ClientError},
    EthRpcServer, EthRpcServerImpl,
};
use sp_core::{H160, U256};

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
            pallet_revive::AccountId32Mapper::<citizenchain::Runtime>::to_fallback_account_id(
                &address,
            ),
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
                let nonce: citizenchain::Nonce = serde_json::from_value(result).map_err(|_| {
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

/// 真实节点服务验收：WASM 客户端、正式交易池、两层导入守卫及 HTTPS/WSS 共用一条链。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::service::{FullBackend, FullClient, Service};
    use citizenchain::opaque::Block;
    use codec::Encode;
    use jsonrpsee::core::client::SubscriptionClientT;
    use pallet_revive::evm::{Account, Bytes, GenericTransaction};
    use sc_chain_spec::{ChainType, Properties};
    use sc_consensus::{
        BlockImport, BlockImportParams, ForkChoiceStrategy, ImportResult, StateAction,
        StorageChanges,
    };
    use sc_consensus_pow::PowAlgorithm;
    use sc_network::{
        config::{MultiaddrWithPeerId, NetworkConfiguration},
        service::traits::{NetworkBlock, NetworkPeers, NetworkService, NetworkStateInfo},
        NetworkBackend as _,
    };
    use sc_service::{
        config::{ExecutorConfiguration, KeystoreConfig, RpcBatchRequestConfig, RpcConfiguration},
        BasePath, BlocksPruning, Configuration, DatabaseSource, PruningMode, Role,
        TransactionPoolOptions,
    };
    use sp_blockchain::HeaderBackend;
    use sp_core::{
        crypto::{Ss58AddressFormat, Ss58Codec},
        sr25519, Pair,
    };
    use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
    use sp_runtime::{traits::Block as BlockT, Digest, DigestItem, OpaqueExtrinsic};
    use std::{sync::Arc, time::Duration};

    fn chain_spec() -> crate::core::chain_spec::ChainSpec {
        chain_spec_with_accounts(&[])
    }

    /// 只在测试创世中预置公开签名夹具；原生账户继续使用官方 Revive 映射。
    fn chain_spec_with_accounts(
        accounts: &[(sp_runtime::AccountId32, u128)],
    ) -> crate::core::chain_spec::ChainSpec {
        let wasm = citizenchain::WASM_BINARY.expect("必须执行真实源码 WASM，禁止跳过");
        let mut genesis = citizenchain::genesis::genesis_config();
        // 仅替换隔离夹具的 GRANDPA 权威，使用公开测试密钥完成真实投票。
        // 正式创世、链身份和 Runtime 源码不从此测试配置取得。
        let authority = sp_consensus_grandpa::AuthorityId::from(Ed25519Keyring::Alice.public());
        genesis["grandpa"]["authorities"] = serde_json::json!([[
            authority.to_ss58check_with_version(Ss58AddressFormat::custom(
                primitives::core_const::SS58_FORMAT
            )),
            1,
        ]]);
        for account in [
            Account::default().substrate_account(),
            Sr25519Keyring::Alice.to_account_id(),
        ] {
            genesis["balances"]["balances"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!([
                    account.to_ss58check_with_version(Ss58AddressFormat::custom(
                        primitives::core_const::SS58_FORMAT
                    )),
                    1_000_000_000_000u128,
                ]));
        }
        for (account, balance) in accounts {
            let address = account.to_ss58check_with_version(Ss58AddressFormat::custom(
                primitives::core_const::SS58_FORMAT,
            ));
            let balances = genesis["balances"]["balances"].as_array_mut().unwrap();
            balances.retain(|entry| entry[0] != address);
            balances.push(serde_json::json!([address, balance]));
        }
        let mut properties = Properties::new();
        properties.insert(
            "ss58Format".into(),
            serde_json::json!(primitives::core_const::SS58_FORMAT),
        );
        properties.insert("tokenDecimals".into(), serde_json::json!(2));
        properties.insert("tokenSymbol".into(), serde_json::json!("GMB"));
        crate::core::chain_spec::ChainSpec::builder(wasm, None)
            .with_name("Ethereum RPC acceptance")
            .with_id("ethereum-rpc-acceptance")
            .with_chain_type(ChainType::Development)
            .with_protocol_id("ethereum-rpc-acceptance")
            .with_properties(properties)
            .with_genesis_config_patch(genesis)
            .build()
    }
    fn test_config(node_name: &str, tokio_handle: tokio::runtime::Handle) -> Configuration {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gmb-ethereum-rpc-{node_name}-{}-{unique}",
            std::process::id()
        ));
        test_config_at(node_name, tokio_handle, root)
    }

    /// 调用方交付本产品生成目录；每个节点仍拥有独立数据库与网络身份。
    fn test_config_at(
        node_name: &str,
        tokio_handle: tokio::runtime::Handle,
        root: std::path::PathBuf,
    ) -> Configuration {
        std::fs::create_dir_all(&root).expect("创建本轮 Ethereum RPC 隔离验收目录");
        let base_path = BasePath::new(root.clone());
        let mut network = NetworkConfiguration::new(
            node_name,
            "citizenchain-ethereum-rpc-test/0.1",
            Default::default(),
            None,
        );
        network.allow_non_globals_in_dht = true;
        network
            .listen_addresses
            .push("/ip4/127.0.0.1/tcp/0".parse().expect("test listen address"));

        Configuration {
            impl_name: "citizenchain-ethereum-rpc-test".into(),
            impl_version: "0.1".into(),
            role: Role::Full,
            tokio_handle,
            transaction_pool: TransactionPoolOptions::default(),
            network,
            keystore: KeystoreConfig::InMemory,
            database: DatabaseSource::RocksDb {
                path: root.join("db"),
                cache_size: 128,
            },
            trie_cache_maximum_size: Some(16 * 1024 * 1024),
            warm_up_trie_cache: None,
            state_pruning: Some(PruningMode::ArchiveAll),
            blocks_pruning: BlocksPruning::KeepAll,
            chain_spec: Box::new(chain_spec()),
            executor: ExecutorConfiguration::default(),
            wasm_runtime_overrides: None,
            rpc: RpcConfiguration {
                addr: None,
                max_connections: Default::default(),
                cors: None,
                methods: Default::default(),
                max_request_size: Default::default(),
                max_response_size: Default::default(),
                id_provider: Default::default(),
                max_subs_per_conn: Default::default(),
                port: 9944,
                message_buffer_capacity: Default::default(),
                batch_config: RpcBatchRequestConfig::Unlimited,
                rate_limit: None,
                rate_limit_whitelisted_ips: Default::default(),
                rate_limit_trust_proxy_headers: Default::default(),
                request_logger_limit: 1024,
            },
            prometheus_config: None,
            telemetry_endpoints: None,
            offchain_worker: Default::default(),
            force_authoring: false,
            disable_grandpa: true,
            dev_key_seed: None,
            tracing_targets: None,
            tracing_receiver: Default::default(),
            announce_block: true,
            data_path: root,
            base_path,
        }
    }

    fn native(partial: &Service) -> RpcModule<()> {
        use sc_rpc_api::{author::AuthorApiServer, chain::ChainApiServer, state::StateApiServer};
        let spawn: Arc<dyn sp_core::traits::SpawnNamed> =
            Arc::new(partial.task_manager.spawn_handle());
        let mut module = RpcModule::new(());
        module
            .merge(sc_rpc::chain::new_full(partial.client.clone(), spawn.clone()).into_rpc())
            .unwrap();
        let (state, _) = sc_rpc::state::new_full(partial.client.clone(), spawn.clone(), None);
        module.merge(state.into_rpc()).unwrap();
        module
            .merge(
                sc_rpc::author::Author::new(
                    partial.client.clone(),
                    partial.transaction_pool.clone(),
                    partial.keystore_container.keystore(),
                    spawn,
                )
                .into_rpc(),
            )
            .unwrap();
        module
            .merge(
                crate::core::rpc::create_full(crate::core::rpc::FullDeps {
                    client: partial.client.clone(),
                    pool: partial.transaction_pool.clone(),
                    keystore: partial.keystore_container.keystore(),
                    cpu_hashrate_fn: || 0.0,
                    gpu_hashrate_fn: None,
                    offchain_clearing_rpc: None,
                })
                .unwrap(),
            )
            .unwrap();
        module.extensions_mut().insert(sc_rpc_api::DenyUnsafe::No);
        module
    }

    fn endpoint(address: std::net::SocketAddr) -> sc_rpc_server::RpcEndpoint {
        sc_rpc_server::RpcEndpoint {
            listen_addr: address,
            batch_config: jsonrpsee::server::BatchRequestConfig::Unlimited,
            max_connections: 32,
            max_payload_in_mb: 2,
            max_payload_out_mb: 2,
            max_subscriptions_per_connection: 16,
            max_buffer_capacity_per_connection: 64,
            rate_limit: None,
            rate_limit_trust_proxy_headers: false,
            rate_limit_whitelisted_ips: vec![],
            cors: Some(vec!["https://localhost".into()]),
            rpc_methods: sc_rpc_server::RpcMethods::Safe,
            is_optional: false,
            retry_random_port: false,
        }
    }

    async fn serve(
        partial: &Service,
        credentials: &rcgen::CertifiedKey,
    ) -> (
        String,
        tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    ) {
        serve_on_port(partial, credentials, 0).await
    }

    /// 显式会话使用固定端口；占用时失败，普通自动化夹具继续使用随机端口。
    async fn serve_on_port(
        partial: &Service,
        credentials: &rcgen::CertifiedKey,
        port: u16,
    ) -> (
        String,
        tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    ) {
        let mut module = native(partial);
        let (ethereum, synchronization) = initialize(module.clone()).await.unwrap();
        module.merge(ethereum).unwrap();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .expect("测试 RPC 端口被占用或不能绑定");
        let address = listener.local_addr().unwrap();
        drop(listener);
        let tls = crate::core::rpc_tls::RpcTls::from_der(
            vec![credentials.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(credentials.key_pair.serialize_der()),
        )
        .unwrap();
        let task = tokio::spawn(async move {
            tokio::try_join!(
                async {
                    synchronization
                        .await
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
                },
                tls.run(vec![endpoint(address)], module)
            )?;
            Ok(())
        });
        (format!("https://localhost:{}", address.port()), task)
    }

    async fn response(
        http: &reqwest::Client,
        url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match http.post(url).json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})).send().await {
                    Ok(response) => return response.json().await.unwrap(),
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        }).await.expect("真实 TLS 请求超时")
    }
    async fn rpc(
        http: &reqwest::Client,
        url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        let response = response(http, url, method, params).await;
        assert!(response.get("error").is_none(), "{method}: {response}");
        response["result"].clone()
    }
    async fn wait_block(http: &reqwest::Client, url: &str, number: u32) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while rpc(http, url, "eth_blockNumber", serde_json::json!([])).await
                != format!("0x{number:x}")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("索引必须追上真实最佳链");
    }
    fn transaction(
        nonce: u32,
        to: Option<H160>,
        input: Vec<u8>,
        value: u128,
    ) -> (String, OpaqueExtrinsic) {
        transaction_from(&Account::default(), nonce, to, input, value)
    }

    fn transaction_from(
        account: &Account,
        nonce: u32,
        to: Option<H160>,
        input: Vec<u8>,
        value: u128,
    ) -> (String, OpaqueExtrinsic) {
        let fee_gas = pallet_revive::evm::fees::native_fee_to_gas::<citizenchain::Runtime>(
            primitives::fee_policy::calculate_onchain_fee(value),
        ).expect("测试金额必须有可表达的费用 gas");
        let tx = GenericTransaction {
            from: Some(account.address()),
            to,
            nonce: Some(nonce.into()),
            chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
            gas: Some(100_000_000u64.max(fee_gas).into()),
            // 测试签名复用已消费 SDK 的钱包报价，不能手填旧的一分 gas 价格掩盖问题。
            gas_price: Some(pallet_revive::Pallet::<citizenchain::Runtime>::evm_base_fee()),
            value: Some(
                U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
            ),
            input: Bytes(input).into(),
            r#type: Some(pallet_revive::evm::TYPE_LEGACY.into()),
            ..Default::default()
        };
        signed_transaction(account, tx)
    }

    /// 同一真实签名与原生包装供固定夹具及钱包 RPC 报价交易使用。
    fn signed_transaction(account: &Account, tx: GenericTransaction) -> (String, OpaqueExtrinsic) {
        let payload = account
            .sign_transaction(tx.try_into_unsigned().unwrap())
            .signed_payload();
        let xt: citizenchain::UncheckedExtrinsic =
            sp_runtime::generic::UncheckedExtrinsic::new_bare(citizenchain::RuntimeCall::Revive(
                pallet_revive::Call::eth_transact {
                    payload: payload.clone(),
                },
            ))
            .into();
        (format!("0x{}", hex::encode(payload)), xt.into())
    }
    /// 按实际区块读取 Runtime 事件，费用与新合约 ED 转移分别验证。
    fn paid_fees(client: &FullClient, hash: sp_core::H256) -> Vec<(sp_runtime::AccountId32, u128)> {
        use sc_client_api::StorageProvider;
        let key = frame_support::storage::storage_prefix(b"System", b"Events");
        let data = client
            .storage(hash, &sp_storage::StorageKey(key.to_vec()))
            .unwrap()
            .unwrap();
        Vec::<frame_system::EventRecord<citizenchain::RuntimeEvent, sp_core::H256>>::decode_all(
            &mut &data.0[..],
        )
        .unwrap()
        .into_iter()
        .filter_map(|record| match record.event {
            citizenchain::RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                account_id,
                fee,
            }) => Some((account_id, fee)),
            _ => None,
        })
        .collect()
    }

    /// 重组分支也必须包含合法用户交易，不能用空块绕过正式导入约束。
    fn native_remark(client: &Arc<FullClient>, nonce: u32) -> OpaqueExtrinsic {
        let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
            &format!("{:?}", client.info().genesis_hash),
            nonce,
            citizenchain::VERSION.spec_version,
            citizenchain::VERSION.transaction_version,
            b"mixed Ethereum index",
        )
        .unwrap();
        OpaqueExtrinsic::try_from_encoded_extrinsic(
            &hex::decode(hex.trim_start_matches("0x")).unwrap(),
        )
        .unwrap()
    }

    fn proposal(
        client: &Arc<FullClient>,
        parent: sp_core::H256,
        body: Vec<OpaqueExtrinsic>,
        timestamp: u64,
    ) -> BlockImportParams<Block> {
        let author = sr25519::Pair::from_string("//Alice//pow", None).unwrap();
        let mut digest = Digest::default();
        digest.push(DigestItem::PreRuntime(
            sp_consensus_pow::POW_ENGINE_ID,
            author.public().encode(),
        ));
        let mut builder = sc_block_builder::BlockBuilderBuilder::new(&**client)
            .on_parent_block(parent)
            .fetch_parent_block_number(&**client)
            .unwrap()
            .with_inherent_digests(digest)
            .build()
            .unwrap();
        let timestamp_xt: citizenchain::UncheckedExtrinsic =
            sp_runtime::generic::UncheckedExtrinsic::new_bare(
                citizenchain::RuntimeCall::Timestamp(citizenchain::TimestampCall::set {
                    now: timestamp,
                }),
            )
            .into();
        builder.push(timestamp_xt.into()).unwrap();
        for xt in body {
            builder.push(xt).unwrap();
        }
        let (block, changes) = builder.build().unwrap().into_inner();
        let (header, body) = block.deconstruct();
        let mut params: BlockImportParams<Block> =
            BlockImportParams::new(sp_consensus::BlockOrigin::Own, header);
        params.body = Some(body);
        params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(changes));
        // 真实 PoW seal 与两层守卫一起验收，不使用跳过签名或导入规则的测试后端。
        let prehash = params.header.hash();
        let difficulty = crate::core::service::SimplePow::new(client.clone())
            .difficulty(parent)
            .unwrap();
        let nonce = (0u64..)
            .find(|n| {
                crate::core::service::hash_meets_difficulty(
                    &crate::core::service::pow_hash(prehash.as_ref(), *n),
                    difficulty,
                )
            })
            .unwrap();
        params.post_digests.push(DigestItem::Seal(
            sp_consensus_pow::POW_ENGINE_ID,
            (nonce, author.sign(prehash.as_ref())).encode(),
        ));
        params.post_hash = Some(params.post_hash());
        params.fork_choice = Some(ForkChoiceStrategy::Custom(true));
        params.insert_intermediate(
            sc_consensus_pow::INTERMEDIATE_KEY,
            sc_consensus_pow::PowIntermediate::<U256> { difficulty: None },
        );
        params
    }

    /// 保留真实网络与数据库生命周期；同步只能经过生产导入队列执行 WASM 和两层守卫。
    struct TestNode {
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        network: Arc<dyn NetworkService>,
        sync: Arc<sc_network_sync::SyncingService<Block>>,
        pending_finality: Option<Box<dyn FnOnce() + Send>>,
        _task_manager: sc_service::TaskManager,
    }

    impl TestNode {
        fn start_finality(&mut self) {
            self.pending_finality
                .take()
                .expect("每个节点只启动一次 GRANDPA")();
        }
    }

    fn start_test_network(config: &mut Configuration, partial: Service, voter: bool) -> TestNode {
        type NetworkBackend = sc_network::NetworkWorker<Block, <Block as BlockT>::Hash>;
        let tls =
            crate::core::tls_cert::load_or_generate_tls_cert(config.base_path.path()).unwrap();
        config.network.tls_private_key_der = Some(tls.private_key_der);
        config.network.tls_certificate_chain_der = Some(tls.certificate_chain_der);
        let sc_service::PartialComponents {
            client,
            backend,
            task_manager,
            import_queue,
            transaction_pool,
            keystore_container,
            other: (_, grandpa_link, _),
            ..
        } = partial;
        let mut net_config = sc_network::config::FullNetworkConfiguration::<
            Block,
            <Block as BlockT>::Hash,
            NetworkBackend,
        >::new(&config.network, config.prometheus_registry().cloned());
        let metrics = NetworkBackend::register_notification_metrics(config.prometheus_registry());
        let protocol = sc_consensus_grandpa::protocol_standard_name(
            &client.info().genesis_hash,
            &config.chain_spec,
        );
        let (grandpa_protocol, notification_service) =
            sc_consensus_grandpa::grandpa_peers_set_config::<_, NetworkBackend>(
                protocol.clone(),
                metrics.clone(),
                net_config.peer_store_handle(),
            );
        net_config.add_notification_protocol(grandpa_protocol);
        let warp = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
            backend.clone(),
            grandpa_link.shared_authority_set().clone(),
            Vec::new(),
        ));
        let (network, _, _, sync) = sc_service::build_network(sc_service::BuildNetworkParams {
            config,
            net_config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            spawn_essential_handle: task_manager.spawn_essential_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_config: Some(sc_service::WarpSyncConfig::WithProvider(warp)),
            block_relay: None,
            metrics,
        })
        .unwrap();
        let keystore = if voter {
            let keystore = keystore_container.keystore();
            let public = keystore
                .ed25519_generate_new(sp_consensus_grandpa::KEY_TYPE, Some("//Alice"))
                .unwrap();
            assert_eq!(public, Ed25519Keyring::Alice.public());
            Some(keystore)
        } else {
            None
        };
        let grandpa_config = sc_consensus_grandpa::Config {
            gossip_duration: Duration::from_millis(333),
            justification_generation_period: 1,
            name: Some(config.network.node_name.clone()),
            observer_enabled: false,
            keystore,
            local_role: config.role,
            telemetry: None,
            protocol_name: protocol,
        };
        let grandpa_network = network.clone();
        let grandpa_sync = sync.clone();
        let spawn = task_manager.spawn_handle();
        let essential = task_manager.spawn_essential_handle();
        // 保留真实通知服务与导入链接，先完成未最终化分叉，再启动官方投票/观察任务。
        let pending_finality = Some(Box::new(move || {
            if voter {
                let params = sc_consensus_grandpa::GrandpaParams {
                    config: grandpa_config,
                    link: grandpa_link,
                    network: grandpa_network,
                    sync: grandpa_sync,
                    notification_service,
                    voting_rule: (),
                    prometheus_registry: None,
                    shared_voter_state: sc_consensus_grandpa::SharedVoterState::empty(),
                    telemetry: None,
                    offchain_tx_pool_factory:
                        sc_transaction_pool_api::OffchainTransactionPoolFactory::new(
                            transaction_pool,
                        ),
                };
                essential.spawn_blocking(
                    "grandpa-voter",
                    None,
                    sc_consensus_grandpa::run_grandpa_voter(params).unwrap(),
                );
            } else {
                spawn.spawn_blocking(
                    "grandpa-observer",
                    None,
                    sc_consensus_grandpa::run_grandpa_observer(
                        grandpa_config,
                        grandpa_link,
                        grandpa_network,
                        grandpa_sync,
                        notification_service,
                    )
                    .unwrap(),
                );
            }
        }) as Box<dyn FnOnce() + Send>);
        TestNode {
            client,
            backend,
            network,
            sync,
            pending_finality,
            _task_manager: task_manager,
        }
    }

    async fn wait_finalized(node: &TestNode, hash: sp_core::H256, number: u32) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while node.client.info().finalized_hash != hash {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("独立节点必须通过真实 GRANDPA 获得同一最终块");
        assert_eq!(node.client.info().finalized_number, number);
    }

    async fn assert_finalized_rpc(http: &reqwest::Client, url: &str, expected: &serde_json::Value) {
        for tag in ["safe", "finalized"] {
            let block = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let block = rpc(
                        http,
                        url,
                        "eth_getBlockByNumber",
                        serde_json::json!([tag, false]),
                    )
                    .await;
                    if block["hash"] == expected["hash"] {
                        break block;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("每个 HTTPS RPC 必须推进到经 GRANDPA 验证的同一最终块");
            assert_eq!(block["number"], expected["number"]);
        }
    }

    async fn connect_peer(peer: &TestNode, source: &TestNode) {
        assert_eq!(
            peer.client.info().genesis_hash,
            source.client.info().genesis_hash
        );
        tokio::time::timeout(Duration::from_secs(20), async {
            while source.network.listen_addresses().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            peer.network
                .add_reserved_peer(MultiaddrWithPeerId {
                    multiaddr: source
                        .network
                        .listen_addresses()
                        .into_iter()
                        .next()
                        .unwrap(),
                    peer_id: source.network.local_peer_id(),
                })
                .unwrap();
            while peer.sync.num_connected_peers() == 0 || source.sync.num_connected_peers() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("真实 TLS P2P 节点必须连接");
    }

    async fn wait_peer(peer: &TestNode, hash: sp_core::H256) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while peer.client.info().best_hash != hash {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("节点必须通过 P2P 导入同一规范块，禁止直接写入跟随节点");
    }

    /// 逐字段比较独立节点的真实 HTTPS 结果，禁止仅凭高度一致宣称状态同步。
    async fn assert_peer_state(
        http: &reqwest::Client,
        source: &str,
        peer: &str,
        signer: H160,
        contract: H160,
        transactions: &[serde_json::Value],
    ) {
        for (method, params) in [
            ("eth_chainId", serde_json::json!([])),
            ("eth_getCode", serde_json::json!([contract, "latest"])),
            (
                "eth_getStorageAt",
                serde_json::json!([contract, "0x0", "latest"]),
            ),
            ("eth_getBalance", serde_json::json!([signer, "latest"])),
            (
                "eth_getTransactionCount",
                serde_json::json!([signer, "latest"]),
            ),
            (
                "eth_getLogs",
                serde_json::json!([{"fromBlock":"0x1","toBlock":"latest","address":contract}]),
            ),
        ] {
            assert_eq!(
                rpc(http, peer, method, params.clone()).await,
                rpc(http, source, method, params).await,
                "跨节点 {method} 必须一致"
            );
        }
        for transaction in transactions {
            let params = serde_json::json!([transaction]);
            assert_eq!(
                rpc(http, peer, "eth_getTransactionReceipt", params.clone()).await,
                rpc(http, source, "eth_getTransactionReceipt", params).await,
                "回执必须绑定同步后的规范链及实际交易位置"
            );
        }
    }

    /// 自动回归也真实运行会话；只有显式参数延长就绪后的保留时间，不忽略测试。
    fn session_seconds(value: &str) -> Result<u64, &'static str> {
        match value.parse::<u64>() {
            Ok(seconds @ 1..=1800) => Ok(seconds),
            _ => Err("测试会话就绪后的保留时间必须为 1 至 1800 秒"),
        }
    }

    fn session_path(path: &std::path::Path) -> Result<(), &'static str> {
        use std::path::Component;
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        if !path.is_absolute()
            || path.parent().is_none()
            || !path.starts_with(repository.join("target"))
            || path.components().any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        {
            return Err("测试会话必须使用本产品 target 内的规范绝对目录");
        }
        Ok(())
    }

    /// 测试失败、超时或取消时也关闭监听与出块任务，避免占用后续验收资源。
    struct AbortSessionTask<T>(tokio::task::JoinHandle<T>);
    impl<T> Drop for AbortSessionTask<T> {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    #[test]
    fn metamask_session_settings_reject_source_paths_and_unbounded_duration() {
        for value in ["1", "5", "1800"] {
            assert!(session_seconds(value).is_ok());
        }
        for value in ["", "0", "1801", "-1", "1.5", "18446744073709551616"] {
            assert!(session_seconds(value).is_err(), "{value}");
        }
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for path in [std::path::PathBuf::from("relative"), repository.join("node/session"),
            std::path::PathBuf::from("/"), std::path::PathBuf::from("/tmp/../session"),
            std::env::temp_dir().join("../session")] {
            assert!(session_path(&path).is_err(), "{}", path.display());
        }
        assert!(session_path(&repository.join("target/macos/build/tmp/citizenchain-session")).is_ok());
        assert!(session_path(&repository.join("target/linux-amd/ci/tmp/citizenchain-session")).is_ok());
    }

    async fn session_receipt(
        http: &reqwest::Client,
        url: &str,
        transaction: &serde_json::Value,
    ) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let receipt = rpc(http, url, "eth_getTransactionReceipt",
                    serde_json::json!([transaction])).await;
                if !receipt.is_null() {
                    assert_eq!(receipt["status"], "0x1", "测试账户转账必须真实成功");
                    return receipt;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }).await.expect("交易池中的签名交易必须经真实出块取得成功回执")
    }

    /// 独立读取同一原生账本；Ethereum 可用余额另扣账户保留金，不能混为总余额。
    fn session_native_balance(
        client: &FullClient,
        account: &sp_runtime::AccountId32,
    ) -> u128 {
        use sc_client_api::StorageProvider;
        let key = frame_system::Account::<citizenchain::Runtime>::hashed_key_for(account);
        let data = client.storage(client.info().best_hash, &sp_storage::StorageKey(key))
            .unwrap().expect("预置测试账户必须存在于真实原生账本");
        type AccountInfo = frame_system::AccountInfo<citizenchain::Nonce,
            <citizenchain::Runtime as frame_system::Config>::AccountData>;
        AccountInfo::decode_all(&mut &data.0[..]).unwrap().data.free
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn metamask_three_node_session_uses_pool_pow_tls_and_grandpa() {
        use sc_transaction_pool_api::{ChainEvent, InPoolTransaction, MaintainedTransactionPool, TransactionPool};
        use sp_runtime::transaction_validity::TransactionSource;
        let explicit_root = std::env::var_os("CITIZENCHAIN_TEST_SESSION_ROOT");
        let root = explicit_root.as_ref().map(std::path::PathBuf::from).unwrap_or_else(|| {
            std::env::temp_dir().canonicalize().unwrap().join(format!("citizenchain-session-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()))
        });
        session_path(&root).unwrap();
        assert_eq!(root.parent().unwrap().canonicalize().unwrap(), root.parent().unwrap(),
            "会话父目录不能经过符号链接");
        let seconds = session_seconds(&std::env::var("CITIZENCHAIN_TEST_SESSION_SECONDS")
            .unwrap_or_else(|_| "1".into())).unwrap();
        std::fs::create_dir(&root).expect("只创建本轮全新会话，禁止覆盖旧数据库");
        // 这两个固定密钥是公开、可复现的测试向量，永远不能控制正式资产。
        let accounts = [Account::from_secret_key([1; 32]), Account::from_secret_key([2; 32])];
        let initial = 1_000_000u128; // 每个账户 10000 GMB，真实账本按分保存。
        let endowed = accounts.iter().map(|account| (account.substrate_account(), initial))
            .collect::<Vec<_>>();
        println!("三节点会话：从源码 WASM 计算本轮测试创世状态");
        let mut session_spec: Box<dyn sc_service::ChainSpec> = Box::new(chain_spec_with_accounts(&endowed));
        let storage = session_spec.as_storage_builder().build_storage().unwrap();
        assert_eq!(storage.top.get(sp_core::storage::well_known_keys::CODE).map(Vec::as_slice),
            citizenchain::WASM_BINARY, "测试创世必须包含本轮准确源码 WASM");
        // 同一会话只执行一次创世构造；三个数据库独立导入相同、已核对源码的 storage。
        session_spec.set_storage(storage);
        let mut configs = ["primary", "peer-two", "peer-three"].map(|name| {
            let mut config = test_config_at(name, tokio::runtime::Handle::current(), root.join(name));
            config.chain_spec = session_spec.cloned_box();
            config
        });
        configs[0].role = Role::Authority;
        println!("三节点会话：初始化主节点的源码 WASM 与独立数据库");
        let partial = crate::core::service::new_partial_for_test(&configs[0]).unwrap();
        let pool = partial.transaction_pool.clone();
        let client = partial.client.clone();
        let identity = crate::core::node_guard::TestGenesisIdentity::from_chain_spec(
            configs[0].chain_spec.as_ref()).unwrap();
        let raw_import = sc_consensus_pow::PowBlockImport::new(partial.other.0.clone(),
            client.clone(), crate::core::service::SimplePow::new(client.clone()), 0,
            partial.select_chain.clone(),
            |_, ()| async { Ok((sp_timestamp::InherentDataProvider::from_system_time(),)) });
        let guarded = crate::core::node_guard::NodeGuard::new_for_test(raw_import,
            client.clone(), partial.backend.clone(), identity);
        let importer = crate::core::constitution::ConstitutionGuard::new(guarded,
            client.clone(), partial.backend.clone()).unwrap();
        let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        // 只导出公开测试证书。RPC 私钥始终保留内存，浏览器信任在后续步骤配置。
        std::fs::write(root.join("rpc-certificate.der"), credentials.cert.der().as_ref()).unwrap();
        let http = reqwest::Client::builder().https_only(true).no_proxy()
            .add_root_certificate(reqwest::Certificate::from_der(credentials.cert.der().as_ref()).unwrap())
            .timeout(Duration::from_secs(15)).build().unwrap();
        let (url, rpc_task) = serve_on_port(&partial, &credentials,
            if explicit_root.is_some() { 9944 } else { 0 }).await;
        let mut rpc_tasks = vec![AbortSessionTask(rpc_task)];
        let mut primary = start_test_network(&mut configs[0], partial, true);
        let mut peers = Vec::new();
        let mut urls = vec![url.clone()];
        for config in &mut configs[1..] {
            println!("三节点会话：初始化 {}", config.network.node_name);
            let partial = crate::core::service::new_partial_for_test(config).unwrap();
            let (peer_url, task) = serve(&partial, &credentials).await;
            rpc_tasks.push(AbortSessionTask(task));
            urls.push(peer_url);
            let mut peer = start_test_network(config, partial, false);
            connect_peer(&peer, &primary).await;
            peer.start_finality();
            peers.push(peer);
        }
        primary.start_finality();
        println!("三节点会话：TLS P2P 已连接，核对原生总余额与 Ethereum 可用余额");
        for (node, endpoint) in urls.iter().enumerate() {
            let endpoint_client = if node == 0 { &client } else { &peers[node - 1].client };
            assert_eq!(rpc(&http, endpoint, "eth_chainId", serde_json::json!([])).await, "0x7eb");
            for account in &accounts {
                assert_eq!(session_native_balance(endpoint_client, &account.substrate_account()), initial);
                let balance: U256 = serde_json::from_value(rpc(&http, endpoint, "eth_getBalance",
                    serde_json::json!([account.address(), "latest"])).await).unwrap();
                assert_eq!(balance, U256::from(initial - citizenchain::EXISTENTIAL_DEPOSIT)
                    * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO));
                assert_eq!(rpc(&http, endpoint, "eth_getTransactionCount",
                    serde_json::json!([account.address(), "latest"])).await, "0x0");
            }
        }
        let (stop_authoring, mut stopped) = tokio::sync::oneshot::channel::<()>();
        let author_pool = pool.clone();
        let author_client = client.clone();
        let synchronization = primary.sync.clone();
        let mut authoring = AbortSessionTask(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            let mut previous_timestamp = 0;
            loop {
                tokio::select! {
                    _ = &mut stopped => return Ok::<(), String>(()),
                    _ = interval.tick() => {}
                }
                let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| error.to_string())?.as_millis() as u64;
                if timestamp < previous_timestamp + 12_000 { continue; }
                // 只消费真实交易池中的 ready 交易；空池不制造空块或修改账户 nonce。
                let body = author_pool.ready().take(1).map(|transaction| (**transaction.data()).clone())
                    .collect::<Vec<_>>();
                if body.is_empty() { continue; }
                let block = proposal(&author_client, author_client.info().best_hash, body, timestamp);
                let hash = block.post_hash();
                let result = importer.import_block(block).await.map_err(|error| format!("{error:?}"))?;
                if !matches!(result, ImportResult::Imported(_)) {
                    return Err(format!("正式守卫拒绝测试交易区块：{result:?}"));
                }
                author_pool.maintain(ChainEvent::NewBestBlock { hash, tree_route: None }).await;
                synchronization.announce_block(hash, None);
                previous_timestamp = timestamp;
            }
        }));
        let price: U256 = serde_json::from_value(rpc(&http, &url, "eth_gasPrice", serde_json::json!([])).await).unwrap();
        assert_eq!(price, U256::from(1_000_000_000u64));
        assert_eq!(rpc(&http, &url, "eth_maxPriorityFeePerGas", serde_json::json!([])).await, "0x0");
        // 非整分金额和无效价格参数必须在真实 WASM 报价阶段拒绝，不能进入交易池。
        for invalid in [
            serde_json::json!({"value":U256::one()}),
            serde_json::json!({"gasPrice":price - U256::one()}),
            serde_json::json!({"maxFeePerGas":price - U256::one(),"maxPriorityFeePerGas":"0x0"}),
            serde_json::json!({"maxFeePerGas":price,"maxPriorityFeePerGas":price + U256::one()}),
        ] {
            let mut request = invalid;
            request["from"] = serde_json::to_value(accounts[0].address()).unwrap();
            request["to"] = serde_json::to_value(accounts[1].address()).unwrap();
            assert!(response(&http, &url, "eth_estimateGas", serde_json::json!([request])).await.get("error").is_some());
        }
        assert_eq!(session_native_balance(&client, &accounts[0].substrate_account()), initial);
        assert_eq!(rpc(&http, &url, "eth_getTransactionCount", serde_json::json!([accounts[0].address(), "latest"])).await, "0x0");
        let transfers = [(0, 1, 100u128, 10u128), (1, 0, 10_499, 10),
            (0, 1, 10_500, 11), (1, 0, 100_000, 100)];
        let mut receipts = Vec::new();
        let mut transactions = Vec::new();
        let mut nonces = [0u32; 2];
        for (index, &(sender, recipient, value, fee)) in transfers.iter().enumerate() {
            println!("三节点会话：广播并等待第 {} 笔真实转账回执", index + 1);
            // 报价和签名携带同一钱包字段；非零优先费不能仅在签名替身中覆盖。
            let eip1559 = index % 2 == 1;
            let priority = if index == 1 { U256::one() } else { U256::from(1_000_000u64) };
            let mut request = serde_json::json!({"from":accounts[sender].address(),
                "to":accounts[recipient].address(),
                "value":U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)});
            if eip1559 {
                request["maxFeePerGas"] = serde_json::to_value(price * U256::from(2)).unwrap();
                request["maxPriorityFeePerGas"] = serde_json::to_value(priority).unwrap();
            } else {
                request["gasPrice"] = serde_json::to_value(price + U256::one()).unwrap();
            }
            let estimate: U256 = serde_json::from_value(rpc(&http, &url, "eth_estimateGas",
                serde_json::json!([request])).await).unwrap();
            assert!(estimate * price >= U256::from(fee) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO));
            // 真实钱包按报价增加 gas 缓冲；Legacy 与 EIP-1559 均不能改变实际业务费。
            let tx = GenericTransaction {
                from: Some(accounts[sender].address()), to: Some(accounts[recipient].address()),
                chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
                nonce: Some(nonces[sender].into()), gas: Some(estimate * U256::from(2)),
                gas_price: if eip1559 { None } else { Some(price + U256::one()) },
                max_fee_per_gas: if eip1559 { Some(price * U256::from(2)) } else { None },
                max_priority_fee_per_gas: if eip1559 { Some(priority) } else { None },
                value: Some(U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)),
                r#type: Some(if eip1559 { pallet_revive::evm::TYPE_EIP1559 } else { pallet_revive::evm::TYPE_LEGACY }.into()),
                ..Default::default()
            };
            let (raw, _) = signed_transaction(&accounts[sender], tx);
            let hash = rpc(&http, &url, "eth_sendRawTransaction", serde_json::json!([raw])).await;
            receipts.push(session_receipt(&http, &url, &hash).await);
            transactions.push(hash);
            nonces[sender] += 1;
        }
        // 用独立的公开 Alice 账户产生第三个合法块，验证真实最终性且不扰动测试账户。
        pool.submit_one(client.info().best_hash, TransactionSource::Local,
            native_remark(&client, 0)).await.unwrap();
        wait_block(&http, &url, 5).await;
        let hash = client.info().best_hash;
        for peer in &peers { wait_peer(peer, hash).await; }
        wait_finalized(&primary, hash, 5).await;
        for peer in &peers { wait_finalized(peer, hash, 5).await; }
        let head = rpc(&http, &url, "eth_getBlockByNumber", serde_json::json!(["latest", false])).await;
        for endpoint in &urls { assert_finalized_rpc(&http, endpoint, &head).await; }
        let mut fees = [0u128; 2];
        let mut expected = [initial; 2];
        for (index, receipt) in receipts.iter().enumerate() {
            // Ethereum 回执哈希与原生头哈希不同；按已验证的同一高度读取原生费用事件。
            let number = (index + 1) as u32;
            assert_eq!(receipt["blockNumber"], format!("0x{number:x}"));
            let ethereum_block = rpc(&http, &url, "eth_getBlockByNumber",
                serde_json::json!([format!("0x{number:x}"), false])).await;
            assert_eq!(ethereum_block["hash"], receipt["blockHash"]);
            let block_hash = client.hash(number).unwrap().expect("成功转账必须具有同高度原生区块");
            let paid = paid_fees(&client, block_hash);
            assert_eq!(paid.len(), 1, "一笔 Ethereum 转账只能产生一次真实手续费事件");
            let (sender, recipient, value, fee) = transfers[index];
            assert_eq!(paid[0], (accounts[sender].substrate_account(), fee));
            let used: U256 = serde_json::from_value(receipt["gasUsed"].clone()).unwrap();
            let effective: U256 = serde_json::from_value(receipt["effectiveGasPrice"].clone()).unwrap();
            assert_eq!(effective, price);
            assert_eq!(used * effective, U256::from(fee) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO));
            assert_eq!(ethereum_block["baseFeePerGas"], serde_json::to_value(price).unwrap());
            fees[sender] += fee;
            expected[sender] -= value + fee;
            expected[recipient] += value;
        }
        let history = rpc(&http, &url, "eth_feeHistory", serde_json::json!(["0x1", "latest", [0, 50, 100]])).await;
        assert_eq!(history["baseFeePerGas"], serde_json::json!([price, price]));
        assert_eq!(history["reward"], serde_json::json!([["0x0", "0x0", "0x0"]]));
        for (node, endpoint) in urls.iter().enumerate() {
            let endpoint_client = if node == 0 { &client } else { &peers[node - 1].client };
            for (index, account) in accounts.iter().enumerate() {
                assert_eq!(session_native_balance(endpoint_client, &account.substrate_account()), expected[index]);
                let balance: U256 = serde_json::from_value(rpc(&http, endpoint, "eth_getBalance",
                    serde_json::json!([account.address(), "latest"])).await).unwrap();
                assert_eq!(balance, U256::from(expected[index] - citizenchain::EXISTENTIAL_DEPOSIT)
                    * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO));
                assert_eq!(rpc(&http, endpoint, "eth_getTransactionCount",
                    serde_json::json!([account.address(), "latest"])).await, "0x2");
            }
            for (transaction, receipt) in transactions.iter().zip(&receipts) {
                assert_eq!(rpc(&http, endpoint, "eth_getTransactionReceipt",
                    serde_json::json!([transaction])).await, *receipt);
            }
        }
        let ready = serde_json::json!({"rpc":url,"chain_id":2027,"genesis_hash":client.info().genesis_hash,
            "runtime_wasm_blake2_256":sp_core::H256::from(sp_crypto_hashing::blake2_256(citizenchain::WASM_BINARY.unwrap())),
            "nodes":3,"finalized_hash":hash,"ethereum_finalized_hash":head["hash"],"finalized_number":5,
            "accounts":accounts.iter().enumerate().map(|(index, account)| serde_json::json!({
                "address":account.address(),"account_id":format!("0x{}", hex::encode(account.substrate_account())),
                "initial_fen":initial,"balance_fen":expected[index],
                "ethereum_balance_fen":expected[index] - citizenchain::EXISTENTIAL_DEPOSIT,
                "existential_deposit_fen":citizenchain::EXISTENTIAL_DEPOSIT,"fee_fen":fees[index],"nonce":nonces[index]
            })).collect::<Vec<_>>(),"transactions":transactions,"hold_seconds":seconds});
        std::fs::write(root.join("ready.json"), serde_json::to_vec_pretty(&ready).unwrap()).unwrap();
        println!("测试链真实就绪：{ready}");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        while tokio::time::Instant::now() < deadline {
            assert!(!authoring.0.is_finished(), "测试出块任务提前结束");
            assert!(rpc_tasks.iter().all(|task| !task.0.is_finished()), "测试 RPC 提前结束");
            let stop = root.join("stop");
            if let Ok(metadata) = std::fs::symlink_metadata(&stop) {
                assert!(metadata.is_file() && !metadata.file_type().is_symlink(), "停止请求必须是普通文件");
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        stop_authoring.send(()).unwrap();
        (&mut authoring.0).await.unwrap().unwrap();
        for task in &mut rpc_tasks { task.0.abort(); let _ = (&mut task.0).await; }
        drop(peers);
        drop(primary);
        drop(pool);
        drop(client);
        let port = url.rsplit_once(':').unwrap().1.parse::<u16>().unwrap();
        let released = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await.expect("停止后必须释放原 RPC 端口");
        drop(released);
        std::fs::write(root.join("stopped.json"), br#"{"stopped":true}"#).unwrap();
        println!("测试会话已停止，监听及三节点服务已关闭");
        if explicit_root.is_none() { std::fs::remove_dir_all(root).unwrap(); }
    }

    #[test]
    fn standard_subscription_kinds_and_parameter_boundaries_are_enforced() {
        use jsonrpsee::types::Params;
        assert_eq!(
            subscription_request(&Params::new(Some(r#"["newHeads"]"#)))
                .unwrap()
                .0,
            SubscriptionKind::NewBlockHeaders
        );
        assert_eq!(
            subscription_request(&Params::new(Some(r#"["logs",{}]"#)))
                .unwrap()
                .0,
            SubscriptionKind::Logs
        );
        for invalid in [
            r#"[]"#,
            r#"["newBlockHeaders"]"#,
            r#"["unknown"]"#,
            r#"["logs",1]"#,
            r#"["logs",{},null]"#,
        ] {
            assert!(
                subscription_request(&Params::new(Some(invalid))).is_err(),
                "{invalid}"
            );
        }
    }

    #[tokio::test]
    async fn pending_mapping_reads_native_storage_and_rejects_corrupt_accounts() {
        let address = H160::from_low_u64_be(42);
        let mapped = sp_runtime::AccountId32::new([7; 32]);
        // 没有 Externalities 的宿主环境也必须正确读取已有映射，并对损坏编码拒绝服务。
        for (raw, expected) in [
            (None, Some(pallet_revive::AccountId32Mapper::<citizenchain::Runtime>::to_fallback_account_id(&address))),
            (Some(mapped.encode()), Some(mapped.clone())),
            (Some(vec![7; 31]), None),
            (Some(vec![7; 33]), None),
        ] {
            let mut native = RpcModule::new(());
            native.register_method("state_getStorage", move |_, _, _| raw.clone().map(sp_core::Bytes)).unwrap();
            let actual = pending_account(&native, address).await;
            match expected {
                Some(expected) => assert_eq!(actual.unwrap(), expected),
                None => assert!(actual.is_err(), "损坏映射不能回退到另一账户"),
            }
        }
        assert!(
            pending_account(&RpcModule::new(()), address).await.is_err(),
            "存储读取失败不能视为无映射"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signed_transactions_receipts_logs_finality_reorg_and_restart_over_real_tls_wasm() {
        let mut config = test_config("ethereum", tokio::runtime::Handle::current());
        config.role = Role::Authority;
        let root = config.base_path.path().to_path_buf();
        let identity = crate::core::node_guard::TestGenesisIdentity::from_chain_spec(
            config.chain_spec.as_ref(),
        )
        .unwrap();
        let partial = crate::core::service::new_partial_for_test(&config).unwrap();
        let raw_import = sc_consensus_pow::PowBlockImport::new(
            partial.other.0.clone(),
            partial.client.clone(),
            crate::core::service::SimplePow::new(partial.client.clone()),
            0,
            partial.select_chain.clone(),
            |_, ()| async { Ok((sp_timestamp::InherentDataProvider::from_system_time(),)) },
        );
        let guarded = crate::core::node_guard::NodeGuard::new_for_test(
            raw_import,
            partial.client.clone(),
            partial.backend.clone(),
            identity,
        );
        let importer = crate::core::constitution::ConstitutionGuard::new(
            guarded,
            partial.client.clone(),
            partial.backend.clone(),
        )
        .unwrap();
        let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let http = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .add_root_certificate(
                reqwest::Certificate::from_der(credentials.cert.der().as_ref()).unwrap(),
            )
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        let (url, server) = serve(&partial, &credentials).await;
        let mut partial = start_test_network(&mut config, partial, true);
        let mut peer_config = test_config("ethereum-peer", tokio::runtime::Handle::current());
        let peer_root = peer_config.base_path.path().to_path_buf();
        let peer_partial = crate::core::service::new_partial_for_test(&peer_config).unwrap();
        let (peer_url, peer_server) = serve(&peer_partial, &credentials).await;
        let mut peer = start_test_network(&mut peer_config, peer_partial, false);
        connect_peer(&peer, &partial).await;
        assert_eq!(
            rpc(&http, &url, "eth_chainId", serde_json::json!([])).await,
            "0x7eb"
        );
        assert_eq!(
            rpc(&http, &url, "eth_accounts", serde_json::json!([])).await,
            serde_json::json!([])
        );
        let signer = Account::default().address();
        let before = rpc(
            &http,
            &url,
            "eth_getBalance",
            serde_json::json!([signer, "latest"]),
        )
        .await;
        let code = vec![
            0x60, 42, 0x60, 0, 0x55, 0x60, 42, 0x60, 0, 0x52, 0x60, 7, 0x60, 32, 0x60, 0, 0xa1,
            0x00,
        ];
        let mut init = vec![
            0x60,
            code.len() as u8,
            0x60,
            12,
            0x60,
            0,
            0x39,
            0x60,
            code.len() as u8,
            0x60,
            0,
            0xf3,
        ];
        init.extend_from_slice(&code);
        let (deploy_raw, deploy_xt) = transaction(0, None, init, 200);
        let deployment = rpc(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([deploy_raw]),
        )
        .await;
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionCount",
                serde_json::json!([signer, "pending"])
            )
            .await,
            "0x1"
        );
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionCount",
                serde_json::json!([signer, "latest"])
            )
            .await,
            "0x0"
        );
        let block1 = proposal(
            &partial.client,
            partial.client.info().genesis_hash,
            vec![deploy_xt],
            1_782_950_406_000,
        );
        let hash1 = block1.post_hash();
        assert!(matches!(
            importer.import_block(block1).await.unwrap(),
            ImportResult::Imported(_)
        ));
        wait_block(&http, &url, 1).await;
        partial.sync.announce_block(hash1, None);
        wait_peer(&peer, hash1).await;
        wait_block(&http, &peer_url, 1).await;
        let receipt = rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([deployment]),
        )
        .await;
        assert_eq!(receipt["status"], "0x1");
        assert_eq!(receipt["transactionIndex"], "0x0");
        let contract: H160 = serde_json::from_value(receipt["contractAddress"].clone()).unwrap();
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getCode",
                serde_json::json!([contract, "latest"])
            )
            .await,
            format!("0x{}", hex::encode(&code))
        );
        assert_peer_state(
            &http,
            &url,
            &peer_url,
            signer,
            contract,
            &[deployment.clone()],
        )
        .await;
        let balance: U256 = serde_json::from_value(before).unwrap();
        let after: U256 = serde_json::from_value(
            rpc(
                &http,
                &url,
                "eth_getBalance",
                serde_json::json!([signer, "latest"]),
            )
            .await,
        )
        .unwrap();
        assert_eq!(
            balance - after,
            U256::from(
                200 + citizenchain::EXISTENTIAL_DEPOSIT
                    + primitives::fee_policy::calculate_onchain_fee(200)
            ) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
        );
        assert_eq!(
            paid_fees(&partial.client, hash1),
            vec![(
                Account::default().substrate_account(),
                primitives::fee_policy::calculate_onchain_fee(200)
            )]
        );
        assert_eq!(
            serde_json::from_value::<U256>(
                rpc(
                    &http,
                    &url,
                    "eth_getBalance",
                    serde_json::json!([contract, "latest"])
                )
                .await
            )
            .unwrap(),
            U256::from(200) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
            "合约 Ethereum 余额保留转入金额，ED 单独由原生账本承载"
        );
        let storage = rpc(
            &http,
            &url,
            "eth_getStorageAt",
            serde_json::json!([contract, "0x0", "latest"]),
        )
        .await;
        let _ = rpc(
            &http,
            &url,
            "eth_call",
            serde_json::json!([{"from":signer,"to":contract,"data":"0x"},"latest"]),
        )
        .await;
        let gas = rpc(
            &http,
            &url,
            "eth_estimateGas",
            serde_json::json!([{"from":signer,"to":contract,"data":"0x"}]),
        )
        .await;
        assert!(serde_json::from_value::<U256>(gas).unwrap() > U256::zero());
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getStorageAt",
                serde_json::json!([contract, "0x0", "latest"])
            )
            .await,
            storage
        );
        assert_eq!(
            serde_json::from_value::<U256>(
                rpc(
                    &http,
                    &url,
                    "eth_getBalance",
                    serde_json::json!([signer, "latest"])
                )
                .await
            )
            .unwrap(),
            after
        );
        let (call_raw, call_xt) = transaction(1, Some(contract), vec![], 0);
        let call_hash = rpc(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([call_raw]),
        )
        .await;
        let branch_a = proposal(
            &partial.client,
            hash1,
            vec![call_xt.clone()],
            1_782_950_412_000,
        );
        let hash_a = branch_a.post_hash();
        importer.import_block(branch_a).await.unwrap();
        wait_block(&http, &url, 2).await;
        partial.sync.announce_block(hash_a, None);
        wait_peer(&peer, hash_a).await;
        wait_block(&http, &peer_url, 2).await;
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionReceipt",
                serde_json::json!([call_hash])
            )
            .await["status"],
            "0x1"
        );
        assert_peer_state(
            &http,
            &url,
            &peer_url,
            signer,
            contract,
            &[deployment.clone(), call_hash.clone()],
        )
        .await;
        // 同高不同最佳链真实重组：替换后旧回执、日志与状态必须消失。
        let branch_b = proposal(
            &partial.client,
            hash1,
            vec![native_remark(&partial.client, 0)],
            1_782_950_418_000,
        );
        let hash2 = branch_b.post_hash();
        importer.import_block(branch_b).await.unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            while !rpc(
                &http,
                &url,
                "eth_getTransactionReceipt",
                serde_json::json!([call_hash]),
            )
            .await
            .is_null()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getLogs",
                serde_json::json!([{"fromBlock":"0x2","toBlock":"0x2","address":contract}])
            )
            .await,
            serde_json::json!([])
        );
        let mut roots = rustls::RootCertStore::empty();
        roots.add(credentials.cert.der().clone()).unwrap();
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let ws = jsonrpsee::ws_client::WsClientBuilder::new()
            .with_custom_cert_store(tls)
            .build(&url.replace("https:", "wss:"))
            .await
            .unwrap();
        let mut heads = ws
            .subscribe::<serde_json::Value, _>(
                "eth_subscribe",
                jsonrpsee::rpc_params!["newHeads"],
                "eth_unsubscribe",
            )
            .await
            .unwrap();
        let mut logs = ws
            .subscribe::<serde_json::Value, _>(
                "eth_subscribe",
                jsonrpsee::rpc_params!["logs", serde_json::json!({"address":contract})],
                "eth_unsubscribe",
            )
            .await
            .unwrap();
        let (fail_raw, fail_xt) = transaction(2, None, vec![0x60, 0, 0x60, 0, 0xfd], 200);
        let fail_hash = rpc(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([fail_raw]),
        )
        .await;
        let remark = native_remark(&partial.client, 1);
        let block3 = proposal(
            &partial.client,
            hash2,
            vec![remark, call_xt, fail_xt],
            1_782_950_424_000,
        );
        let hash3 = block3.post_hash();
        importer.import_block(block3).await.unwrap();
        wait_block(&http, &url, 3).await;
        // 同高分支不强迫跟随节点切换；更重分支经网络导入后才验证旧回执替换。
        partial.sync.announce_block(hash3, None);
        wait_peer(&peer, hash3).await;
        wait_block(&http, &peer_url, 3).await;
        assert_peer_state(
            &http,
            &url,
            &peer_url,
            signer,
            contract,
            &[deployment.clone(), call_hash.clone(), fail_hash.clone()],
        )
        .await;
        assert_eq!(
            paid_fees(&peer.client, hash3),
            paid_fees(&partial.client, hash3)
        );
        let head = tokio::time::timeout(Duration::from_secs(10), heads.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(head["number"], "0x3");
        assert!(
            tokio::time::timeout(Duration::from_millis(100), logs.next())
                .await
                .is_err(),
            "最佳链日志不得冒充最终日志"
        );
        let success = rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([call_hash]),
        )
        .await;
        let failure = rpc(
            &http,
            &url,
            "eth_getTransactionReceipt",
            serde_json::json!([fail_hash]),
        )
        .await;
        assert_eq!(success["transactionIndex"], "0x0");
        assert_eq!(success["logs"][0]["logIndex"], "0x0");
        assert_eq!(failure["transactionIndex"], "0x1");
        assert_eq!(failure["status"], "0x0");
        assert!(failure["contractAddress"].is_null());
        assert_eq!(failure["logs"], serde_json::json!([]));
        let first: U256 = serde_json::from_value(success["gasUsed"].clone()).unwrap();
        let second: U256 = serde_json::from_value(failure["gasUsed"].clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<U256>(failure["cumulativeGasUsed"].clone()).unwrap(),
            first + second
        );
        // 分叉验收完成之前不能固化旧分支；最佳块通知不构成最终性证明。
        assert_eq!(partial.client.info().finalized_number, 0);
        assert_eq!(peer.client.info().finalized_number, 0);
        peer.start_finality();
        partial.start_finality();
        wait_finalized(&partial, hash3, 3).await;
        wait_finalized(&peer, hash3, 3).await;
        let log = tokio::time::timeout(Duration::from_secs(10), logs.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(log["transactionHash"], call_hash);
        let finalized3 = rpc(
            &http,
            &url,
            "eth_getBlockByNumber",
            serde_json::json!(["0x3", false]),
        )
        .await;
        assert_eq!(finalized3["number"], "0x3");
        for node_url in [&url, &peer_url] {
            assert_finalized_rpc(&http, node_url, &finalized3).await;
        }
        assert!(response(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([deploy_raw])
        )
        .await
        .get("error")
        .is_some());
        let after_calls: U256 = serde_json::from_value(
            rpc(
                &http,
                &url,
                "eth_getBalance",
                serde_json::json!([signer, "latest"]),
            )
            .await,
        )
        .unwrap();
        assert_eq!(
            after - after_calls,
            U256::from(
                primitives::fee_policy::calculate_onchain_fee(0)
                    + primitives::fee_policy::calculate_onchain_fee(200)
            ) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
        );
        assert_eq!(
            paid_fees(&partial.client, hash3)
                .into_iter()
                .filter(|(account_id, _)| *account_id == Account::default().substrate_account())
                .collect::<Vec<_>>(),
            vec![
                (
                    Account::default().substrate_account(),
                    primitives::fee_policy::calculate_onchain_fee(0)
                ),
                (
                    Account::default().substrate_account(),
                    primitives::fee_policy::calculate_onchain_fee(200)
                ),
            ],
            "成功调用和失败部署均只收一次既有原生费用"
        );
        // 迟到的第三个独立数据库从块0追块，不能复用前两个节点的数据库或索引。
        let mut late_config = test_config("ethereum-late", tokio::runtime::Handle::current());
        let late_root = late_config.base_path.path().to_path_buf();
        let late_partial = crate::core::service::new_partial_for_test(&late_config).unwrap();
        assert_eq!(late_partial.client.info().best_number, 0);
        let (late_url, late_server) = serve(&late_partial, &credentials).await;
        let mut late = start_test_network(&mut late_config, late_partial, false);
        connect_peer(&late, &partial).await;
        partial.sync.announce_block(hash3, None);
        wait_peer(&late, hash3).await;
        wait_block(&http, &late_url, 3).await;
        late.start_finality();
        wait_finalized(&late, hash3, 3).await;
        assert_finalized_rpc(&http, &late_url, &finalized3).await;
        assert_peer_state(
            &http,
            &url,
            &late_url,
            signer,
            contract,
            &[deployment.clone(), call_hash.clone(), fail_hash.clone()],
        )
        .await;

        // 真正断开第二节点并释放 RocksDB；离线期间的新调用只在源节点执行和收费。
        peer_server.abort();
        let _ = peer_server.await;
        let peer_backend = Arc::downgrade(&peer.backend);
        drop(peer);
        tokio::time::timeout(Duration::from_secs(10), async {
            while peer_backend.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("重启前跟随节点数据库必须释放");
        let (offline_raw, offline_xt) = transaction(3, Some(contract), vec![], 0);
        let offline_hash = rpc(
            &http,
            &url,
            "eth_sendRawTransaction",
            serde_json::json!([offline_raw]),
        )
        .await;
        let block4 = proposal(&partial.client, hash3, vec![offline_xt], 1_782_950_430_000);
        let hash4 = block4.post_hash();
        importer.import_block(block4).await.unwrap();
        wait_block(&http, &url, 4).await;
        partial.sync.announce_block(hash4, None);
        wait_peer(&late, hash4).await;
        wait_block(&http, &late_url, 4).await;
        let peer_partial = crate::core::service::new_partial_for_test(&peer_config).unwrap();
        assert_eq!(peer_partial.client.info().best_hash, hash3);
        let (peer_url, peer_server) = serve(&peer_partial, &credentials).await;
        let mut peer = start_test_network(&mut peer_config, peer_partial, false);
        peer.start_finality();
        connect_peer(&peer, &partial).await;
        wait_peer(&peer, hash4).await;
        wait_block(&http, &peer_url, 4).await;
        let finalized4 = rpc(
            &http,
            &url,
            "eth_getBlockByNumber",
            serde_json::json!(["0x4", false]),
        )
        .await;
        assert_eq!(finalized4["number"], "0x4");
        for node in [&partial, &peer, &late] {
            wait_finalized(node, hash4, 4).await;
        }
        for node_url in [&url, &peer_url, &late_url] {
            assert_finalized_rpc(&http, node_url, &finalized4).await;
        }
        for peer_url in [&peer_url, &late_url] {
            assert_peer_state(
                &http,
                &url,
                peer_url,
                signer,
                contract,
                &[
                    deployment.clone(),
                    call_hash.clone(),
                    fail_hash.clone(),
                    offline_hash.clone(),
                ],
            )
            .await;
        }
        assert_eq!(
            paid_fees(&peer.client, hash4),
            vec![(
                Account::default().substrate_account(),
                primitives::fee_policy::calculate_onchain_fee(0),
            )],
            "同步重放不产生第二份费用事件"
        );
        let after_offline: U256 = serde_json::from_value(
            rpc(
                &http,
                &peer_url,
                "eth_getBalance",
                serde_json::json!([signer, "latest"]),
            )
            .await,
        )
        .unwrap();
        assert_eq!(
            after_calls - after_offline,
            U256::from(primitives::fee_policy::calculate_onchain_fee(0))
                * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
        );
        for (peer, peer_server) in [(peer, peer_server), (late, late_server)] {
            peer_server.abort();
            let _ = peer_server.await;
            let backend = Arc::downgrade(&peer.backend);
            drop(peer);
            tokio::time::timeout(Duration::from_secs(10), async {
                while backend.upgrade().is_some() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("清理前每个独立节点数据库必须释放");
        }
        drop(peer_config);
        drop(late_config);
        std::fs::remove_dir_all(peer_root).unwrap();
        std::fs::remove_dir_all(late_root).unwrap();
        // 真正关闭监听、同步任务和链数据库，再重新打开同一数据库重建近期索引。
        drop(heads);
        drop(logs);
        drop(ws);
        server.abort();
        let _ = server.await;
        drop(importer);
        let backend = Arc::downgrade(&partial.backend);
        drop(partial);
        tokio::time::timeout(Duration::from_secs(10), async {
            while backend.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let restarted = crate::core::service::new_partial_for_test(&config).unwrap();
        let (url, restarted_server) = serve(&restarted, &credentials).await;
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionReceipt",
                serde_json::json!([deployment])
            )
            .await["status"],
            "0x1"
        );
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionReceipt",
                serde_json::json!([fail_hash])
            )
            .await["contractAddress"],
            serde_json::Value::Null
        );
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getLogs",
                serde_json::json!([{"fromBlock":"0x1","toBlock":"0x4","address":contract}])
            )
            .await
            .as_array()
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            rpc(
                &http,
                &url,
                "eth_getTransactionCount",
                serde_json::json!([signer, "latest"])
            )
            .await,
            "0x4"
        );
        restarted_server.abort();
        let _ = restarted_server.await;
        let backend = Arc::downgrade(&restarted.backend);
        drop(restarted);
        tokio::time::timeout(Duration::from_secs(10), async {
            while backend.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        drop(config);
        std::fs::remove_dir_all(root).unwrap();
    }
}
