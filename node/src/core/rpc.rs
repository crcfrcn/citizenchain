//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, OnceLock},
};

use citizenchain::{self as runtime, opaque::Block, AccountId, Balance, Nonce};
use codec::{Decode, Encode};
use jsonrpsee::RpcModule;
use sc_client_api::StorageProvider;
use sc_transaction_pool_api::{TransactionPool, TransactionSource};
use sp_api::Core as CoreApi;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_keystore::Keystore;
use sp_runtime::OpaqueExtrinsic;
use substrate_frame_rpc_system::AccountNonceApi;

use crate::core::service::POW_AUTHOR_KEY_TYPE;
const MAX_TRANSFER_REMARK_BYTES: usize = 99;
static MINER_TRANSFER_TOKENS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// 签发一次性矿工热钱包转账令牌。
///
/// 令牌只在当前进程内保存，供 Tauri 命令在完成设备密码校验后调用本机 RPC。
pub(crate) fn issue_miner_transfer_token() -> Result<String, String> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    let tokens = MINER_TRANSFER_TOKENS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = tokens
        .lock()
        .map_err(|_| "矿工热钱包令牌状态异常".to_string())?;
    guard.insert(token.clone());
    Ok(token)
}

/// 回收尚未被 RPC 消费的一次性矿工热钱包转账令牌。
pub(crate) fn revoke_miner_transfer_token(token: &str) {
    let Some(tokens) = MINER_TRANSFER_TOKENS.get() else {
        return;
    };
    if let Ok(mut guard) = tokens.lock() {
        guard.remove(token);
    }
}

fn consume_miner_transfer_token(token: &str) -> bool {
    let Some(tokens) = MINER_TRANSFER_TOKENS.get() else {
        return false;
    };
    tokens
        .lock()
        .map(|mut guard| guard.remove(token))
        .unwrap_or(false)
}

/// Full client dependencies.
pub struct FullDeps<C, P> {
    /// The client instance to use.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
    /// Keystore（用于签名奖励账户绑定交易和矿工热账户交易）。
    pub keystore: sp_keystore::KeystorePtr,
    /// CPU 哈希率查询函数（hashes/sec）。
    pub cpu_hashrate_fn: fn() -> f64,
    /// GPU 哈希率查询函数（仅在 gpu-mining feature 启用且有 GPU 时为 Some）。
    pub gpu_hashrate_fn: Option<fn() -> f64>,
    /// 清算行节点的 RPC 命名空间实现。
    /// None 表示本节点未以清算行角色启动,跳过 `offchain_*` RPC 注入。
    pub offchain_clearing_rpc:
        Option<Arc<crate::transaction::offchain::rpc::OffchainClearingRpcImpl>>,
}

