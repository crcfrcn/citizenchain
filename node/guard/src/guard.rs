//! 组装创世制度状态，在导入和升级边界执行节点守卫。

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

use nodeclient::codec::RuntimeStorage;

/// 除宪法外的节点永久规则统一导入包装器。
pub struct NodeGuard<I> {
    inner: I,
    client: Arc<FullClient>,
    backend: Arc<FullBackend>,
    cid_lifecycle: Option<crate::guard::cid_lifecycle::GenesisReference>,
    genesis: Result<Arc<GenesisState>, String>,
    expected_hash: sp_core::H256,
    expected_root: sp_core::H256,
}

/// 两类守卫共用的只读创世输入；字段私有，验证后不能改写存储内容。
pub struct GenesisState {
    storage: sp_storage::Storage,
    hash: sp_core::H256,
    state_root: sp_core::H256,
}

impl GenesisState {
    /// 按冻结输入自身的状态版本计算全部主/子trie；只分配内存，不打开链库。
    fn from_storage(storage: sp_storage::Storage, version: sp_storage::StateVersion) -> Self {
        use sp_runtime::traits::{BlakeTwo256, Hash as _};
        let (state_root, _) = Self::memory_state(&storage, version);
        let header = runtime::opaque::Header::new(
            0,
            BlakeTwo256::trie_root(Vec::new(), version),
            state_root,
            Default::default(),
            Default::default(),
        );
        Self {
            storage,
            hash: header.hash(),
            state_root,
        }
    }

    fn memory_state(
        storage: &sp_storage::Storage,
        version: sp_storage::StateVersion,
    ) -> (
        sp_core::H256,
        impl sp_state_machine::Backend<sp_runtime::traits::BlakeTwo256> + 'static,
    ) {
        use sp_runtime::traits::BlakeTwo256;
        let mut changes = vec![(
            None,
            storage
                .top
                .iter()
                .map(|(key, value)| (key.clone(), Some(value.clone())))
                .collect::<Vec<_>>(),
        )];
        changes.extend(storage.children_default.values().map(|child| {
            (
                Some(child.child_info.clone()),
                child
                    .data
                    .iter()
                    .map(|(key, value)| (key.clone(), Some(value.clone())))
                    .collect::<Vec<_>>(),
            )
        }));
        let mut state = sp_state_machine::new_in_mem::<BlakeTwo256>();
        state.insert(changes, version);
        (*state.root(), state)
    }

    /// 正式基准必须同时匹配冻结状态根和完整块0哈希。
    pub fn new(
        storage: sp_storage::Storage,
        version: sp_storage::StateVersion,
    ) -> Result<Self, String> {
        let state = Self::from_storage(storage, version);
        state.verify(
            primitives::genesis::GENESIS_HASH.into(),
            primitives::genesis::GENESIS_STATE_ROOT.into(),
        )?;
        Ok(state)
    }

    fn verify(&self, hash: sp_core::H256, root: sp_core::H256) -> Result<(), String> {
        if self.hash != hash || self.state_root != root {
            return Err("守卫创世输入与声明的状态根或区块哈希不匹配".into());
        }
        Ok(())
    }

