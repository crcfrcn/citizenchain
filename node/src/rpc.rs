//! 装配节点原生RPC、CID绑定查询及PoW账户签名交易入口。

#![warn(missing_docs)]
use codec::Decode;
use codec::Encode;
use jsonrpsee::RpcModule;
use nodedesktop::security::consume_miner_transfer_token;
use nodemining::pow::POW_AUTHOR_KEY_TYPE;
use runtime::opaque::Block;
use runtime::AccountId;
use runtime::Balance;
use runtime::Nonce;
use sc_client_api::StorageProvider;
use sc_transaction_pool_api::TransactionPool;
use sc_transaction_pool_api::TransactionSource;
use sp_api::Core as CoreApi;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::Error as BlockChainError;
use sp_blockchain::HeaderBackend;
use sp_blockchain::HeaderMetadata;
use sp_keystore::Keystore;
use sp_runtime::OpaqueExtrinsic;

use std::sync::Arc;
use std::sync::Mutex;

use substrate_frame_rpc_system::AccountNonceApi;

/// 在同一既有finalized快照下严格读取最小绑定；不读取注册历史或投票等级。
pub(crate) fn identity_binding(
    cid: &identity::CidNumberBound,
    account: &AccountId,
    genesis: runtime::Hash,
    info: &sp_blockchain::Info<Block>,
    mut read: impl FnMut(
        runtime::Hash,
        Vec<u8>,
    ) -> Result<Option<Vec<u8>>, jsonrpsee::types::ErrorObjectOwned>,
) -> Result<Option<serde_json::Value>, jsonrpsee::types::ErrorObjectOwned> {
    if info.genesis_hash != genesis {
        return Err(jsonrpsee::types::ErrorObjectOwned::owned(
            -32013,
            "chain_genesis_mismatch",
            None::<()>,
        ));
    }
    let at = info.finalized_hash;
    fn decoded<T: Decode>(
        raw: Option<Vec<u8>>,
    ) -> Result<Option<T>, jsonrpsee::types::ErrorObjectOwned> {
        raw.map(|bytes| {
            let mut input = bytes.as_slice();
            let value = T::decode(&mut input).map_err(|_| {
                jsonrpsee::types::ErrorObjectOwned::owned(
                    -32010,
                    "identity_state_unavailable",
                    None::<()>,
                )
            })?;
            if !input.is_empty() {
                return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                    -32010,
                    "identity_state_unavailable",
                    None::<()>,
                ));
            }
            Ok(value)
        })
        .transpose()
    }
    let registry = decoded::<identity::CidRecord<u32>>(read(
        at,
        identity::CidRegistry::<runtime::Runtime>::hashed_key_for(cid),
    )?)?;
    let Some(registry) = registry else {
        return Ok(None);
    };
    if registry.status != identity::CidRecordStatus::Active || registry.revoked_at.is_some() {
        return Err(jsonrpsee::types::ErrorObjectOwned::owned(
            -32011,
            "identity_revoked",
            None::<()>,
        ));
    }
    let forward = decoded::<AccountId>(read(
        at,
        identity::AccountIdByCid::<runtime::Runtime>::hashed_key_for(cid),
    )?)?;
    let reverse = decoded::<identity::CidNumberBound>(read(
        at,
        identity::CidByAccountId::<runtime::Runtime>::hashed_key_for(account),
    )?)?;
    let revision = decoded::<u64>(read(
        at,
        identity::BindingRevisionByCid::<runtime::Runtime>::hashed_key_for(cid),
    )?)?;
    if forward.as_ref() != Some(account)
        || reverse.as_ref() != Some(cid)
        || !revision.is_some_and(|v| v > 0)
    {
        return Err(jsonrpsee::types::ErrorObjectOwned::owned(
            -32012,
            "identity_binding_mismatch",
            None::<()>,
        ));
    }
    let cid = std::str::from_utf8(cid.as_slice()).map_err(|_| {
        jsonrpsee::types::ErrorObjectOwned::owned(-32010, "identity_state_unavailable", None::<()>)
    })?;
    Ok(Some(
        serde_json::json!({"block":format!("0x{}",hex::encode(at.as_bytes())),"cid_number":cid,"account_id":format!("0x{}",hex::encode(account.encode())),"bind_rev":revision}),
    ))
}