/// 构造并签名一笔 powr 矿工交易，提交到交易池。
fn submit_powr_signed_tx<C, P>(
    client: &Arc<C>,
    pool: &Arc<P>,
    keystore: &sp_keystore::KeystorePtr,
    call: runtime::RuntimeCall,
) -> Result<String, jsonrpsee::types::ErrorObjectOwned>
where
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + 'static,
    C::Api: AccountNonceApi<Block, AccountId, Nonce> + CoreApi<Block>,
    P: TransactionPool<Block = Block> + 'static,
{
    use jsonrpsee::types::error::ErrorObject;

    // 1. 从 keystore 取 powr 公钥
    let keys = keystore.sr25519_public_keys(POW_AUTHOR_KEY_TYPE);
    let public = keys
        .first()
        .copied()
        .ok_or_else(|| ErrorObject::owned(-1, "未找到矿工密钥，请先启动节点", None::<()>))?;

    // 2. 推导 AccountId
    let miner_account_id: AccountId = chain_signing::account_id_from_public(public);

    // 3. 查询链信息
    let info = (*client).info();
    let best_hash = info.best_hash;
    let genesis_hash = client
        .hash(0)
        .map_err(|e| ErrorObject::owned(-1, format!("查询创世块哈希失败: {e}"), None::<()>))?
        .ok_or_else(|| ErrorObject::owned(-1, "创世块不存在", None::<()>))?;

    // 4. 查询 nonce
    let nonce = client
        .runtime_api()
        .account_nonce(best_hash, miner_account_id.clone())
        .map_err(|e| ErrorObject::owned(-1, format!("查询账户 nonce 失败: {e}"), None::<()>))?;

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
    let signature = keystore.sr25519_sign(POW_AUTHOR_KEY_TYPE, &public, &material.signing_bytes);
    let signature = signature
        .map_err(|e| ErrorObject::owned(-1, format!("keystore 签名失败: {e}"), None::<()>))?
        .ok_or_else(|| ErrorObject::owned(-1, "keystore 未返回签名", None::<()>))?;

    // 6. 组装 UncheckedExtrinsic
    let extrinsic = chain_signing::assemble_signed_extrinsic(material, public, signature);

    // 7. 编码并提交到交易池
    let encoded = extrinsic.encode();
    let opaque = OpaqueExtrinsic::try_from_encoded_extrinsic(&encoded)
        .map_err(|_| ErrorObject::owned(-1, "交易编码失败", None::<()>))?;

    // submit_one 是 async，但我们在同步上下文中，使用 futures::executor::block_on
    let tx_hash =
        futures::executor::block_on(pool.submit_one(best_hash, TransactionSource::Local, opaque))
            .map_err(|e| ErrorObject::owned(-1, format!("提交交易到交易池失败: {e}"), None::<()>))?;

    Ok(format!("{tx_hash:?}"))
}

