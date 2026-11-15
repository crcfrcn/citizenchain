use codec::Decode;
use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;
use runtime::opaque::Block;
use sc_client_api::backend::Backend as _;
use sc_client_api::backend::TrieCacheContext;
use sc_client_api::StorageProvider;
use sc_consensus::BlockCheckParams;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ImportResult;
use sc_consensus::StateAction;
use sc_consensus::StorageChanges;
use sp_api::ApiExt;
use sp_api::Core;
use sp_api::Metadata;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::Block as BlockT;
use sp_runtime::traits::Header as HeaderT;
use sp_storage::StorageKey;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 除宪法外的节点永久规则统一导入包装器。
pub struct NodeGuard<I> {
    inner: I,
    client: Arc<FullClient>,
    backend: Arc<FullBackend>,
    cid_lifecycle: Option<crate::guard::cid_lifecycle::GenesisReference>,
}

/// 只读取数据库中的真实块 0 头，不使用创世 storage 中的 BlockHash(0) 占位值。
fn verify_genesis_identity<C: HeaderBackend<Block>>(client: &C) -> Result<(), String> {
    let expected_hash = sp_core::H256::from(primitives::genesis::GENESIS_HASH);
    let expected_root = sp_core::H256::from(primitives::genesis::GENESIS_STATE_ROOT);
    verify_genesis_identity_against(client, expected_hash, expected_root)
}

// 生产入口的期望值只来自冻结常量；共用核验不从客户端事实推导信任值。
fn verify_genesis_identity_against<C: HeaderBackend<Block>>(
    client: &C,
    expected_hash: sp_core::H256,
    expected_root: sp_core::H256,
) -> Result<(), String> {
    let recorded_hash = client.info().genesis_hash;
    if recorded_hash != expected_hash {
        return Err(format!(
            "客户端创世哈希不匹配：期望 {expected_hash:?}，实际 {recorded_hash:?}"
        ));
    }
    let indexed_hash = client
        .hash(0)
        .map_err(|error| format!("读取块 0 哈希失败：{error}"))?
        .ok_or_else(|| "块 0 哈希不存在".to_string())?;
    if indexed_hash != expected_hash {
        return Err(format!(
            "块 0 索引哈希不匹配：期望 {expected_hash:?}，实际 {indexed_hash:?}"
        ));
    }
    let header = client
        .header(indexed_hash)
        .map_err(|error| format!("读取创世区块头失败：{error}"))?
        .ok_or_else(|| "创世区块头不存在".to_string())?;
    if *header.number() != 0 {
        return Err(format!(
            "创世区块高度不匹配：期望 0，实际 {}",
            header.number()
        ));
    }
    if *header.state_root() != expected_root {
        return Err(format!(
            "创世状态根不匹配：期望 {expected_root:?}，实际 {:?}",
            header.state_root()
        ));
    }
    let computed_hash = header.hash();
    if computed_hash != expected_hash {
        return Err(format!(
            "创世区块头重算哈希不匹配：期望 {expected_hash:?}，实际 {computed_hash:?}"
        ));
    }
    Ok(())
}

/// 保持现有构造接口；创世身份失败立即中止装配，不能降级为启动警告。
fn enforce_genesis_identity<C: HeaderBackend<Block>>(client: &C) {
    if let Err(reason) = verify_genesis_identity(client) {
        log::error!(target: "node-guard", "创世身份校验失败，拒绝启动节点：{reason}");
        panic!("NodeGuard 拒绝启动：{reason}");
    }
}

/// 合法 finalize 原生发行仅登记金额，由 `NodeGuard` 核对总发行差额。
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct FinalizeIssuancePlan {
    total: u128,
}

/// 两层守卫共用的最终导入闸门：只有全部原生规则明确验证成功才允许调用内层导入器。
///
/// 把委派动作单独收口后，测试可以直接证明 `Err` 路径返回 `KnownBad` 且内层调用次数为零，
/// 避免某个包装器以后在日志分支中误调用内层导入器。
pub(crate) async fn import_if_verified<I>(
    inner: &I,
    params: BlockImportParams<Block>,
    verdict: Result<(), String>,
) -> Result<ImportResult, ConsensusError>
where
    I: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
    match verdict {
        Ok(()) => inner.import_block(params).await,
        Err(_) => Ok(ImportResult::KnownBad),
    }
}