/// Full client dependencies.
pub(crate) struct FullDeps<C, P> {
    /// The client instance to use.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
    /// Keystore（用于签名奖励账户绑定交易和矿工热账户交易）。
    pub keystore: sp_keystore::KeystorePtr,
    /// 同一矿工的 nonce 查询、签名和入池共用一把锁。
    pub miner_signing: Arc<Mutex<()>>,
    /// CPU 哈希率查询函数（hashes/sec）。
    pub cpu_hashrate_fn: fn() -> f64,
    /// GPU 哈希率查询函数（仅在 gpu-mining feature 启用且有 GPU 时为 Some）。
    pub gpu_hashrate_fn: Option<fn() -> f64>,
    /// 清算行节点的 RPC 命名空间实现。
    /// None 表示本节点未以清算行角色启动,跳过 `offchain_*` RPC 注入。
    pub offchain_clearing_rpc: Option<Arc<nodeoffchain::rpc::OffchainClearingRpcImpl>>,
}

/// 矿工 RPC 与桌面维护共用 V5 签名及正常入池路径；返回官方交易状态流。
pub(crate) fn submit_powr_signed_tx<C, P>(
    client: &Arc<C>,
    pool: &Arc<P>,
    keystore: &sp_keystore::KeystorePtr,
    miner_signing: &Mutex<()>,
    call: runtime::RuntimeCall,
    expected_payer: Option<&AccountId>,
) -> Result<
    (
        String,
        std::pin::Pin<Box<sc_transaction_pool_api::TransactionStatusStreamFor<P>>>,
    ),
    jsonrpsee::types::ErrorObjectOwned,
>
where
    C: ProvideRuntimeApi<Block> + Send + Sync,
    C: HeaderBackend<Block> + 'static,
    C::Api: AccountNonceApi<Block, AccountId, Nonce> + CoreApi<Block> + BlockBuilder<Block>,
    P: TransactionPool<Block = Block> + 'static,
{
    use jsonrpsee::types::error::ErrorObject;
    use substrate_frame_rpc_system::{System, SystemApiServer};

    // 持锁直到正常交易池确认接收，防止热钱包与维护备注同时取得同一 nonce。
    let _signing = miner_signing
        .lock()
        .map_err(|_| ErrorObject::owned(-1, "矿工签名锁异常", None::<()>))?;

    // 1. 从 keystore 取 powr 公钥
    let keys = keystore.sr25519_public_keys(POW_AUTHOR_KEY_TYPE);
    let public = keys
        .first()
        .copied()
        .ok_or_else(|| ErrorObject::owned(-1, "未找到矿工密钥，请先启动节点", None::<()>))?;

    // 2. 推导 AccountId
    let miner_account_id: AccountId = chain_signing::account_id_from_public(public);
    if expected_payer.is_some_and(|payer|payer!=&miner_account_id) {
        return Err(ErrorObject::owned(-1,"矿工签名账户已改变，请重新选择付款钱包",None::<()>));
    }

    // 3. 查询链信息
    let info = (*client).info();
    let best_hash = info.best_hash;
    let genesis_hash = client
        .hash(0)
        .map_err(|e| ErrorObject::owned(-1, format!("查询创世块哈希失败: {e}"), None::<()>))?
        .ok_or_else(|| ErrorObject::owned(-1, "创世块不存在", None::<()>))?;

    // 4. 使用官方 System 接口，计入本账户在交易池中的连续 nonce。
    let nonce = futures::executor::block_on(
        System::new(client.clone(), pool.clone()).nonce(miner_account_id),
    )?;

    // 4b. 查询链上 WASM 运行时的版本号（不使用 native 编译时常量，
    //     避免 spec_version 升级后 native 与链上 WASM 不一致导致 BadProof）
    let on_chain_version = client
        .runtime_api()
        .version(best_hash)
        .map_err(|e| ErrorObject::owned(-1, format!("查询运行时版本失败: {e}"), None::<()>))?;

    // 5. 使用共享签名材料，版本号取链上 WASM。
    let material = chain_signing::build_signing_material_from_call(
        call,
        genesis_hash,
        nonce,
        on_chain_version.spec_version,
        on_chain_version.transaction_version,
    );
    let signature = keystore.sr25519_sign(
        POW_AUTHOR_KEY_TYPE,
        &public,
        &material.signing_message(&public.0),
    );
    let signature = signature
        .map_err(|e| ErrorObject::owned(-1, format!("keystore 签名失败: {e}"), None::<()>))?
        .ok_or_else(|| ErrorObject::owned(-1, "keystore 未返回签名", None::<()>))?;

    // 6. 组装 UncheckedExtrinsic
    let extrinsic = chain_signing::assemble_signed_extrinsic(material, public, signature);

    // 7. 编码并提交到交易池
    let encoded = extrinsic.encode();
    let opaque = OpaqueExtrinsic::try_from_encoded_extrinsic(&encoded)
        .map_err(|_| ErrorObject::owned(-1, "交易编码失败", None::<()>))?;

    let tx_hash = pool.hash_of(&opaque);
    // 官方状态流用于维护任务等待最终性；普通 RPC 返回后即可释放其观察者。
    let status = futures::executor::block_on(pool.submit_and_watch(
        best_hash,
        TransactionSource::Local,
        opaque,
    ))
    .map_err(|e| ErrorObject::owned(-1, format!("提交交易到交易池失败: {e}"), None::<()>))?;

    Ok((format!("{tx_hash:?}"), status))
}