/// 从 SS58 地址解析 AccountId。
fn account_id_from_ss58_address(
    ss58_address: &str,
) -> Result<AccountId, jsonrpsee::types::ErrorObjectOwned> {
    use sp_core::crypto::Ss58Codec;
    sp_runtime::AccountId32::from_ss58check(ss58_address).map_err(|e| {
        jsonrpsee::types::error::ErrorObject::owned(
            -1,
            format!("SS58 地址解析失败: {e:?}"),
            None::<()>,
        )
    })
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
pub fn create_full<C, P>(
    deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
    C: StorageProvider<Block, sc_service::TFullBackend<Block>> + 'static,
    C: Send + Sync + 'static,
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: BlockBuilder<Block>,
    C::Api: CoreApi<Block>,
    P: TransactionPool<Block = Block> + 'static,
{
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut module = RpcModule::new(());
    let FullDeps {
        client,
        pool,
        keystore,
        cpu_hashrate_fn,
        gpu_hashrate_fn,
        offchain_clearing_rpc,
    } = deps;

    // 若清算行组件已启动,合并 offchain_* RPC 命名空间。
    if let Some(impl_) = offchain_clearing_rpc {
        use crate::transaction::offchain::rpc::OffchainClearingRpcServer;
        module
            .merge(OffchainClearingRpcServer::into_rpc((*impl_).clone()))
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(format!(
                    "合并 offchain 清算 RPC 失败:{e:?}"
                )))
            })?;
    }

    module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
    module.merge(TransactionPayment::new(client.clone()).into_rpc())?;

    // 公民宪法 RPC：直接 RAW 读链上立法院模块存储(law_id=0,tier=宪法)的**当前生效版本**,
    // 据 章>节>条>款 + 中英双语重建为 HTML(复用原 CSS 外壳,样式与迁移前一致)。
    // 故意不走 runtime API —— API 属可升级 runtime,恶意升级可伪造返回;RAW 读的正是 L2 守卫
    // 所保护的存储。读显式 effective_version,避免提前展示修宪待生效版(ADR-027 §6.1)。
    {
        let client = client.clone();
        use crate::core::constitution::{self, CONSTITUTION_LAW_ID};
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
            let version = constitution::effective_version_of_law(&law_bytes)
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
                constitution::immutable_article_numbers(&manifest_bytes)
                    .map_err(|e| ErrorObject::owned(-1, e, None::<()>))?;

            // 4. 重建 HTML 并按内容计算摘要。
            let html = constitution::render_constitution_html(
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
        module.register_method("reward_bindAccount", move |params, _, _| {
            let reward_account_id: String = params.one()?;
            let reward_account_id = parse_account_id(&reward_account_id)?;
            let call = runtime::RuntimeCall::FullnodeIssuance(
                fullnode_issuance::pallet::Call::bind_reward_account { reward_account_id },
            );
            let _tx_hash = submit_powr_signed_tx(&client, &pool, &keystore, call)?;
            Ok::<&str, jsonrpsee::types::ErrorObjectOwned>("ok")
        })?;
    }

    // reward_rebindAccount(new_reward_account_id: String)
    // 由 node 端签名并提交 rebind_reward_account 交易。
    {
        let client = client.clone();
        let pool = pool.clone();
        let keystore = keystore.clone();
        module.register_method("reward_rebindAccount", move |params, _, _| {
            let new_reward_account_id: String = params.one()?;
            let new_reward_account_id = parse_account_id(&new_reward_account_id)?;
            let call = runtime::RuntimeCall::FullnodeIssuance(
                fullnode_issuance::pallet::Call::rebind_reward_account {
                    new_reward_account_id,
                },
            );
            let _tx_hash = submit_powr_signed_tx(&client, &pool, &keystore, call)?;
            Ok::<&str, jsonrpsee::types::ErrorObjectOwned>("ok")
        })?;
    }

    // transaction_submitMinerTransfer(to_ss58_address, amount_fen, remark, token) -> tx_hash
    // 由 node 端使用本机 powr 矿工密钥签名并提交 OnchainTransaction::transfer_with_remark。
    {
        let client = client.clone();
        let pool = pool.clone();
        let keystore = keystore.clone();
        module.register_method("transaction_submitMinerTransfer", move |params, _, _| {
            use jsonrpsee::types::error::ErrorObject;

            let (to_ss58_address, amount_fen_raw, remark_raw, auth_token): (
                String,
                String,
                String,
                String,
            ) = params.parse()?;
            if !consume_miner_transfer_token(&auth_token) {
                return Err(ErrorObject::owned(-1, "矿工热钱包提交令牌无效", None::<()>));
            }
            let destination_account_id = account_id_from_ss58_address(&to_ss58_address)?;
            let amount_fen: Balance = amount_fen_raw.parse().map_err(|e| {
                ErrorObject::owned(-1, format!("转账金额解析失败: {e}"), None::<()>)
            })?;
            if amount_fen == 0 {
                return Err(ErrorObject::owned(-1, "转账金额不能为零", None::<()>));
            }
            let remark_len = remark_raw.len();
            if remark_len > MAX_TRANSFER_REMARK_BYTES {
                return Err(ErrorObject::owned(
                    -1,
                    format!(
                        "转账备注不能超过 {MAX_TRANSFER_REMARK_BYTES} 字节，当前 {remark_len} 字节"
                    ),
                    None::<()>,
                ));
            }
            let remark: onchain::pallet::TransferRemarkOf<runtime::Runtime> = remark_raw
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| ErrorObject::owned(-1, "转账备注长度超过链上限制", None::<()>))?;

            let call = runtime::RuntimeCall::OnchainTransaction(
                onchain::pallet::Call::transfer_with_remark {
                    beneficiary_account_id: destination_account_id,
                    amount: amount_fen,
                    remark,
                },
            );
            let tx_hash = submit_powr_signed_tx(&client, &pool, &keystore, call)?;
            Ok::<String, jsonrpsee::types::ErrorObjectOwned>(tx_hash)
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
                if let runtime::RuntimeEvent::OnchainTransaction(
                    onchain::pallet::Event::FeePaid { fee, .. },
                ) = &record.event
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