/// 节点镜像的 `frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>`。
#[derive(Debug, Decode, codec::Encode, Eq, PartialEq)]
pub(super) struct MAccountInfo {
    nonce: u32,
    consumers: u32,
    providers: u32,
    sufficients: u32,
    data: MAccountData,
}

#[derive(Debug, Decode, codec::Encode, Eq, PartialEq)]
pub(super) struct MAccountData {
    free: u128,
    reserved: u128,
    frozen: u128,
    flags: u128,
}

fn decode_exact<T: Decode>(raw: &[u8], label: &str) -> Result<T, String> {
    let mut input = raw;
    let value = T::decode(&mut input).map_err(|_| format!("{label} SCALE 解码失败"))?;
    if !input.is_empty() {
        return Err(format!("{label} 存在尾随字节"));
    }
    Ok(value)
}

fn signed_delta(before: u128, after: u128) -> i128 {
    if after >= before {
        i128::try_from(after - before).unwrap_or(i128::MAX)
    } else {
        -i128::try_from(before - after).unwrap_or(i128::MAX)
    }
}

/// timestamp inherent 之外必须至少有一笔用户交易；在执行 runtime 前判断，避免空提案触发 panic。
fn has_user_transaction(extrinsic_count: usize) -> bool {
    extrinsic_count > 1
}

/// finalize 总发行增量必须等于各合法发行来源登记的金额。
fn verify_finalize_issuance<FPre, FPost>(
    pre: &FPre,
    post: &FPost,
    plan: &FinalizeIssuancePlan,
) -> Result<(), String>
where
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let total_key = crate::guard::fullnode_issuance::storage_key::total_issuance();
    let total_before: u128 = decode_exact(
        &pre(&total_key).ok_or("finalize 前缺少 Balances::TotalIssuance")?,
        "Balances::TotalIssuance",
    )?;
    let total_after: u128 = decode_exact(
        &post(&total_key).ok_or("finalize 后缺少 Balances::TotalIssuance")?,
        "Balances::TotalIssuance",
    )?;
    let total_delta = signed_delta(total_before, total_after);
    let expected_total = i128::try_from(plan.total).unwrap_or(i128::MAX);
    if total_delta != expected_total {
        return Err(format!(
            "finalize 总发行差额错误:期望 {},实际 {total_delta}",
            plan.total
        ));
    }

    Ok(())
}

#[path = "lifecycle.rs"]
pub mod cid_lifecycle;

#[path = "citizen.rs"]
pub mod citizen_issuance;

#[path = "fullnode.rs"]
pub mod fullnode_issuance;

#[path = "phase.rs"]
pub mod genesis_pallet;

#[path = "composition.rs"]
pub mod national_body_composition;

#[path = "interest.rs"]
pub mod provincialbank_interest;

#[cfg(test)]
#[path = "../tests/identity.rs"]
mod genesis_identity_tests;
#[path = "policy.rs"]
pub mod runtime_policy;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod finalize_issuance_tests;

#[cfg(any(test, feature = "test-support"))]
#[path = "../tests/support.rs"]
pub mod testing;

impl FinalizeIssuancePlan {
    pub(super) fn add(&mut self, amount: u128) -> Result<(), ()> {
        self.total = self.total.checked_add(amount).ok_or(())?;
        Ok(())
    }
}

impl<I> NodeGuard<I> {
    /// 装配节点守卫。
    ///
    /// 首先强制校验唯一合法的创世哈希与状态根，失败立即中止节点启动。
    /// 其他本机状态自检仍只报告警告；后续执行块及候选 runtime 校验保持
    /// fail-closed，坏输入一律不委派内层导入器。
    ///
    /// # Panics
    /// 创世身份不匹配、缺失或无法读取时拒绝构造守卫，保持现有调用接口不变。
    pub fn new(inner: I, client: Arc<FullClient>, backend: Arc<FullBackend>) -> Self {
        enforce_genesis_identity(client.as_ref());
        Self::assemble(inner, client, backend)
    }