/// 从唯一文本格式解析 AccountId：小写 `0x` + 64 位十六进制。
fn parse_account_id(account_id: &str) -> Result<AccountId, jsonrpsee::types::ErrorObjectOwned> {
    use jsonrpsee::types::error::ErrorObject;
    if account_id.len() != 66
        || !account_id.starts_with("0x")
        || !account_id[2..]
            .chars()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
    {
        return Err(ErrorObject::owned(
            -1,
            "账户 ID 格式无效，应为小写 0x + 64 位十六进制",
            None::<()>,
        ));
    }
    let bytes = hex::decode(&account_id[2..])
        .map_err(|e| ErrorObject::owned(-1, format!("账户 ID 解码失败: {e}"), None::<()>))?;
    let raw: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ErrorObject::owned(-1, "账户 ID 必须是 32 字节", None::<()>))?;
    Ok(AccountId::from(raw))
}

/// 构造 smoldot 轻节点 checkpoint。
///
/// CitizenApp 的 chainspec 只保留 `genesis.stateRootHash`,不携带完整创世存储。
/// smoldot 因此还需要一个 finalized header + GRANDPA authority set 作为轻节点
/// 启动锚点；这里只返回这两个字段,避免把完整 chainspec 塞进 RPC 响应触发大小限制。
fn build_light_sync_state<C>(
    client: &Arc<C>,
) -> Result<serde_json::Value, jsonrpsee::types::ErrorObjectOwned>
where
    C: HeaderBackend<Block> + StorageProvider<Block, sc_service::TFullBackend<Block>> + 'static,
{
    use jsonrpsee::types::error::ErrorObject;

    let finalized_hash = client.info().finalized_hash;
    let finalized_header = client
        .header(finalized_hash)
        .map_err(|e| {
            ErrorObject::owned(-1, format!("获取 finalized header 失败: {e}"), None::<()>)
        })?
        .ok_or_else(|| ErrorObject::owned(-1, "finalized header 不存在", None::<()>))?;
    let finalized_header_hex = format!("0x{}", hex::encode(finalized_header.encode()));

    // Grandpa::CurrentSetId 的 storage key = twox_128("Grandpa") ++ twox_128("CurrentSetId")。
    let grandpa_set_id_key = {
        let mut k = Vec::new();
        k.extend_from_slice(&sp_io::hashing::twox_128(b"Grandpa"));
        k.extend_from_slice(&sp_io::hashing::twox_128(b"CurrentSetId"));
        k
    };
    let set_id_bytes = client
        .storage(finalized_hash, &sp_storage::StorageKey(grandpa_set_id_key))
        .map_err(|e| ErrorObject::owned(-1, format!("读取 GRANDPA set_id 失败: {e}"), None::<()>))?
        .ok_or_else(|| ErrorObject::owned(-1, "GRANDPA set_id 不存在", None::<()>))?;
    let set_id = u64::decode(&mut &set_id_bytes.0[..]).map_err(|e| {
        ErrorObject::owned(-1, format!("解码 GRANDPA set_id 失败: {e}"), None::<()>)
    })?;

    // Grandpa::Authorities 已经是 Vec<(AuthorityId, u64)> 的 SCALE 编码。
    let grandpa_authorities_key = {
        let mut k = Vec::new();
        k.extend_from_slice(&sp_io::hashing::twox_128(b"Grandpa"));
        k.extend_from_slice(&sp_io::hashing::twox_128(b"Authorities"));
        k
    };
    let auth_bytes = client
        .storage(
            finalized_hash,
            &sp_storage::StorageKey(grandpa_authorities_key),
        )
        .map_err(|e| {
            ErrorObject::owned(
                -1,
                format!("读取 GRANDPA authorities 失败: {e}"),
                None::<()>,
            )
        })?
        .ok_or_else(|| ErrorObject::owned(-1, "GRANDPA authorities 不存在", None::<()>))?;
    if auth_bytes.0.is_empty() {
        return Err(ErrorObject::owned(
            -1,
            "GRANDPA authorities 为空,无法生成 lightSyncState",
            None::<()>,
        ));
    }

    // smoldot 的 grandpaAuthoritySet 期望完整 AuthoritySet 编码:
    // Vec<(AuthorityId, u64)> + set_id + 空 pending_standard_changes/
    // pending_forced_changes/authority_set_changes。
    let set_id_encoded = set_id.encode();
    let mut combined = Vec::with_capacity(auth_bytes.0.len() + set_id_encoded.len() + 4);
    combined.extend_from_slice(&auth_bytes.0);
    combined.extend_from_slice(&set_id_encoded);
    combined.push(0x00u8); // ForkTree roots: Compact<0>
    combined.push(0x00u8); // ForkTree best_finalized_number: Option::None
    combined.push(0x00u8); // Vec<PendingChange>: Compact<0>
    combined.push(0x00u8); // Vec<(u64, u32)>: Compact<0>

    Ok(serde_json::json!({
        "finalizedBlockHeader": finalized_header_hex,
        "grandpaAuthoritySet": format!("0x{}", hex::encode(&combined)),
    }))
}