    pub(crate) fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.storage.top.get(key).cloned()
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.hash == sp_core::H256::from(primitives::genesis::GENESIS_HASH)
            && self.state_root == sp_core::H256::from(primitives::genesis::GENESIS_STATE_ROOT)
    }

    // 仅解释已验证冻结块0的原始存储命名；这些值的SCALE布局没有改变。
    // 不用于运行态查询，不先读新键再回退，也不生成或改写任何链状态。
    fn baseline_key(key: &[u8], frozen: bool) -> Vec<u8> {
        for (current, original) in [
            ("Legislation", "LegislationYuan"),
            ("FullnodeIssue", "FullnodeIssuance"),
            ("PrbBankIssue", "ProvincialBankInterest"),
            ("CitizenIssue", "CitizenIssuance"),
            ("Genesis", "GenesisPallet"),
            ("Offchain", "OffchainTransaction"),
        ] {
            let (from, to) = if frozen {
                (current, original)
            } else {
                (original, current)
            };
            if let Some(suffix) =
                key.strip_prefix(sp_crypto_hashing::twox_128(from.as_bytes()).as_slice())
            {
                return [
                    sp_crypto_hashing::twox_128(to.as_bytes()).to_vec(),
                    suffix.to_vec(),
                ]
                .concat();
            }
        }
        key.to_vec()
    }

    pub(crate) fn read_baseline(&self, key: &[u8]) -> Option<Vec<u8>> {
        if self.is_frozen() {
            self.read(&Self::baseline_key(key, true))
        } else {
            self.read(key)
        }
    }

    pub(crate) fn baseline_entries(&self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        self.storage
            .top
            .iter()
            .map(|(key, value)| {
                (
                    if self.is_frozen() {
                        Self::baseline_key(key, false)
                    } else {
                        key.clone()
                    },
                    value.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn keys(&self) -> Vec<Vec<u8>> {
        self.storage.top.keys().cloned().collect()
    }
}

/// 只读取数据库中的真实块 0 头，不使用创世 storage 中的 BlockHash(0) 占位值。
#[cfg(test)]
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
    /// 启动自检只报告诊断；创世身份与规则校验在入链前继续严格执行。
    pub fn new(
        inner: I,
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        genesis: Result<Arc<GenesisState>, String>,
    ) -> Self {
        Self::assemble(
            inner,
            client,
            backend,
            genesis,
            primitives::genesis::GENESIS_HASH.into(),
            primitives::genesis::GENESIS_STATE_ROOT.into(),
        )
    }

    fn assemble(
        inner: I,
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        genesis: Result<Arc<GenesisState>, String>,
        expected_hash: sp_core::H256,
        expected_root: sp_core::H256,
    ) -> Self {
        let genesis = genesis.and_then(|state| {
            state.verify(expected_hash, expected_root)?;
            Ok(state)
        });
        let mut startup_issues = Vec::new();
        if let Err(reason) =
            verify_genesis_identity_against(client.as_ref(), expected_hash, expected_root)
        {
            startup_issues.push(reason);
        }
        let cid_lifecycle = match &genesis {
            Err(reason) => {
                startup_issues.push(reason.clone());
                None
            }
            Ok(state) => {
                let read = |key: &[u8]| state.read_baseline(key);
                let entries = state.baseline_entries();
                let keys = entries.keys().cloned().collect::<Vec<_>>();
                // 冻结块0只存在准确的旧实体布局；按已验证创世输入核对真实成员，
                // 不能因新命名空间没有记录而误判为尚未组成。
                let composition_error = std::cell::RefCell::new(None);
                let composition_read = |key: &[u8]| {
                    if !state.is_frozen() {
                        return state.read(key);
                    }
                    let (source, target) = runtime::guard_entity_names(true);
                    let target_prefix = sp_crypto_hashing::twox_128(target);
                    let Some(suffix) = key.strip_prefix(target_prefix.as_slice()) else {
                        return read(key);
                    };
                    let physical =
                        [sp_crypto_hashing::twox_128(source).as_slice(), suffix].concat();
                    let raw = state.read(&physical)?;
                    if suffix.get(..16)
                        == Some(sp_crypto_hashing::twox_128(b"Institutions").as_slice())
                    {
                        match runtime::guard_institution(&raw, true) {
                            Ok(info) => Some(codec::Encode::encode(&info)),
                            Err(error) => {
                                *composition_error.borrow_mut() =
                                    Some(format!("冻结机构解码失败:{error:?}"));
                                None
                            }
                        }
                    } else {
                        Some(raw)
                    }
                };
                if let Err(reason) =
                    crate::guard::national_body_composition::check_full_state_with_layout(
                        composition_read,
                        |_| state.is_frozen(),
                    )
                {
                    startup_issues.push(format!("国家机构组成校验失败:{reason:?}"));
                }
                if let Some(reason) = composition_error.into_inner() {
                    startup_issues.push(reason);
                }
                if let Err(reason) =
                    crate::guard::national_body_composition::check_vote_state_keys(&keys, &read)
                {
                    startup_issues.push(format!("固定治理阈值校验失败:{reason:?}"));
                }
                if let Err(reason) = crate::guard::fullnode_issuance::check_genesis(&read) {
                    startup_issues.push(format!("全节点发行校验失败:{reason:?}"));
                }
                let bank_keys = crate::guard::provincialbank_interest::relevant_import_keys();
                let bank_prefix =
                    crate::guard::provincialbank_interest::storage_key::pallet_prefix();
                let bank = entries
                    .iter()
                    .filter(|(key, _)| key.starts_with(&bank_prefix) || bank_keys.contains(*key));
                if let Err(reason) =
                    crate::guard::provincialbank_interest::check_recorded_state(0, bank)
                {
                    startup_issues.push(format!("省储行固定发行校验失败:{reason:?}"));
                }
                let citizen_prefix = crate::guard::citizen_issuance::storage_key::pallet_prefix();
                if let Err(reason) = crate::guard::citizen_issuance::check_genesis_key_values(
                    entries
                        .iter()
                        .filter(|(key, _)| key.starts_with(&citizen_prefix)),
                ) {
                    startup_issues.push(format!("公民认证发行校验失败:{reason:?}"));
                }
                if let Err(reason) = crate::guard::genesis_pallet::check_genesis(&read) {
                    startup_issues.push(format!("创世固定事实校验失败:{reason:?}"));
                }
                let fee_prefix = sp_crypto_hashing::twox_128(b"Offchain");
                if let Err(reason) = crate::guard::runtime_policy::check_imported_state(
                    entries
                        .iter()
                        .filter(|(key, _)| key.starts_with(&fee_prefix)),
                ) {
                    startup_issues.push(format!("手续费制度校验失败:{reason}"));
                }
                let reference = if state.is_frozen() {
                    crate::guard::cid_lifecycle::GenesisReference::from_frozen(state)
                } else {
                    crate::guard::cid_lifecycle::GenesisReference::from_genesis(&keys, &read)
                };
                match reference {
                    Ok(reference) => Some(reference),
                    Err(reason) => {
                        startup_issues.push(format!("CID创世基准校验失败:{reason:?}"));
                        None
                    }
                }
            }
        };
        if startup_issues.is_empty() {
            log::info!(target: "node-guard", "节点守卫启动自检通过");
        } else {
            log::error!(target: "node-guard", "节点守卫启动自检诊断，服务继续运行，入链校验保持严格: {}", startup_issues.join("；"));
        }
        Self {
            inner,
            client,
            backend,
            cid_lifecycle,
            genesis,
            expected_hash,
            expected_root,
        }
    }

    fn check_identity(&self) -> Result<(), String> {
        self.genesis.as_ref().map_err(Clone::clone)?;
        verify_genesis_identity_against(
            self.client.as_ref(),
            self.expected_hash,
            self.expected_root,
        )
    }

    fn verify_imported_state(&self, params: &BlockImportParams<Block>) -> Result<(), String> {
        use sc_executor::RuntimeVersionOf;
        let imported = match &params.state_action {
            StateAction::ApplyChanges(StorageChanges::Import(imported)) => imported,
            _ => return Err("完整状态形态无效".into()),
        };
        if imported.block != params.post_hash() {
            return Err("完整状态的目标区块不匹配".into());
        }
        let mut storage = sp_storage::Storage::default();
        for level in &imported.state.0 {
            if level.parent_storage_keys.is_empty() && level.state_root.is_empty() {
                for (key, value) in &level.key_values {
                    if storage.top.insert(key.clone(), value.clone()).is_some() {
                        return Err("完整状态包含重复主存储键".into());
                    }
                }
            } else {
                if level.parent_storage_keys.is_empty() {
                    return Err("子状态缺少所属存储键".into());
                }
                for parent in &level.parent_storage_keys {
                    let (_, id) = sp_storage::ChildType::from_prefixed_key(
                        sp_storage::PrefixedStorageKey::new_ref(parent),
                    )
                    .ok_or("子状态所属存储键无效")?;
                    let child = storage
                        .children_default
                        .entry(id.to_vec())
                        .or_insert_with(|| sp_storage::StorageChild {
                            data: Default::default(),
                            child_info: sp_storage::ChildInfo::new_default(id),
                        });
                    for (key, value) in &level.key_values {
                        if child.data.insert(key.clone(), value.clone()).is_some() {
                            return Err("完整状态包含重复子存储键".into());
                        }
                    }
                }
            }
        }
        let code = storage
            .top
            .get(sp_storage::well_known_keys::CODE)
            .ok_or("完整状态缺少runtime代码")?;
        let fetcher = sp_core::traits::WrappedRuntimeCode(code.as_slice().into());
        let runtime_code = sp_core::traits::RuntimeCode {
            code_fetcher: &fetcher,
            heap_pages: None,
            hash: sp_crypto_hashing::blake2_256(code).to_vec(),
        };
        let executor = sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::default();
        let version = executor
            .runtime_version(
                &mut sp_state_machine::BasicExternalities::new_empty(),
                &runtime_code,
            )
            .map_err(|error| format!("完整状态runtime版本不可读:{error}"))?
            .state_version();
        let (root, memory) = GenesisState::memory_state(&storage, version);
        if root != *params.header.state_root() {
            return Err("完整状态根与区块头不匹配".into());
        }
        // 完整状态也必须使用该状态真实代码的类型声明，不用节点编译期布局猜测。
        // 这里只调用只读元数据接口，临时externalities中的任何写入均不进入核验状态。
        use sp_core::traits::CodeExecutor as _;
        let (metadata, _) = executor.call(
            &mut sp_state_machine::BasicExternalities::new_empty(),
            &runtime_code,
            "Metadata_metadata",
            &[],
            sp_core::traits::CallContext::Onchain { import: false },
        );
        let metadata = metadata.map_err(|e| format!("完整状态元数据调用失败:{e}"))?;
        let metadata: sp_core::OpaqueMetadata =
            codec::DecodeAll::decode_all(&mut metadata.as_slice())
                .map_err(|e| format!("完整状态元数据编码非法:{e}"))?;
        let physical = |key: &[u8]| Ok(storage.top.get(key).cloned());
        let layout = RuntimeStorage::from_metadata(&metadata)?.with_state(physical)?;
        let read_error = std::cell::RefCell::new(None);
        let read = |key: &[u8]| match layout.read(key, physical) {
            Ok(value) => value,
            Err(reason) => {
                *read_error.borrow_mut() = Some(reason);
                None
            }
        };
        let legacy = |key: &[u8]| match layout.read_raw(key, physical) {
            Ok(value) => value.is_some_and(|record| record.legacy),
            Err(reason) => {
                *read_error.borrow_mut() = Some(reason);
                false
            }
        };
        let keys = storage
            .top
            .keys()
            .map(|key| layout.logical_key(key))
            .collect::<Result<Vec<_>, _>>()?;
        crate::guard::national_body_composition::check_full_state_with_layout(&read, legacy)
            .map_err(|e| format!("完整状态机构组成非法:{e:?}"))?;
        crate::guard::national_body_composition::check_vote_state_keys(&keys, &read)
            .map_err(|e| format!("完整状态治理阈值非法:{e:?}"))?;
        crate::guard::genesis_pallet::check_full_state(&read)
            .map_err(|e| format!("完整状态创世事实非法:{e:?}"))?;
        crate::guard::provincialbank_interest::check_full_state(*params.header.number(), &read)
            .map_err(|e| format!("完整状态省储行规则非法:{e:?}"))?;
        let values = keys
            .iter()
            .filter_map(|key| read(key).map(|value| (key.clone(), value)))
            .collect::<BTreeMap<_, _>>();
        crate::guard::runtime_policy::check_imported_state(values.iter())?;
        crate::guard::cid_lifecycle::check_full_state(
            &keys,
            &read,
            self.cid_lifecycle.as_ref().ok_or("CID创世基准不可验证")?,
        )
        .map_err(|e| format!("完整状态CID规则非法:{e:?}"))?;
        if let Some(reason) = read_error.into_inner() {
            return Err(reason);
        }
        // 与普通升级块复用同一候选runtime检查；内存状态精确来自下载态，不混入父状态。
        crate::guard::runtime_policy::check_candidate_runtime(
            &memory,
            *params.header.parent_hash(),
            &BTreeMap::new(),
            code,
            None,
        )?;
        Ok(())
    }

    /// 枚举运行态CID规范表供runtime升级全检；创世基准仅由已验证的内存输入提供。
    fn cid_state_keys(
        client: &Arc<FullClient>,
        at: <Block as BlockT>::Hash,
        storage: &RuntimeStorage,
    ) -> Result<Vec<Vec<u8>>, String> {
        let mut keys = BTreeMap::<Vec<u8>, ()>::new();
        for prefix in crate::guard::cid_lifecycle::storage_key::enumerated_prefixes() {
            for prefix in storage.physical_prefixes(&prefix) {
                let prefix = StorageKey(prefix);
                let iter = client
                    .storage_keys(at, Some(&prefix), None)
                    .map_err(|e| format!("枚举 CID 规范表失败:{e}"))?;
                for key in iter {
                    keys.insert(storage.logical_key(&key.0)?, ());
                }
            }
        }
        Ok(keys.into_keys().collect())
    }

    /// 枚举保留的费率、提案和阈值存储子树。
    fn state_keys_for_prefixes(
        client: &Arc<FullClient>,
        at: <Block as BlockT>::Hash,
        prefixes: Vec<Vec<u8>>,
        storage: &RuntimeStorage,
    ) -> Result<Vec<Vec<u8>>, String> {
        let mut keys = BTreeMap::<Vec<u8>, ()>::new();
        for prefix in prefixes {
            for prefix in storage.physical_prefixes(&prefix) {
                let prefix = StorageKey(prefix);
                let iter = client
                    .storage_keys(at, Some(&prefix), None)
                    .map_err(|e| format!("枚举节点永久策略 storage 子树失败:{e}"))?;
                for key in iter {
                    keys.insert(storage.logical_key(&key.0)?, ());
                }
            }
        }
        Ok(keys.into_keys().collect())
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
        let raw_pre_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
            pre_changes.main_storage_changes.into_iter().collect();

        // 第二阶段完整执行同一区块，得到 finalize 后状态；两阶段都只在 overlay 中执行，不提交后端。
        let block = Block::new(params.header.clone(), body.clone());
        let api = self.client.runtime_api();
        api.execute_block(parent_hash, block.into())
            .map_err(|e| format!("只读执行区块失败:{e}"))?;
        let changes = api
            .into_storage_changes(&parent_state, parent_hash)
            .map_err(|e| format!("提取存储变更失败:{e}"))?;
        let raw_post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
            changes.main_storage_changes.iter().cloned().collect();

        // 元数据来自执行该块的父 Runtime。只建立规则读取视图；提交和候选 Wasm
        // 排演继续使用原始物理变更，不能把前缀定位结果写回真实状态或执行器。
        let parent_metadata = self
            .client
            .runtime_api()
            .metadata(parent_hash)
            .map_err(|error| format!("读取父块 Runtime 元数据失败:{error}"))?;
        let raw_parent = |key: &[u8]| -> Result<Option<Vec<u8>>, String> {
            self.client
                .storage(parent_hash, &StorageKey(key.to_vec()))
                .map(|value| value.map(|data| data.0))
                .map_err(|error| format!("读取守卫父状态失败:{error}"))
        };
        let raw_pre = |key: &[u8]| -> Result<Option<Vec<u8>>, String> {
            match raw_pre_delta.get(key) {
                Some(value) => Ok(value.clone()),
                None => raw_parent(key),
            }
        };
        let raw_post = |key: &[u8]| -> Result<Option<Vec<u8>>, String> {
            match raw_post_delta.get(key) {
                Some(value) => Ok(value.clone()),
                None => raw_parent(key),
            }
        };
        // 父状态按写入它的Runtime解释；本块两个执行阶段按执行Runtime及各自进度解释。
        // set_code后的首块不能把尚未搬迁的父状态误当成已经完成的新存储。
        let storage =
            RuntimeStorage::at_state(&self.client, parent_hash)?.with_state(&raw_parent)?;
        let pre_storage =
            RuntimeStorage::from_metadata(parent_metadata.as_slice())?.with_state(&raw_pre)?;
        let post_storage =
            RuntimeStorage::from_metadata(parent_metadata.as_slice())?.with_state(&raw_post)?;
        let phase_changed = storage.migration() != post_storage.migration();
        let stage_error = std::cell::RefCell::new(None);
        let stage_read = |value: Result<Option<Vec<u8>>, String>| match value {
            Ok(value) => value,
            Err(reason) => {
                *stage_error.borrow_mut() = Some(reason);
                None
            }
        };
        runtime::guard_migration_transition(
            |key| stage_read(raw_parent(key)),
            |key| stage_read(raw_post(key)),
        )
        .map_err(|e| format!("迁移阶段转换非法:{e:?}"))?;
        if let Some(reason) = stage_error.into_inner() {
            return Err(reason);
        }
        if phase_changed && post_storage.migration() == runtime::MigrationState::Complete {
            // 进度或版本键不能替代真实清理证据；已知来源必须在完成态全部为空。
            let keys = self
                .client
                .storage_keys(parent_hash, None, None)
                .map_err(|e| format!("枚举迁移完成来源失败:{e}"))?;
            for key in keys {
                if runtime::guard_retired_key(&key.0) && raw_post(&key.0)?.is_some() {
                    return Err("迁移完成后仍存在旧机构或管理员记录".into());
                }
            }
        }
        let pre_delta = storage.transition_changes(&pre_storage, &raw_pre_delta, &raw_parent)?;
        let post_delta = storage.transition_changes(&post_storage, &raw_post_delta, &raw_parent)?;
        // 既有规则接口用Option表达不存在，所有读取/解码错误另行保留，最终必须拒绝导入。
        let read_error = std::cell::RefCell::new(None);
        let checked = |result: Result<Option<Vec<u8>>, String>| match result {
            Ok(value) => value,
            Err(reason) => {
                *read_error.borrow_mut() = Some(reason);
                None
            }
        };
        let read_parent = |key: &[u8]| checked(storage.read(key, &raw_parent));
        let read_pre = |key: &[u8]| checked(pre_storage.read(key, &raw_pre));
        let read_post = |key: &[u8]| checked(post_storage.read(key, &raw_post));
        let parent_legacy = |key: &[u8]| match storage.read_raw(key, &raw_parent) {
            Ok(value) => value.is_some_and(|record| record.legacy),
            Err(reason) => {
                *read_error.borrow_mut() = Some(reason);
                false
            }
        };
        let post_legacy = |key: &[u8]| match post_storage.read_raw(key, &raw_post) {
            Ok(value) => value.is_some_and(|record| record.legacy),
            Err(reason) => {
                *read_error.borrow_mut() = Some(reason);
                false
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

        if crate::guard::cid_lifecycle::needs_full_check(&post_delta) || phase_changed {
            let Some(cid_lifecycle) = self.cid_lifecycle.as_ref() else {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):CID 生命周期启动基准不可用，不能复核候选 runtime",
                    params.header.number(),
                    params.post_hash(),
                );
                return Ok(true);
            };
            let mut keys: BTreeMap<Vec<u8>, ()> =
                Self::cid_state_keys(&self.client, parent_hash, &storage)?
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
                &storage,
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
                &raw_post_delta,
                &candidate_code,
                Some(parent_metadata.as_slice()),
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
                    &storage,
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
        if phase_changed {
            crate::guard::national_body_composition::check_full_state_with_layout(
                &read_post,
                &post_legacy,
            )
            .map_err(|e| format!("迁移阶段机构组成全检失败:{e:?}"))?;
        }
        if let Err(reason) = crate::guard::national_body_composition::check_transition_with_layout(
            &post_delta,
            read_parent,
            read_post,
            parent_legacy,
            post_legacy,
            storage.migration() == runtime::MigrationState::Running
                || post_storage.migration() == runtime::MigrationState::Running,
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
        if let Some(reason) = read_error.into_inner() {
            return Err(reason);
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
        if let Err(reason) = self.check_identity() {
            log::error!(target: "node-guard", "拒绝区块预检:{reason}");
            return Ok(ImportResult::KnownBad);
        }
        self.inner.check_block(block).await
    }

    async fn import_block(
        &self,
        mut params: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        if let Err(reason) = self.check_identity() {
            log::error!(target: "node-guard", "拒绝区块导入:{reason}");
            return Ok(ImportResult::KnownBad);
        }
        if params.with_state() {
            let verdict = self.verify_imported_state(&params);
            if let Err(reason) = &verdict {
                log::error!(target: "node-guard", "拒绝完整状态导入:{reason}");
            }
            return import_if_verified(&self.inner, params, verdict).await;
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