    fn assemble(inner: I, client: Arc<FullClient>, backend: Arc<FullBackend>) -> Self {
        let genesis_hash = client.info().genesis_hash;
        let mut startup_issues = Vec::<String>::new();

        if let Err(reason) = crate::guard::national_body_composition::check_full_state(|key| {
            client
                .storage(genesis_hash, &StorageKey(key.to_vec()))
                .ok()
                .flatten()
                .map(|data| data.0)
        }) {
            startup_issues.push(format!("国家机构组成校验失败:{reason:?}"));
        }
        let vote_state_keys = match Self::state_keys_for_prefixes(
            &client,
            genesis_hash,
            vec![
                crate::guard::national_body_composition::storage_key::threshold_prefix(),
                crate::guard::national_body_composition::storage_key::proposal_prefix(),
            ],
        ) {
            Ok(keys) => keys,
            Err(reason) => {
                startup_issues.push(format!("固定治理阈值枚举失败:{reason}"));
                Vec::new()
            }
        };
        if let Err(reason) = crate::guard::national_body_composition::check_vote_state_keys(
            &vote_state_keys,
            |key| {
                client
                    .storage(genesis_hash, &StorageKey(key.to_vec()))
                    .ok()
                    .flatten()
                    .map(|data| data.0)
            },
        ) {
            startup_issues.push(format!("固定治理阈值校验失败:{reason:?}"));
        }
        if let Err(reason) = crate::guard::fullnode_issuance::check_genesis(|key| {
            client
                .storage(genesis_hash, &StorageKey(key.to_vec()))
                .ok()
                .flatten()
                .map(|data| data.0)
        }) {
            startup_issues.push(format!("全节点发行审计状态校验失败:{reason:?}"));
        }
        let mut provincialbank_state = match Self::pallet_state(
            &client,
            genesis_hash,
            crate::guard::provincialbank_interest::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("省储行固定发行状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        for key in crate::guard::provincialbank_interest::relevant_import_keys() {
            if key.starts_with(&crate::guard::provincialbank_interest::storage_key::pallet_prefix())
            {
                continue;
            }
            match client.storage(genesis_hash, &StorageKey(key.clone())) {
                Ok(Some(value)) => {
                    provincialbank_state.insert(key, value.0);
                }
                Ok(None) => {}
                Err(reason) => startup_issues.push(format!("读取省储行质押本金失败:{reason}")),
            }
        }
        if let Err(reason) = crate::guard::provincialbank_interest::check_recorded_state(
            0,
            provincialbank_state.iter(),
        ) {
            startup_issues.push(format!("省储行固定发行校验失败:{reason:?}"));
        }
        let citizen_issuance_state = match Self::pallet_state(
            &client,
            genesis_hash,
            crate::guard::citizen_issuance::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("公民认证发行状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        if let Err(reason) =
            crate::guard::citizen_issuance::check_genesis_key_values(citizen_issuance_state.iter())
        {
            startup_issues.push(format!("公民认证发行状态校验失败:{reason:?}"));
        }
        let genesis_pallet_state = match Self::pallet_state(
            &client,
            genesis_hash,
            crate::guard::genesis_pallet::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("GenesisPallet 状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        if let Err(reason) = crate::guard::genesis_pallet::check_genesis(|key| {
            genesis_pallet_state.get(key).cloned()
        }) {
            startup_issues.push(format!("GenesisPallet 事实校验失败:{reason:?}"));
        }
        let runtime_policy_state = match Self::pallet_state(
            &client,
            genesis_hash,
            sp_crypto_hashing::twox_128(b"OffchainTransaction"),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("手续费制度状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        if let Err(reason) =
            crate::guard::runtime_policy::check_imported_state(runtime_policy_state.iter())
        {
            startup_issues.push(format!("手续费制度校验失败:{reason}"));
        }
        let cid_lifecycle = match Self::cid_state_keys(&client, genesis_hash)
            .map_err(|reason| format!("CID 生命周期基准枚举失败:{reason}"))
            .and_then(|cid_keys| {
                crate::guard::cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
                    client
                        .storage(genesis_hash, &StorageKey(key.to_vec()))
                        .ok()
                        .flatten()
                        .map(|data| data.0)
                })
                .map_err(|reason| format!("CID 生命周期基准校验失败:{reason:?}"))
            }) {
            Ok(reference) => Some(reference),
            Err(reason) => {
                startup_issues.push(reason);
                None
            }
        };

        if startup_issues.is_empty() {
            log::info!(target: "node-guard", "节点守卫启动自检通过");
        } else {
            // 启动自检只报告本机现有状态风险；导入新块和候选 runtime 时仍由守卫 fail-closed。
            log::error!(
                target: "node-guard",
                "节点守卫启动自检未通过，节点继续启动并保持导入守卫: {}",
                startup_issues.join("；")
            );
        }

        Self {
            inner,
            client,
            backend,
            cid_lifecycle,
        }
    }

    /// 枚举节点原生 CID 策略承认的全部规范 RAW 表；只用于启动与 runtime 升级全检。
    fn cid_state_keys(
        client: &Arc<FullClient>,
        at: <Block as BlockT>::Hash,
    ) -> Result<Vec<Vec<u8>>, String> {
        let mut keys = BTreeMap::<Vec<u8>, ()>::new();
        for prefix in crate::guard::cid_lifecycle::storage_key::enumerated_prefixes() {
            let prefix = StorageKey(prefix);
            let iter = client
                .storage_keys(at, Some(&prefix), None)
                .map_err(|e| format!("枚举 CID 规范表失败:{e}"))?;
            for key in iter {
                keys.insert(key.0, ());
            }
        }
        Ok(keys.into_keys().collect())
    }

    /// 枚举保留的费率、提案和阈值存储子树。
    fn state_keys_for_prefixes(
        client: &Arc<FullClient>,
        at: <Block as BlockT>::Hash,
        prefixes: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, String> {
        let mut keys = BTreeMap::<Vec<u8>, ()>::new();
        for prefix in prefixes {
            let prefix = StorageKey(prefix);
            let iter = client
                .storage_keys(at, Some(&prefix), None)
                .map_err(|e| format!("枚举节点永久策略 storage 子树失败:{e}"))?;
            for key in iter {
                keys.insert(key.0, ());
            }
        }
        Ok(keys.into_keys().collect())
    }

    /// 枚举指定 pallet 的 RAW key/value，用于启动基准检查。
    fn pallet_state(
        client: &Arc<FullClient>,
        at: <Block as BlockT>::Hash,
        pallet_prefix: [u8; 16],
    ) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let prefix = StorageKey(pallet_prefix.to_vec());
        let keys = client
            .storage_keys(at, Some(&prefix), None)
            .map_err(|e| format!("枚举节点永久策略 pallet 失败:{e}"))?;
        let mut state = BTreeMap::new();
        for key in keys {
            let value = client
                .storage(at, &key)
                .map_err(|e| format!("读取节点永久策略 RAW 状态失败:{e}"))?
                .ok_or_else(|| "枚举到的节点永久策略 RAW key 缺少值".to_string())?;
            state.insert(key.0, value.0);
        }
        Ok(state)
    }

    /// 对正常执行型区块统一预执行一次，并检查当前已注册的全部节点永久策略。
    fn detect_violation(&self, params: &mut BlockImportParams<Block>) -> Result<bool, String> {
        let body = params
            .body
            .clone()
            .ok_or_else(|| "普通执行块缺少 body，无法验证状态".to_string())?;
        let extrinsic_count = body.len();
        if !has_user_transaction(extrinsic_count) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):空块不允许上链",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        let parent_hash = *params.header.parent_hash();
        let parent_state = self
            .backend
            .state_at(parent_hash, TrieCacheContext::Untrusted)
            .map_err(|e| format!("取父状态失败:{e}"))?;

        // 第一阶段只执行 initialize + extrinsics，不运行 finalize，用来隔离 runtime on_finalize 的净变化。
        let pre_api = self.client.runtime_api();
        pre_api
            .initialize_block(parent_hash, &params.header)
            .map_err(|e| format!("初始化区块预执行失败:{e}"))?;
        for extrinsic in body.iter().cloned() {
            match pre_api.apply_extrinsic(parent_hash, extrinsic) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(format!("预执行交易有效性失败:{e:?}")),
                Err(e) => return Err(format!("预执行交易调用失败:{e}")),
            }
        }
        let pre_changes = pre_api
            .into_storage_changes(&parent_state, parent_hash)
            .map_err(|e| format!("提取 finalize 前存储变更失败:{e}"))?;
        let pre_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
            pre_changes.main_storage_changes.into_iter().collect();

        // 第二阶段完整执行同一区块，得到 finalize 后状态；两阶段都只在 overlay 中执行，不提交后端。
        let block = Block::new(params.header.clone(), body.clone());
        let api = self.client.runtime_api();
        api.execute_block(parent_hash, block.into())
            .map_err(|e| format!("只读执行区块失败:{e}"))?;
        let changes = api
            .into_storage_changes(&parent_state, parent_hash)
            .map_err(|e| format!("提取存储变更失败:{e}"))?;
        let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
            changes.main_storage_changes.iter().cloned().collect();

        let read_parent = |key: &[u8]| -> Option<Vec<u8>> {
            self.client
                .storage(parent_hash, &StorageKey(key.to_vec()))
                .ok()
                .flatten()
                .map(|data| data.0)
        };
        let read_pre = |key: &[u8]| -> Option<Vec<u8>> {
            match pre_delta.get(key) {
                Some(value) => value.clone(),
                None => read_parent(key),
            }
        };
        let read_post = |key: &[u8]| -> Option<Vec<u8>> {
            match post_delta.get(key) {
                Some(value) => value.clone(),
                None => read_parent(key),
            }
        };

        if let Err(reason) = crate::guard::runtime_policy::check_transition(&post_delta, read_post)
        {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):手续费制度状态非法 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }
        // 事件由父块 Runtime 执行时编码；升级块不能用节点内置的新事件类型解码。
        let parent_metadata = self
            .client
            .runtime_api()
            .metadata(parent_hash)
            .map_err(|error| format!("读取父块 Runtime 元数据失败:{error}"))?;
        if let Err(reason) =
            crate::guard::runtime_policy::check_block(&body, read_post, parent_metadata.as_slice())
        {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):实际手续费结果非法 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        let mut issuance_plan = crate::guard::FinalizeIssuancePlan::default();
        if let Err(reason) =
            crate::guard::genesis_pallet::check_transition(&post_delta, &read_parent, &read_post)
        {
            log::error!(target: "node-guard", "创世固定事实校验失败:{reason:?}");
            return Ok(true);
        }

        if let Err(reason) = crate::guard::fullnode_issuance::check_transition(
            *params.header.number(),
            crate::guard::fullnode_issuance::author_from_header(&params.header),
            read_parent,
            read_pre,
            read_post,
            &mut issuance_plan,
        ) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):全节点发行永久规则被破坏 —— {:?}",
                params.header.number(),
                params.post_hash(),
                reason,
            );
            return Ok(true);
        }

        if let Err(reason) = crate::guard::citizen_issuance::check_transition(
            &post_delta,
            &read_parent,
            &read_pre,
            &read_post,
            &mut issuance_plan,
        ) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):公民认证发行永久规则被破坏 —— {:?}",
                params.header.number(),
                params.post_hash(),
                reason,
            );
            return Ok(true);
        }

        if let Err(reason) = crate::guard::provincialbank_interest::check_transition(
            *params.header.number(),
            &pre_delta,
            &post_delta,
            &read_parent,
            &read_pre,
            &read_post,
            &mut issuance_plan,
        ) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):省储行固定发行永久规则被破坏 —— {:?}",
                params.header.number(),
                params.post_hash(),
                reason,
            );
            return Ok(true);
        }

        if let Err(reason) = verify_finalize_issuance(&read_pre, &read_post, &issuance_plan) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):finalize 统一发行核算失败 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        if let Some(cid_lifecycle) = self.cid_lifecycle.as_ref() {
            if let Err(reason) = crate::guard::cid_lifecycle::check_transition(
                *params.header.number(),
                &post_delta,
                read_parent,
                read_post,
                cid_lifecycle,
            ) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):CID 生命周期永久规则被破坏 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
        } else if post_delta
            .keys()
            .any(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
            || crate::guard::cid_lifecycle::needs_full_check(&post_delta)
        {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):CID 生命周期启动基准不可用，不能验证受保护状态变更",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        if crate::guard::cid_lifecycle::needs_full_check(&post_delta) {
            let Some(cid_lifecycle) = self.cid_lifecycle.as_ref() else {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):CID 生命周期启动基准不可用，不能复核候选 runtime",
                    params.header.number(),
                    params.post_hash(),
                );
                return Ok(true);
            };
            let mut keys: BTreeMap<Vec<u8>, ()> = Self::cid_state_keys(&self.client, parent_hash)?
                .into_iter()
                .map(|key| (key, ()))
                .collect();
            for key in post_delta
                .keys()
                .filter(|key| crate::guard::cid_lifecycle::is_relevant_key(key))
            {
                keys.insert(key.clone(), ());
            }
            let keys: Vec<Vec<u8>> = keys.into_keys().collect();
            if let Err(reason) =
                crate::guard::cid_lifecycle::check_full_state(&keys, read_post, cid_lifecycle)
            {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):runtime 升级后的 CID 规范表全检失败 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
        }

        if post_delta.contains_key(sp_storage::well_known_keys::CODE) {
            // 候选 runtime 生效前全量复核当前/待生效清算行费率；不能只看升级块
            // 自己触及的 key，否则新 WASM 可以把存量非法值留到后续块再激活。
            let mut fee_keys = Self::state_keys_for_prefixes(
                &self.client,
                parent_hash,
                crate::guard::runtime_policy::storage_key::relevant_prefixes().to_vec(),
            )?
            .into_iter()
            .map(|key| (key, ()))
            .collect::<BTreeMap<_, _>>();
            fee_keys.insert(crate::guard::runtime_policy::storage_key::max_rate(), ());
            for key in post_delta
                .keys()
                .filter(|key| crate::guard::runtime_policy::storage_key::is_relevant(key))
            {
                fee_keys.insert(key.clone(), ());
            }
            let fee_state = fee_keys
                .into_keys()
                .filter_map(|key| read_post(&key).map(|value| (key, value)))
                .collect::<BTreeMap<_, _>>();
            if let Err(reason) =
                crate::guard::runtime_policy::check_imported_state(fee_state.iter())
            {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):候选 runtime 清算费率全检失败 —— {reason}",
                    params.header.number(),
                    params.post_hash(),
                );
                return Ok(true);
            }
            let candidate_code =
                read_post(sp_storage::well_known_keys::CODE).ok_or("runtime 升级后缺少 :code")?;
            if let Err(reason) = crate::guard::runtime_policy::check_candidate_runtime(
                &parent_state,
                parent_hash,
                &post_delta,
                &candidate_code,
            ) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):候选 runtime 接口策略非法 —— {reason}",
                    params.header.number(),
                    params.post_hash(),
                );
                return Ok(true);
            }
            if let Err(reason) = crate::guard::genesis_pallet::check_full_state(read_post) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):runtime 升级后的创世模块全检失败 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
            if let Err(reason) = crate::guard::provincialbank_interest::check_full_state(
                *params.header.number(),
                &read_post,
            ) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):runtime 升级后的省储行固定发行全检失败 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
        }

        // runtime 升级必须复核升级前已存在、升级后仍存在以及本块新增/删除的全部内部
        // 提案和阈值快照，防止新 WASM 通过移走旧 storage 绕过五类固定治理阈值语义。
        let runtime_upgrade_vote_keys =
            if post_delta.contains_key(sp_storage::well_known_keys::CODE) {
                let mut keys = Self::state_keys_for_prefixes(
                    &self.client,
                    parent_hash,
                    vec![
                        crate::guard::national_body_composition::storage_key::threshold_prefix(),
                        crate::guard::national_body_composition::storage_key::proposal_prefix(),
                    ],
                )?
                .into_iter()
                .map(|key| (key, ()))
                .collect::<BTreeMap<_, _>>();
                for key in post_delta.keys().filter(|key| {
                    key.starts_with(
                        &crate::guard::national_body_composition::storage_key::threshold_prefix(),
                    ) || key.starts_with(
                        &crate::guard::national_body_composition::storage_key::proposal_prefix(),
                    )
                }) {
                    keys.insert(key.clone(), ());
                }
                Some(keys.into_keys().collect::<Vec<_>>())
            } else {
                None
            };
        if let Err(reason) = crate::guard::national_body_composition::check_transition(
            &post_delta,
            read_parent,
            read_post,
            runtime_upgrade_vote_keys.as_deref(),
        ) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):国家机构组成或固定治理阈值被破坏 —— {:?}",
                params.header.number(),
                params.post_hash(),
                reason,
            );
            return Ok(true);
        }
        // 普通执行块只导入本节点已验证的执行结果，不消费外来预计算变更。
        if !matches!(&params.state_action, StateAction::Skip) {
            params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(changes));
        }
        Ok(false)
    }
}

#[async_trait::async_trait]
impl<I> BlockImport<Block> for NodeGuard<I>
where
    I: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
    type Error = ConsensusError;

    async fn check_block(
        &self,
        block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        self.inner.check_block(block).await
    }

    async fn import_block(
        &self,
        mut params: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        if params.with_state() {
            return self.inner.import_block(params).await;
        }

        let verdict = match self.detect_violation(&mut params) {
            Ok(true) => Err("节点永久规则明确判定为违规".to_string()),
            Ok(false) => Ok(()),
            // 沿用现有 fail-closed 口径：无法完成节点规则检查时不导入未经验证的区块。
            Err(reason) => {
                log::error!(
                    target: "node-guard",
                    "节点守卫判定失败,fail-closed 拒块 ({:?}):{reason}",
                    params.post_hash(),
                );
                Err(reason)
            }
        };
        import_if_verified(&self.inner, params, verdict).await
    }
}