/// Instantiate all full RPC extensions.
pub(crate) fn create_full<C, P>(
    deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
    C: StorageProvider<Block, sc_service::TFullBackend<Block>> + sc_client_api::BlockBackend<Block> + 'static,
    C: Send + Sync + 'static,
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: BlockBuilder<Block>,
    C::Api: CoreApi<Block> + sp_api::Metadata<Block>,
    P: TransactionPool<Block = Block> + 'static,
{
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut module = RpcModule::new(());
    let FullDeps {
        client,
        pool,
        keystore,
        miner_signing,
        cpu_hashrate_fn,
        gpu_hashrate_fn,
        offchain_clearing_rpc,
    } = deps;

    // 若清算行组件已启动,合并 offchain_* RPC 命名空间。
    if let Some(impl_) = offchain_clearing_rpc {
        use nodeoffchain::rpc::OffchainClearingRpcServer;
        module
            .merge(OffchainClearingRpcServer::into_rpc((*impl_).clone()))
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(format!(
                    "合并 offchain 清算 RPC 失败:{e:?}"
                )))
            })?;
    }

    // 此只读方法只聚合本地既有存储；不增加端口、不改变共识或最终性规则。
    {
        let client = client.clone();
        module.register_method("identity_getBinding", move |params, _, _| {
            let (cid, account, genesis): (String, String, String) = params.parse()?;
            let cid: identity::CidNumberBound = cid.into_bytes().try_into().map_err(|_| {
                jsonrpsee::types::ErrorObjectOwned::owned(-32602, "invalid_cid", None::<()>)
            })?;
            if cid.is_empty() || !cid.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-') {
                return Err(jsonrpsee::types::ErrorObjectOwned::owned(
                    -32602,
                    "invalid_cid",
                    None::<()>,
                ));
            }
            let account_bytes = account
                .strip_prefix("0x")
                .and_then(|s| hex::decode(s).ok())
                .and_then(|v| <[u8; 32]>::try_from(v).ok())
                .ok_or_else(|| {
                    jsonrpsee::types::ErrorObjectOwned::owned(-32602, "invalid_account", None::<()>)
                })?;
            let account = AccountId::new(account_bytes);
            let genesis: runtime::Hash = genesis.parse().map_err(|_| {
                jsonrpsee::types::ErrorObjectOwned::owned(-32602, "invalid_genesis", None::<()>)
            })?;
            let info = client.info();
            identity_binding(&cid, &account, genesis, &info, |at, key| {
                client
                    .storage(at, &sp_storage::StorageKey(key))
                    .map(|v| v.map(|v| v.0))
                    .map_err(|_| {
                        jsonrpsee::types::ErrorObjectOwned::owned(
                            -32010,
                            "identity_state_unavailable",
                            None::<()>,
                        )
                    })
            })
        })?;
    }

    module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
    module.merge(TransactionPayment::new(client.clone()).into_rpc())?;

    // 公民宪法 RPC：直接 RAW 读链上立法院模块存储(law_id=0,tier=宪法)的**当前生效版本**,
    // 据 章>节>条>款 + 中英双语重建为 HTML(复用原 CSS 外壳,样式与迁移前一致)。
    // 故意不走 runtime API —— API 属可升级 runtime,恶意升级可伪造返回;RAW 读的正是 L2 守卫
    // 所保护的存储。读显式 effective_version,避免提前展示修宪待生效版(立法与宪法协议)。
    {
        let client = client.clone();
        use nodeguard::constitution::{self, CONSTITUTION_LAW_ID};
        module.register_method("constitution_getDocument", move |_params, _, _| {
            use jsonrpsee::types::error::ErrorObject;
            use sp_storage::StorageKey;

            let best_hash = client.info().best_hash;
            let raw =
                |key: Vec<u8>| -> Result<Option<Vec<u8>>, jsonrpsee::types::ErrorObjectOwned> {
                    client
                        .storage(best_hash, &StorageKey(key))
                        .map(|opt| opt.map(|d| d.0))
                        .map_err(|e| {
                            ErrorObject::owned(-1, format!("读取链上宪法存储失败: {e}"), None::<()>)
                        })
                };

            // 1. RAW 读 Law(0),解出显式 effective_version。
            let law_bytes =
                raw(constitution::storage_key::law(CONSTITUTION_LAW_ID))?.ok_or_else(|| {
                    ErrorObject::owned(-1, "链上宪法 Law 不存在(law_id=0)", None::<()>)
                })?;
            let version = nodeguard::constitution::render::effective_version_of_law(&law_bytes)
                .map_err(|e| ErrorObject::owned(-1, e, None::<()>))?;

            // 2. RAW 读该版本 LawVersion 字节。
            let version_bytes = raw(constitution::storage_key::law_version(
                CONSTITUTION_LAW_ID,
                version,
            ))?
            .ok_or_else(|| {
                ErrorObject::owned(-1, format!("链上宪法版本不存在(v{version})"), None::<()>)
            })?;
            let version_label_bytes = raw(constitution::storage_key::law_version_label(
                CONSTITUTION_LAW_ID,
                version,
            ))?;

            // 3. RAW 读不可修改条款 manifest,用于展示「不可修改条款」徽章。
            let manifest_bytes = raw(constitution::storage_key::manifest())?.ok_or_else(|| {
                ErrorObject::owned(-1, "链上宪法不可修改条款 manifest 不存在", None::<()>)
            })?;
            let immutable_article_numbers =
                nodeguard::constitution::render::immutable_article_numbers(&manifest_bytes)
                    .map_err(|e| ErrorObject::owned(-1, e, None::<()>))?;

            // 4. 重建 HTML 并按内容计算摘要。
            let html = nodeguard::constitution::render::render_constitution_html(
                &version_bytes,
                &immutable_article_numbers,
                version_label_bytes.as_deref(),
            )
            .map_err(|e| ErrorObject::owned(-1, e, None::<()>))?;
            let digest = sp_crypto_hashing::blake2_256(html.as_bytes());

            Ok::<serde_json::Value, jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({
                "html": html,
                "blake2_256": format!("0x{}", hex::encode(digest)),
                "source": "legislation-raw",
            }))
        })?;
    }

    // sync_state_genLightSyncState: 返回小体积 checkpoint,供 CitizenApp 注入
    // smoldot chainspec。旧的 full spec 响应会超过 RPC 限制,这里不再返回完整 chainspec。
    {
        let client = client.clone();
        module.register_method("sync_state_genLightSyncState", move |_params, _, _| {
            build_light_sync_state(&client)
        })?;
    }

    // CPU 哈希率 RPC：mining_cpuHashrate
    // 返回值：当前 CPU 全线程合计哈希率（hashes/sec），u64 整数。
    module.register_method("mining_cpuHashrate", move |_, _, _| {
        cpu_hashrate_fn() as u64
    })?;

    // GPU 哈希率 RPC：mining_gpuHashrate
    // 返回值：当前 GPU 哈希率（hashes/sec），u64 整数。
    if let Some(get_hashrate) = gpu_hashrate_fn {
        module.register_method("mining_gpuHashrate", move |_, _, _| get_hashrate() as u64)?;
    }

    // reward_bindAccount(reward_account_id: String)
    // 由 node 端签名并提交 bind_reward_account 交易。
    {
        let client = client.clone();
        let pool = pool.clone();
        let keystore = keystore.clone();
        let miner_signing = miner_signing.clone();
        module.register_method("reward_bindAccount", move |params, _, _| {
            let reward_account_id: String = params.one()?;
            let reward_account_id = parse_account_id(&reward_account_id)?;
            let call = runtime::RuntimeCall::FullnodeIssue(
                fullnodeissue::pallet::Call::bind_reward_account { reward_account_id },
            );
            let _submission =
                submit_powr_signed_tx(&client, &pool, &keystore, &miner_signing, call, None)?;
            Ok::<&str, jsonrpsee::types::ErrorObjectOwned>("ok")
        })?;
    }

    // reward_rebindAccount(new_reward_account_id: String)
    // 由 node 端签名并提交 rebind_reward_account 交易。
    {
        let client = client.clone();
        let pool = pool.clone();
        let keystore = keystore.clone();
        let miner_signing = miner_signing.clone();
        module.register_method("reward_rebindAccount", move |params, _, _| {
            let new_reward_account_id: String = params.one()?;
            let new_reward_account_id = parse_account_id(&new_reward_account_id)?;
            let call = runtime::RuntimeCall::FullnodeIssue(
                fullnodeissue::pallet::Call::rebind_reward_account {
                    new_reward_account_id,
                },
            );
            let _submission =
                submit_powr_signed_tx(&client, &pool, &keystore, &miner_signing, call, None)?;
            Ok::<&str, jsonrpsee::types::ErrorObjectOwned>("ok")
        })?;
    }

    // transaction_submitMinerTransfer(account_id, mode, transfers, token) -> tx_hash
    // 复用唯一调用构造器及本机powr签名器，清单只有一次外层签名。
    {
        let client = client.clone();
        let pool = pool.clone();
        let keystore = keystore.clone();
        let miner_signing = miner_signing.clone();
        module.register_method("transaction_submitMinerTransfer", move |params, _, _| {
            use jsonrpsee::types::error::ErrorObject;

            let (account_id, mode, transfers, auth_token): (String,nodeonchain::types::TransferMode, Vec<nodeonchain::types::TransferItem>, String) = params.parse()?;
            if !consume_miner_transfer_token(&auth_token) {
                return Err(ErrorObject::owned(-1, "矿工热钱包提交令牌无效", None::<()>));
            }
            let payer=parse_account_id(&account_id)?;
            let call = nodeonchain::transfer::build_transfer_call(mode, &transfers)
                .map_err(|e| ErrorObject::owned(-32602, e, None::<()>))?;
            let (tx_hash, _) =
                submit_powr_signed_tx(&client, &pool, &keystore, &miner_signing, call, Some(&payer))?;
            Ok::<String, jsonrpsee::types::ErrorObjectOwned>(tx_hash)
        })?;
    }

    {
        let client=client.clone();
        module.register_method("transaction_transferPolicy",move |_params,_,_| {
            use sp_api::Metadata;
            let invalid=|e:String|jsonrpsee::types::ErrorObjectOwned::owned(-1,e,None::<()>);
            let at=client.info().finalized_hash;
            let metadata=client.runtime_api().metadata(at).map_err(|e|invalid(e.to_string()))?;
            let (execution,mut batch)=nodeguard::guard::runtime_policy::transfer_policy(metadata.as_slice()).map_err(invalid)?;
            if batch {
                let errors=std::cell::RefCell::new(None);
                let state=runtime::guard_migration_state(|key|match client.storage(at,&sp_storage::StorageKey(key.to_vec())) {Ok(v)=>v.map(|v|v.0),Err(e)=>{*errors.borrow_mut()=Some(e.to_string());None}}).map_err(|e|invalid(format!("迁移状态非法: {e:?}")))?;
                if let Some(error)=errors.into_inner(){return Err(invalid(error));}
                batch=state!=runtime::MigrationState::Running;
            }
            Ok::<_,jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({"executionFee":execution,"batchEnabled":batch}))
        })?;
    }

    // 最终块分页查询；不新建索引、不签名、不重发。
    {
        let client = client.clone();
        module.register_method("transaction_getTransferResult", move |params, _, _| {
            use sp_api::Metadata;
            use sp_runtime::traits::Header;
            let (tx_hash, cursor, limit): (String, u32, u32) = params.parse()?;
            let invalid = |message: String| jsonrpsee::types::ErrorObjectOwned::owned(-32602,message,None::<()>);
            let wanted = tx_hash.strip_prefix("0x").and_then(|s| hex::decode(s).ok()).filter(|v|v.len()==32).ok_or_else(||invalid("交易哈希格式非法".into()))?;
            if limit == 0 || limit > 32 { return Err(invalid("查询分页必须在1到32块之间".into())); }
            let info = client.info();
            let last = cursor.saturating_add(limit).min(info.finalized_number);
            for number in cursor.saturating_add(1)..=last {
                let hash = client.hash(number).map_err(|e|invalid(e.to_string()))?.ok_or_else(||invalid("最终块缺失".into()))?;
                let header = client.header(hash).map_err(|e|invalid(e.to_string()))?.ok_or_else(||invalid("最终块头缺失".into()))?;
                let body = client.block_body(hash).map_err(|e|invalid(e.to_string()))?.ok_or_else(||invalid("归档区块体缺失".into()))?;
                for (index, xt) in body.iter().enumerate() {
                    if sp_io::hashing::blake2_256(&xt.encode()).as_slice() != wanted.as_slice() { continue; }
                    let metadata = client.runtime_api().metadata(*header.parent_hash()).map_err(|e|invalid(e.to_string()))?;
                    let raw = client.storage(hash,&sp_storage::StorageKey(nodeguard::guard::runtime_policy::storage_key::events())).map_err(|e|invalid(e.to_string()))?.ok_or_else(||invalid("最终块事件缺失".into()))?;
                    let result = nodeguard::guard::runtime_policy::transfer_outcome(xt,&raw.0,metadata.as_slice(),index as u32).map_err(invalid)?;
                    let fees = result.fees.iter().try_fold(0u128,|s,f|s.checked_add(*f)).ok_or_else(||invalid("实际费用合计溢出".into()))?;
                    return Ok::<_,jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({
                        "state":result.state,"cursor":number,"blockHash":format!("{hash:#x}"),"index":index,"feeFen":fees.to_string(),"error":result.error,
                        "items":result.items.iter().zip(result.fees.iter()).map(|(state,fee)|serde_json::json!({"state":state,"feeFen":fee.to_string()})).collect::<Vec<_>>()
                    }));
                }
            }
            Ok(serde_json::json!({"state":"pending","cursor":last.max(cursor)}))
        })?;
    }

    // 从本机最终链倒序读取钱包记录；有界分页，无本地索引或写入。
    {
        let client = client.clone();
        module.register_method("transaction_getHistory", move |params, _, _| {
            use sp_api::Metadata;
            use sp_runtime::traits::Header;
            use nodeonchain::types::{TransactionCursor, TransactionHistory, TransactionRecord};
            let (accounts, cursor): (Vec<String>, Option<TransactionCursor>) = params.parse()?;
            let invalid=|message:String|jsonrpsee::types::ErrorObjectOwned::owned(-32602,message,None::<()>);
            let accounts=accounts.iter().map(|account| {
                account.strip_prefix("0x").and_then(|value|hex::decode(value).ok()).and_then(|value|<[u8;32]>::try_from(value).ok()).ok_or_else(||invalid("交易记录钱包账户无效".into()))
            }).collect::<Result<Vec<_>,_>>()?;
            if accounts.is_empty(){return Ok(TransactionHistory{records:Vec::new(),cursor:None});}
            transaction_history_page(client.info().finalized_number,cursor,|number| {
                let hash=client.hash(number).map_err(|error|error.to_string())?.ok_or("历史最终块缺失")?;
                let header=client.header(hash).map_err(|error|error.to_string())?.ok_or("历史块头缺失")?;
                let body=client.block_body(hash).map_err(|error|error.to_string())?.ok_or("历史区块体缺失")?;
                let metadata=client.runtime_api().metadata(*header.parent_hash()).map_err(|error|error.to_string())?;
                let raw=client.storage(hash,&sp_storage::StorageKey(nodedesktop::storage::prefix(b"System",b"Events"))).map_err(|error|error.to_string())?.ok_or("历史事件缺失")?;
                let time=client.storage(hash,&sp_storage::StorageKey(nodedesktop::storage::prefix(b"Timestamp",b"Now"))).map_err(|error|error.to_string())?.ok_or("历史时间缺失")?;
                let timestamp=u64::decode(&mut time.0.as_slice()).map_err(|error|error.to_string())?;
                if time.0.len()!=8 {return Err("历史时间格式无效".into());}
                let activities=nodeclient::codec::transaction_activity(&body,&raw.0,metadata.as_slice(),&accounts)?;
                let event_count=activities.event_count;
                let records=activities.records.into_iter().map(|activity| {
                    let index=activity.event_index;
                    let record=TransactionRecord{block_number:number,timestamp,tx_hash:activity.extrinsic_index.map(|index|format!("0x{}",hex::encode(sp_io::hashing::blake2_256(&body[index as usize].encode())))),
                        event_index:index,account_id:format!("0x{}",hex::encode(activity.account_id)),
                        direction:activity.direction.into(),mode:activity.mode,transfer_count:activity.transfer_count,
                        amount_fen:activity.amount_fen.to_string(),fee_fen:activity.fee_fen.to_string(),state:activity.state.into()};
                    (index,record)
                }).collect();
                Ok(HistoryBlock{event_count,records})
            }).map_err(invalid)
        })?;
    }

    // fee_blockFees(block_hash_hex: String) -> u128
    // 读取指定区块的 System::Events，累加所有 FeePaid.fee。
    // tip 在协议层强制为零，不再读取框架 TransactionFeePaid 事件拼接第二种口径。
    {
        let client = client.clone();
        module.register_method("fee_blockFees", move |params, _, _| {
            use jsonrpsee::types::error::ErrorObject;

            let hash_hex: String = params.one()?;
            let hash_bytes = hex::decode(hash_hex.trim_start_matches("0x"))
                .map_err(|e| ErrorObject::owned(-1, format!("无效区块哈希: {e}"), None::<()>))?;
            if hash_bytes.len() != 32 {
                return Err(ErrorObject::owned(-1, "区块哈希长度错误", None::<()>));
            }
            let block_hash = sp_core::H256::from_slice(&hash_bytes);

            // System::Events 的 storage key = twox_128("System") ++ twox_128("Events")
            let key = {
                let mut k = Vec::with_capacity(32);
                k.extend_from_slice(&sp_io::hashing::twox_128(b"System"));
                k.extend_from_slice(&sp_io::hashing::twox_128(b"Events"));
                k
            };
            let storage = client
                .storage(block_hash, &sp_storage::StorageKey(key))
                .map_err(|e| ErrorObject::owned(-1, format!("读取存储失败: {e}"), None::<()>))?;

            let Some(data) = storage else {
                return Ok(0u128);
            };

            type EventRecord = frame_system::EventRecord<runtime::RuntimeEvent, sp_core::H256>;
            let events: Vec<EventRecord> = Decode::decode(&mut &data.0[..])
                .map_err(|e| ErrorObject::owned(-1, format!("解码事件失败: {e}"), None::<()>))?;

            let mut total_fee: u128 = 0;
            for record in &events {
                if let runtime::RuntimeEvent::Onchain(onchain::pallet::Event::FeePaid {
                    fee, ..
                }) = &record.event
                {
                    // FeePaid.fee 就是完整链上交易费或投票费。
                    total_fee = total_fee.saturating_add(*fee);
                }
            }

            Ok(total_fee)
        })?;
    }

    Ok(module)
}


