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
    use sc_client_api::backend::Finalizer;
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
    use sp_keyring::Sr25519Keyring;
    use sp_runtime::{traits::Block as BlockT, Digest, DigestItem, OpaqueExtrinsic};
    use std::{sync::Arc, time::Duration};

    fn chain_spec() -> crate::core::chain_spec::ChainSpec {
        let wasm = citizenchain::WASM_BINARY.expect("必须执行真实源码 WASM，禁止跳过");
        let mut genesis = citizenchain::genesis::genesis_config();
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
        let mut module = native(partial);
        let (ethereum, synchronization) = initialize(module.clone()).await.unwrap();
        module.merge(ethereum).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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
        let tx = GenericTransaction {
            from: Some(Account::default().address()),
            to,
            nonce: Some(nonce.into()),
            chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
            gas: Some(100_000_000u64.into()),
            gas_price: Some(primitives::core_const::NATIVE_TO_ETH_RATIO.into()),
            value: Some(
                U256::from(value) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO),
            ),
            input: Bytes(input).into(),
            r#type: Some(pallet_revive::evm::TYPE_LEGACY.into()),
            ..Default::default()
        };
        let payload = Account::default()
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
        _task_manager: sc_service::TaskManager,
    }

    fn start_test_network(config: &mut Configuration, partial: Service) -> TestNode {
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
        let (grandpa_protocol, _) = sc_consensus_grandpa::grandpa_peers_set_config::<
            _,
            NetworkBackend,
        >(
            protocol, metrics.clone(), net_config.peer_store_handle()
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
            transaction_pool,
            spawn_handle: task_manager.spawn_handle(),
            spawn_essential_handle: task_manager.spawn_essential_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_config: Some(sc_service::WarpSyncConfig::WithProvider(warp)),
            block_relay: None,
            metrics,
        })
        .unwrap();
        TestNode {
            client,
            backend,
            network,
            sync,
            _task_manager: task_manager,
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
        let partial = start_test_network(&mut config, partial);
        let mut peer_config = test_config("ethereum-peer", tokio::runtime::Handle::current());
        let peer_root = peer_config.base_path.path().to_path_buf();
        let peer_partial = crate::core::service::new_partial_for_test(&peer_config).unwrap();
        let (peer_url, peer_server) = serve(&peer_partial, &credentials).await;
        let peer = start_test_network(&mut peer_config, peer_partial);
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
        partial.client.finalize_block(hash3, None, true).unwrap();
        // 此夹具没有 GRANDPA voter；远端最佳块不能冒充已验证最终性证明。
        assert_eq!(peer.client.info().finalized_number, 0);
        let log = tokio::time::timeout(Duration::from_secs(10), logs.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(log["transactionHash"], call_hash);
        tokio::time::timeout(Duration::from_secs(30), async {
            while rpc(
                &http,
                &url,
                "eth_getBlockByNumber",
                serde_json::json!(["finalized", false]),
            )
            .await["number"]
                != "0x3"
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
                "eth_getBlockByNumber",
                serde_json::json!(["safe", false])
            )
            .await["number"],
            "0x3"
        );
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
        let late = start_test_network(&mut late_config, late_partial);
        connect_peer(&late, &partial).await;
        partial.sync.announce_block(hash3, None);
        wait_peer(&late, hash3).await;
        wait_block(&http, &late_url, 3).await;
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
        let peer = start_test_network(&mut peer_config, peer_partial);
        connect_peer(&peer, &partial).await;
        wait_peer(&peer, hash4).await;
        wait_block(&http, &peer_url, 4).await;
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