/// 一次块读取的原始事件数及相关收支，仅用于本机历史分页。
pub(crate) struct HistoryBlock {
    pub event_count: usize,
    pub records: Vec<(u32, nodeonchain::types::TransactionRecord)>,
}

/// 最多读取16个最终块；约20条收支为一页，同一事件的收支两侧不会在边界漏掉。
pub(crate) fn transaction_history_page(
    finalized: u32, cursor: Option<nodeonchain::types::TransactionCursor>,
    mut read: impl FnMut(u32) -> Result<HistoryBlock,String>,
) -> Result<nodeonchain::types::TransactionHistory,String> {
    use nodeonchain::types::{TransactionCursor,TransactionHistory};
    let upper=cursor.as_ref().map_or(finalized,|cursor|cursor.block_number);
    if upper>finalized || upper==0 && cursor.as_ref().is_some_and(|cursor|cursor.event_index.is_some()) {return Err("交易记录游标超出最终链".into());}
    let lower=upper.saturating_sub(15).max(1);
    let mut records=Vec::new();
    for number in (lower..=upper).rev() {
        let block=read(number)?;
        let start=cursor.as_ref().filter(|cursor|cursor.block_number==number).and_then(|cursor|cursor.event_index);
        if start.is_some_and(|index|index as usize>=block.event_count){return Err("历史事件索引无效".into());}
        let mut pending=block.records.into_iter().rev().filter(|(index,_)|start.is_none_or(|start|*index<=start)).peekable();
        while let Some((index,record))=pending.next() {
            records.push(record);
            if records.len()>=20 && pending.peek().is_none_or(|(next,_)|*next!=index) {
                let cursor=if index>0 {Some(TransactionCursor{block_number:number,event_index:Some(index-1)})} else if number>1 {Some(TransactionCursor{block_number:number-1,event_index:None})} else {None};
                return Ok(TransactionHistory{records,cursor});
            }
        }
    }
    Ok(TransactionHistory{records,cursor:if lower>1 {Some(TransactionCursor{block_number:lower-1,event_index:None})} else {None}})
}
