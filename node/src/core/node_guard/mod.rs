//! 节点守卫统一 `BlockImport` 包装器。
//!
//! 公民宪法是整条链最高规则，继续由独立的 `ConstitutionGuard` 在本包装器外层先行检查。
//! 本模块只收口**除宪法外**的节点永久规则：统一预执行正常区块、统一提取后置 storage delta，
//! 再把同一份检查上下文交给内部策略。当前已注册固定治理骨架、三类固定发行、GenesisPallet
//! 五字段与 CID 生命周期；后续非宪法永久规则仍必须加在本包装器内部，不得新增平行包装器。

mod cid_lifecycle;
mod citizen_issuance;
mod fullnode_issuance;
mod genesis_pallet;
mod governance_skeleton;
mod national_body_composition;
mod provincialbank_interest;
mod runtime_policy;

use std::collections::BTreeMap;
use std::sync::Arc;

use codec::Decode;
use sc_client_api::backend::{Backend as _, TrieCacheContext};
use sc_client_api::StorageProvider;
use sc_consensus::{
    BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction, StorageChanges,
};
use sp_api::{ApiExt, Core, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_consensus::Error as ConsensusError;
use sp_crypto_hashing::blake2_128;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_storage::StorageKey;

use citizenchain::opaque::Block;

use crate::core::service::{FullBackend, FullClient};

/// 除宪法外的节点永久规则统一导入包装器。
pub struct NodeGuard<I> {
    inner: I,
    client: Arc<FullClient>,
    backend: Arc<FullBackend>,
    cid_lifecycle: Option<cid_lifecycle::GenesisReference>,
}

/// 所有合法 finalize 原生发行按账户汇总后，由 `NodeGuard` 统一核对余额和总发行量。
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct FinalizeIssuancePlan {
    accounts: BTreeMap<[u8; 32], u128>,
    total: u128,
}

impl FinalizeIssuancePlan {
    pub(super) fn add(&mut self, account: [u8; 32], amount: u128) -> Result<(), ()> {
        let next = self
            .accounts
            .get(&account)
            .copied()
            .unwrap_or_default()
            .checked_add(amount)
            .ok_or(())?;
        self.total = self.total.checked_add(amount).ok_or(())?;
        self.accounts.insert(account, next);
        Ok(())
    }
}

/// 两层守卫共用的最终导入闸门：只有全部原生规则明确验证成功才允许调用内层导入器。
///
/// 把委派动作单独收口后，测试可以直接证明 `Err` 路径返回 `KnownBad` 且内层调用次数为零，
/// 避免某个包装器以后在日志分支中误调用内层导入器。
pub(super) async fn import_if_verified<I>(
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

fn parse_system_account_key(key: &[u8], prefix: &[u8]) -> Result<[u8; 32], String> {
    if !key.starts_with(prefix) || key.len() != prefix.len() + 48 {
        return Err("System::Account RAW key 形态错误".into());
    }
    let hash_at = prefix.len();
    let account_at = hash_at + 16;
    let account: [u8; 32] = key[account_at..]
        .try_into()
        .map_err(|_| "System::Account 账户长度错误")?;
    if blake2_128(&account) != key[hash_at..account_at] {
        return Err("System::Account Blake2_128Concat 校验失败".into());
    }
    Ok(account)
}

fn account_info<F>(read: &F, account: &[u8; 32]) -> Result<Option<MAccountInfo>, String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    read(&fullnode_issuance::storage_key::system_account(account))
        .map(|raw| decode_exact(&raw, "System::Account"))
        .transpose()
}

#[derive(Default)]
struct ImportedPolicyState {
    governance: BTreeMap<Vec<u8>, Vec<u8>>,
    national_body_composition: BTreeMap<Vec<u8>, Vec<u8>>,
    fullnode_issuance: BTreeMap<Vec<u8>, Vec<u8>>,
    citizen_issuance: BTreeMap<Vec<u8>, Vec<u8>>,
    genesis_pallet: BTreeMap<Vec<u8>, Vec<u8>>,
    provincialbank_interest: BTreeMap<Vec<u8>, Vec<u8>>,
    runtime_policy: BTreeMap<Vec<u8>, Vec<u8>>,
    cid: BTreeMap<Vec<u8>, Vec<u8>>,
    scanned: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImportedPolicyStats {
    scanned: usize,
    governance: usize,
    national_body_composition: usize,
    fullnode_issuance: usize,
    citizen_issuance: usize,
    genesis_pallet: usize,
    provincialbank_interest: usize,
    runtime_policy: usize,
    cid: usize,
}

impl ImportedPolicyState {
    fn stats(&self) -> ImportedPolicyStats {
        ImportedPolicyStats {
            scanned: self.scanned,
            governance: self.governance.len(),
            national_body_composition: self.national_body_composition.len(),
            fullnode_issuance: self.fullnode_issuance.len(),
            citizen_issuance: self.citizen_issuance.len(),
            genesis_pallet: self.genesis_pallet.len(),
            provincialbank_interest: self.provincialbank_interest.len(),
            runtime_policy: self.runtime_policy.len(),
            cid: self.cid.len(),
        }
    }
}

/// 对完整下载态只遍历一次，并把所有内部策略需要的 RAW 状态分流到共享快照。
fn partition_imported_state<'a, I>(pairs: I) -> ImportedPolicyState
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let fullnode_prefix = fullnode_issuance::storage_key::pallet_prefix();
    let system_prefix = fullnode_issuance::storage_key::system_prefix();
    let total_issuance_key = fullnode_issuance::storage_key::total_issuance();
    let citizen_issuance_prefix = citizen_issuance::storage_key::pallet_prefix();
    let genesis_pallet_prefix = genesis_pallet::storage_key::pallet_prefix();
    let provincialbank_prefix = provincialbank_interest::storage_key::pallet_prefix();
    let provincialbank_keys = provincialbank_interest::relevant_import_keys();
    let cid_prefixes = cid_lifecycle::storage_key::relevant_prefixes();
    let mut state = ImportedPolicyState::default();
    for (key, value) in pairs {
        state.scanned = state.scanned.saturating_add(1);
        if governance_skeleton::storage_key::is_relevant(key) {
            state.governance.insert(key.clone(), value.clone());
        }
        if national_body_composition::storage_key::is_relevant(key) {
            state
                .national_body_composition
                .insert(key.clone(), value.clone());
        }
        if key.starts_with(&fullnode_prefix)
            || key.starts_with(&system_prefix)
            || key == &total_issuance_key
        {
            state.fullnode_issuance.insert(key.clone(), value.clone());
        }
        if cid_lifecycle::matches_relevant_prefixes(key, &cid_prefixes) {
            state.cid.insert(key.clone(), value.clone());
        }
        if key.starts_with(&citizen_issuance_prefix) {
            state.citizen_issuance.insert(key.clone(), value.clone());
        }
        if key.starts_with(&genesis_pallet_prefix) {
            state.genesis_pallet.insert(key.clone(), value.clone());
        }
        if key.starts_with(&provincialbank_prefix) || provincialbank_keys.contains(key) {
            state
                .provincialbank_interest
                .insert(key.clone(), value.clone());
        }
        if runtime_policy::storage_key::is_relevant(key) {
            state.runtime_policy.insert(key.clone(), value.clone());
        }
    }
    state
}

/// 校验完整下载态中的全部 NodeGuard 策略，并返回单遍扫描统计供日志和回归测试核对。
fn verify_imported_policy_state<'a, I>(
    block: u32,
    pairs: I,
    cid_reference: &cid_lifecycle::GenesisReference,
) -> Result<ImportedPolicyStats, String>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    cid_lifecycle::check_state_import_height(block).map_err(|e| format!("CID 生命周期:{e:?}"))?;
    let state = partition_imported_state(pairs);
    governance_skeleton::check_catalog_keys(state.governance.keys())
        .map_err(|e| format!("固定治理岗位目录:{e:?}"))?;
    governance_skeleton::check_skeleton_invariants(|key| state.governance.get(key).cloned())
        .map_err(|e| format!("固定治理骨架:{e:?}"))?;
    national_body_composition::check_imported_state(&state.national_body_composition)
        .map_err(|e| format!("国家机构组成:{e:?}"))?;
    fullnode_issuance::check_imported_state_key_values(block, state.fullnode_issuance.iter())
        .map_err(|e| format!("全节点发行:{e:?}"))?;
    citizen_issuance::check_genesis_key_values(state.citizen_issuance.iter())
        .map_err(|e| format!("公民认证发行:{e:?}"))?;
    genesis_pallet::check_imported_state(state.genesis_pallet.iter())
        .map_err(|e| format!("创世模块:{e:?}"))?;
    provincialbank_interest::check_imported_genesis(state.provincialbank_interest.iter())
        .map_err(|e| format!("省储行固定发行:{e:?}"))?;
    runtime_policy::check_imported_state(state.runtime_policy.iter())
        .map_err(|e| format!("手续费制度:{e}"))?;
    cid_lifecycle::check_imported_genesis(state.cid.iter(), cid_reference)
        .map_err(|e| format!("CID 生命周期:{e:?}"))?;
    Ok(state.stats())
}

/// finalize 阶段只允许已注册发行计划改变账户和 `TotalIssuance`，其余变化一律 fail-closed。
fn verify_finalize_issuance<FPre, FPost>(
    pre_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    pre: &FPre,
    post: &FPost,
    plan: &FinalizeIssuancePlan,
) -> Result<(), String>
where
    FPre: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let total_key = fullnode_issuance::storage_key::total_issuance();
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

    let account_prefix = fullnode_issuance::storage_key::system_account_prefix();
    let mut changed_accounts = BTreeMap::<[u8; 32], ()>::new();
    for key in pre_delta.keys().chain(post_delta.keys()) {
        if key.starts_with(&account_prefix) && pre(key) != post(key) {
            changed_accounts.insert(parse_system_account_key(key, &account_prefix)?, ());
        }
    }
    for account in changed_accounts.keys() {
        if !plan.accounts.contains_key(account) {
            return Err(format!(
                "finalize 出现未登记账户变化:0x{}",
                hex::encode(account)
            ));
        }
    }

    const NEW_BALANCES_FLAGS: u128 = 0x80000000_00000000_00000000_00000000;
    for (account, expected) in &plan.accounts {
        let before = account_info(pre, account)?;
        let after = account_info(post, account)?
            .ok_or_else(|| format!("finalize 收款账户缺失:0x{}", hex::encode(account)))?;
        let before_free = before
            .as_ref()
            .map(|info| info.data.free)
            .unwrap_or_default();
        let balance_delta = signed_delta(before_free, after.data.free);
        let expected_delta = i128::try_from(*expected).unwrap_or(i128::MAX);
        if balance_delta != expected_delta {
            return Err(format!(
                "finalize 收款差额错误:账户 0x{},期望 {expected},实际 {balance_delta}",
                hex::encode(account)
            ));
        }
        if let Some(before) = before {
            if before.nonce != after.nonce
                || before.consumers != after.consumers
                || before.providers != after.providers
                || before.sufficients != after.sufficients
                || before.data.reserved != after.data.reserved
                || before.data.frozen != after.data.frozen
                || before.data.flags != after.data.flags
            {
                return Err(format!(
                    "finalize 非 free 账户字段被改写:0x{}",
                    hex::encode(account)
                ));
            }
        } else if after.nonce != 0
            || after.consumers != 0
            || after.providers != 1
            || after.sufficients != 0
            || after.data.reserved != 0
            || after.data.frozen != 0
            || after.data.flags != NEW_BALANCES_FLAGS
        {
            return Err(format!(
                "finalize 新建收款账户形态错误:0x{}",
                hex::encode(account)
            ));
        }
    }
    Ok(())
}

/// 对普通块导入方携带的预计算 storage changes 做自证一致性校验。
///
/// 本链 CPU/GPU 矿工会沿用 Substrate PoW worker 的 `ApplyChanges(Changes)` 快路径，
/// 但该字段来自导入方本地构块产物，不能被节点守卫信任。守卫已经用本节点 runtime
/// 只读重放同一块；若导入方携带的预计算变更与重放结果不一致，即使 header/state_root
/// 自洽，也必须在委派内层 import 前 fail-closed。
fn verify_precomputed_changes(
    params: &BlockImportParams<Block>,
    executed: &sp_state_machine::StorageChanges<sp_runtime::traits::HashingFor<Block>>,
) -> Result<(), String> {
    let StateAction::ApplyChanges(StorageChanges::Changes(precomputed)) = &params.state_action
    else {
        return Ok(());
    };

    if precomputed.transaction_storage_root != executed.transaction_storage_root {
        return Err("普通块预计算 state root 与本节点重放结果不一致".into());
    }
    if precomputed.main_storage_changes != executed.main_storage_changes {
        return Err("普通块预计算主存储变更与本节点重放结果不一致".into());
    }
    if precomputed.child_storage_changes != executed.child_storage_changes {
        return Err("普通块预计算子存储变更与本节点重放结果不一致".into());
    }
    if precomputed.offchain_storage_changes != executed.offchain_storage_changes {
        return Err("普通块预计算 offchain 存储变更与本节点重放结果不一致".into());
    }
    Ok(())
}

impl<I> NodeGuard<I> {
    /// 装配节点守卫。
    ///
    /// 启动阶段只做本机已持有状态的守卫自检：自检失败说明当前数据库里已有状态和当前
    /// 节点二进制不完全匹配，但不能反过来杀死节点进程。节点守卫真正执法的边界是后续
    /// 区块、状态包和候选 runtime 导入；这些路径仍然 fail-closed，坏输入一律不委派内层导入器。
    pub fn new(inner: I, client: Arc<FullClient>, backend: Arc<FullBackend>) -> Self {
        let genesis_hash = client.info().genesis_hash;
        let mut startup_issues = Vec::<String>::new();

        let governance_catalog_keys = match Self::state_keys_for_prefixes(
            &client,
            genesis_hash,
            governance_skeleton::storage_key::fixed_catalog_prefixes(),
        ) {
            Ok(keys) => keys,
            Err(reason) => {
                startup_issues.push(format!("固定治理岗位目录枚举失败:{reason}"));
                Vec::new()
            }
        };
        if let Err(reason) = governance_skeleton::check_catalog_keys(&governance_catalog_keys) {
            startup_issues.push(format!("固定治理岗位目录校验失败:{reason:?}"));
        }
        if let Err(reason) = governance_skeleton::check_skeleton_invariants(|key| {
            client
                .storage(genesis_hash, &StorageKey(key.to_vec()))
                .ok()
                .flatten()
                .map(|data| data.0)
        }) {
            startup_issues.push(format!("固定治理骨架校验失败:{reason:?}"));
        }
        if let Err(reason) = national_body_composition::check_full_state(|key| {
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
                national_body_composition::storage_key::threshold_prefix(),
                national_body_composition::storage_key::proposal_prefix(),
            ],
        ) {
            Ok(keys) => keys,
            Err(reason) => {
                startup_issues.push(format!("固定治理阈值枚举失败:{reason}"));
                Vec::new()
            }
        };
        if let Err(reason) =
            national_body_composition::check_vote_state_keys(&vote_state_keys, |key| {
                client
                    .storage(genesis_hash, &StorageKey(key.to_vec()))
                    .ok()
                    .flatten()
                    .map(|data| data.0)
            })
        {
            startup_issues.push(format!("固定治理阈值校验失败:{reason:?}"));
        }
        if let Err(reason) = fullnode_issuance::check_genesis(|key| {
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
            provincialbank_interest::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("省储行固定发行状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        for key in provincialbank_interest::relevant_import_keys() {
            if key.starts_with(&provincialbank_interest::storage_key::pallet_prefix()) {
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
        if let Err(reason) =
            provincialbank_interest::check_imported_genesis(provincialbank_state.iter())
        {
            startup_issues.push(format!("省储行固定发行校验失败:{reason:?}"));
        }
        let citizen_issuance_state = match Self::pallet_state(
            &client,
            genesis_hash,
            citizen_issuance::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("公民认证发行状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        if let Err(reason) =
            citizen_issuance::check_genesis_key_values(citizen_issuance_state.iter())
        {
            startup_issues.push(format!("公民认证发行状态校验失败:{reason:?}"));
        }
        let genesis_pallet_state = match Self::pallet_state(
            &client,
            genesis_hash,
            genesis_pallet::storage_key::pallet_prefix(),
        ) {
            Ok(state) => state,
            Err(reason) => {
                startup_issues.push(format!("GenesisPallet 状态枚举失败:{reason}"));
                BTreeMap::new()
            }
        };
        if let Err(reason) =
            genesis_pallet::check_genesis(|key| genesis_pallet_state.get(key).cloned())
        {
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
        if let Err(reason) = runtime_policy::check_imported_state(runtime_policy_state.iter()) {
            startup_issues.push(format!("手续费制度校验失败:{reason}"));
        }
        let cid_lifecycle = match Self::cid_state_keys(&client, genesis_hash)
            .map_err(|reason| format!("CID 生命周期基准枚举失败:{reason}"))
            .and_then(|cid_keys| {
                cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| {
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
        for prefix in cid_lifecycle::storage_key::enumerated_prefixes() {
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

    /// 枚举若干精确 storage 子树；固定治理岗位只扫描固定机构 CID，不扫描全部机构。
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

    /// 枚举指定 pallet 的全部 RAW key/value；只用于启动与 block#0 完整导入基准。
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

    /// 提交前校验 warp/状态导入携带的完整下载态；无法抽取或不满足策略时一律拒绝导入。
    fn verify_imported_state(&self, params: &BlockImportParams<Block>) -> Result<(), String> {
        let stats = verify_imported_state_params(params, self.cid_lifecycle.as_ref())?;
        log::debug!(
            target: "node-guard",
            "完整状态单遍扫描:总键 {},治理 {},国家组成/固定阈值 {},全节点发行/账户 {},公民发行 {},创世模块 {},省储行固定发行 {},手续费制度 {},CID {}",
            stats.scanned,
            stats.governance,
            stats.national_body_composition,
            stats.fullnode_issuance,
            stats.citizen_issuance,
            stats.genesis_pallet,
            stats.provincialbank_interest,
            stats.runtime_policy,
            stats.cid,
        );
        Ok(())
    }

    /// 对正常执行型区块统一预执行一次，并检查当前已注册的全部节点永久策略。
    fn detect_violation(&self, params: &BlockImportParams<Block>) -> Result<bool, String> {
        let body = params
            .body
            .clone()
            .ok_or_else(|| "普通区块缺少 body,无法复算 finalize 前后发行状态".to_string())?;
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
        verify_precomputed_changes(params, &changes)?;
        let post_delta: BTreeMap<Vec<u8>, Option<Vec<u8>>> =
            changes.main_storage_changes.into_iter().collect();

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

        if let Err(reason) = runtime_policy::check_transition(&post_delta, read_post) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):手续费制度状态非法 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }
        if let Err(reason) = runtime_policy::check_block(&body, read_post) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):实际手续费结果非法 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        let mut issuance_plan = FinalizeIssuancePlan::default();
        if let Err(reason) = genesis_pallet::check_transition(&post_delta, &read_parent, &read_post)
        {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):创世模块永久规则被破坏 —— {:?}",
                params.header.number(),
                params.post_hash(),
                reason,
            );
            return Ok(true);
        }

        if let Err(reason) = fullnode_issuance::check_transition(
            *params.header.number(),
            fullnode_issuance::author_from_header(&params.header),
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

        if let Err(reason) = citizen_issuance::check_transition(
            extrinsic_count,
            &pre_delta,
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

        if let Err(reason) = provincialbank_interest::check_transition(
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

        if let Err(reason) = verify_finalize_issuance(
            &pre_delta,
            &post_delta,
            &read_pre,
            &read_post,
            &issuance_plan,
        ) {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):finalize 统一发行核算失败 —— {reason}",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        if let Some(cid_lifecycle) = self.cid_lifecycle.as_ref() {
            if let Err(reason) = cid_lifecycle::check_transition(
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
            .any(|key| cid_lifecycle::is_relevant_key(key))
            || cid_lifecycle::needs_full_check(&post_delta)
        {
            log::error!(
                target: "node-guard",
                "拒绝区块 #{} ({:?}):CID 生命周期启动基准不可用，不能验证受保护状态变更",
                params.header.number(),
                params.post_hash(),
            );
            return Ok(true);
        }

        if cid_lifecycle::needs_full_check(&post_delta) {
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
                .filter(|key| cid_lifecycle::is_relevant_key(key))
            {
                keys.insert(key.clone(), ());
            }
            let keys: Vec<Vec<u8>> = keys.into_keys().collect();
            if let Err(reason) = cid_lifecycle::check_full_state(&keys, read_post, cid_lifecycle) {
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
                runtime_policy::storage_key::relevant_prefixes().to_vec(),
            )?
            .into_iter()
            .map(|key| (key, ()))
            .collect::<BTreeMap<_, _>>();
            fee_keys.insert(runtime_policy::storage_key::max_rate(), ());
            for key in post_delta
                .keys()
                .filter(|key| runtime_policy::storage_key::is_relevant(key))
            {
                fee_keys.insert(key.clone(), ());
            }
            let fee_state = fee_keys
                .into_keys()
                .filter_map(|key| read_post(&key).map(|value| (key, value)))
                .collect::<BTreeMap<_, _>>();
            if let Err(reason) = runtime_policy::check_imported_state(fee_state.iter()) {
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
            if let Err(reason) = runtime_policy::check_candidate_runtime(
                &parent_state,
                parent_hash,
                params.post_hash(),
                *params.header.number(),
                &post_delta,
                &candidate_code,
                self.client.info().genesis_hash,
            ) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):候选 runtime 手续费行为非法 —— {reason}",
                    params.header.number(),
                    params.post_hash(),
                );
                return Ok(true);
            }
            if let Err(reason) = genesis_pallet::check_full_state(read_post) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):runtime 升级后的创世模块全检失败 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
            if let Err(reason) =
                provincialbank_interest::check_full_state(*params.header.number(), &read_post)
            {
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

        // 治理骨架只在受保护机构精确 key 变化时按机构复核；`:code` 变化才全量复核。
        if governance_skeleton::needs_full_check(&post_delta) {
            if let Err(reason) = governance_skeleton::check_catalog_keys(post_delta.keys()) {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):固定治理岗位目录被破坏 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
            if let Err(reason) =
                governance_skeleton::check_affected_institutions(&post_delta, read_post)
            {
                log::error!(
                    target: "node-guard",
                    "拒绝区块 #{} ({:?}):固定治理骨架不变式被破坏 —— {:?}",
                    params.header.number(),
                    params.post_hash(),
                    reason,
                );
                return Ok(true);
            }
        }
        // runtime 升级必须复核升级前已存在、升级后仍存在以及本块新增/删除的全部内部
        // 提案和阈值快照，防止新 WASM 通过移走旧 storage 绕过五类固定治理阈值语义。
        let runtime_upgrade_vote_keys = if post_delta
            .contains_key(sp_storage::well_known_keys::CODE)
        {
            let mut keys = Self::state_keys_for_prefixes(
                &self.client,
                parent_hash,
                vec![
                    national_body_composition::storage_key::threshold_prefix(),
                    national_body_composition::storage_key::proposal_prefix(),
                ],
            )?
            .into_iter()
            .map(|key| (key, ()))
            .collect::<BTreeMap<_, _>>();
            for key in post_delta.keys().filter(|key| {
                key.starts_with(&national_body_composition::storage_key::threshold_prefix())
                    || key.starts_with(&national_body_composition::storage_key::proposal_prefix())
            }) {
                keys.insert(key.clone(), ());
            }
            Some(keys.into_keys().collect::<Vec<_>>())
        } else {
            None
        };
        if let Err(reason) = national_body_composition::check_transition(
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
        Ok(false)
    }
}

/// 校验导入队列/warp 携带的完整状态包，并返回共享单遍扫描统计。
///
/// 该函数不读取节点数据库，方便测试直接构造 `BlockImportParams::state_action` 证明
/// 完整状态导入路径会在委派内层导入前执行永久规则。
fn verify_imported_state_params(
    params: &BlockImportParams<Block>,
    cid_reference: Option<&cid_lifecycle::GenesisReference>,
) -> Result<ImportedPolicyStats, String> {
    let imported = match &params.state_action {
        StateAction::ApplyChanges(StorageChanges::Import(imported)) => imported,
        _ => return Err("warp 状态非 ApplyChanges(Import) 形态,无法提交前校验".into()),
    };
    let cid_reference = cid_reference.ok_or("CID 生命周期启动基准不可用，拒绝完整状态导入")?;
    // 所有策略复用同一遍 imported state 扫描，禁止为单项永久规则再新增包装器。
    verify_imported_policy_state(
        *params.header.number(),
        imported
            .state
            .0
            .iter()
            .flat_map(|level| level.key_values.iter())
            .map(|(key, value)| (key, value)),
        cid_reference,
    )
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
        params: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        if params.with_state() {
            let verdict = self.verify_imported_state(&params);
            if let Err(reason) = &verdict {
                log::error!(
                    target: "node-guard",
                    "拒绝 warp/状态导入 ({:?}):节点永久规则校验未通过 —— {reason}",
                    params.post_hash(),
                );
            }
            return import_if_verified(&self.inner, params, verdict).await;
        }

        let verdict = match self.detect_violation(&params) {
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

#[cfg(test)]
mod finalize_issuance_tests {
    use super::*;

    fn full_runtime_genesis_storage() -> sp_runtime::Storage {
        use sp_runtime::BuildStorage;

        let config: citizenchain::RuntimeGenesisConfig =
            serde_json::from_value(citizenchain::genesis::genesis_config())
                .expect("decode complete runtime genesis config");
        config
            .build_storage()
            .expect("build complete runtime genesis storage")
    }
    use codec::Encode;
    use sc_chain_spec::{ChainType, Properties};
    use sc_network::config::NetworkConfiguration;
    use sc_service::config::{
        ExecutorConfiguration, KeystoreConfig, RpcBatchRequestConfig, RpcConfiguration,
    };
    use sc_service::{
        BasePath, BlocksPruning, Configuration, DatabaseSource, PruningMode, Role,
        TransactionPoolOptions,
    };
    use sp_api::ProvideRuntimeApi;
    use sp_consensus::BlockOrigin;
    use sp_consensus_pow::POW_ENGINE_ID;
    use sp_core::{
        crypto::{Ss58AddressFormat, Ss58Codec},
        sr25519, Pair as _,
    };
    use sp_keyring::Sr25519Keyring;
    use sp_state_machine::Backend as _;
    use sp_storage::ChildInfo;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn block_requires_timestamp_and_at_least_one_user_transaction() {
        assert!(!has_user_transaction(0));
        assert!(!has_user_transaction(1));
        assert!(has_user_transaction(2));
    }

    #[derive(Default)]
    struct CountingImport {
        imports: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl BlockImport<Block> for CountingImport {
        type Error = ConsensusError;

        async fn check_block(
            &self,
            _block: BlockCheckParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            Ok(ImportResult::AlreadyInChain)
        }

        async fn import_block(
            &self,
            _block: BlockImportParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            self.imports.fetch_add(1, Ordering::SeqCst);
            Ok(ImportResult::AlreadyInChain)
        }
    }

    #[derive(Clone, Default)]
    struct SharedCountingImport {
        imports: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl BlockImport<Block> for SharedCountingImport {
        type Error = ConsensusError;

        async fn check_block(
            &self,
            _block: BlockCheckParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            Ok(ImportResult::AlreadyInChain)
        }

        async fn import_block(
            &self,
            _block: BlockImportParams<Block>,
        ) -> Result<ImportResult, Self::Error> {
            self.imports.fetch_add(1, Ordering::SeqCst);
            Ok(ImportResult::AlreadyInChain)
        }
    }

    fn test_chain_spec() -> crate::core::chain_spec::ChainSpec {
        let wasm = citizenchain::WASM_BINARY.expect("test requires embedded runtime WASM");
        let mut genesis_patch = citizenchain::genesis::genesis_config();
        let alice = Sr25519Keyring::Alice
            .to_account_id()
            .to_ss58check_with_version(Ss58AddressFormat::custom(
                primitives::core_const::SS58_FORMAT,
            ));
        genesis_patch["balances"]["balances"]
            .as_array_mut()
            .expect("genesis balances array")
            .push(serde_json::json!([alice, 1_000_000_000_000u128]));

        let mut properties = Properties::new();
        properties.insert(
            "ss58Format".into(),
            serde_json::json!(primitives::core_const::SS58_FORMAT),
        );
        properties.insert("tokenDecimals".into(), serde_json::json!(2));
        properties.insert("tokenSymbol".into(), serde_json::json!("GMB"));

        crate::core::chain_spec::ChainSpec::builder(wasm, None)
            .with_name("CitizenChain NodeGuard Test")
            .with_id("citizenchain-node-guard-test")
            .with_chain_type(ChainType::Development)
            .with_protocol_id("citizenchain-node-guard-test")
            .with_properties(properties)
            .with_genesis_config_patch(genesis_patch)
            .build()
    }

    fn test_config(tokio_handle: tokio::runtime::Handle) -> Configuration {
        let base_path = BasePath::new_temp_dir().expect("create node guard bad-block temp base");
        let root = base_path.path().to_path_buf();
        let network = NetworkConfiguration::new(
            "node-guard-bad-block-test",
            "citizenchain-node-guard-test/0.1",
            Default::default(),
            None,
        );

        Configuration {
            impl_name: "citizenchain-node-guard-test".into(),
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
            chain_spec: Box::new(test_chain_spec()),
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
            disable_grandpa: false,
            dev_key_seed: None,
            tracing_targets: None,
            tracing_receiver: Default::default(),
            announce_block: true,
            data_path: root,
            base_path,
        }
    }

    fn skip_without_wasm_binary(test_name: &str) -> bool {
        if citizenchain::WASM_BINARY.is_some() {
            return false;
        }
        eprintln!("{test_name}: 跳过真实服务级坏块验收；当前测试构建未内置 WASM_BINARY");
        true
    }

    fn remark_extrinsic(genesis_hash: <Block as BlockT>::Hash) -> <Block as BlockT>::Extrinsic {
        let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
            &format!("{genesis_hash:?}"),
            0,
            citizenchain::VERSION.spec_version,
            citizenchain::VERSION.transaction_version,
            b"node-guard-bad-block",
        )
        .expect("build signed remark extrinsic");
        let raw = hex::decode(hex.trim_start_matches("0x")).expect("decode remark extrinsic hex");
        sp_runtime::OpaqueExtrinsic::try_from_encoded_extrinsic(&raw)
            .expect("decode opaque remark extrinsic")
    }

    fn timestamp_extrinsic(now: u64) -> <Block as BlockT>::Extrinsic {
        let xt = citizenchain::UncheckedExtrinsic::new_bare(citizenchain::RuntimeCall::Timestamp(
            citizenchain::TimestampCall::set { now },
        ));
        xt.into()
    }

    fn legal_remark_block_params(client: &Arc<FullClient>) -> BlockImportParams<Block> {
        let parent_hash = client.info().genesis_hash;
        let pow_author =
            sr25519::Pair::from_string("//Alice//pow", None).expect("derive test pow author");
        let mut digest = sp_runtime::Digest::default();
        digest.push(sp_runtime::DigestItem::PreRuntime(
            POW_ENGINE_ID,
            pow_author.public().encode(),
        ));
        let mut builder = sc_block_builder::BlockBuilderBuilder::new(&**client)
            .on_parent_block(parent_hash)
            .fetch_parent_block_number(&**client)
            .expect("fetch genesis number")
            .with_inherent_digests(digest)
            .build()
            .expect("create block builder");

        builder
            .push(timestamp_extrinsic(1_782_950_406_000))
            .expect("push timestamp inherent");
        builder
            .push(remark_extrinsic(parent_hash))
            .expect("push signed remark");

        let built = builder.build().expect("build legal remark block");
        let (block, storage_changes) = built.into_inner();
        let (header, body) = block.deconstruct();
        let mut params = BlockImportParams::new(BlockOrigin::Own, header);
        params.body = Some(body);
        params.state_action = StateAction::ApplyChanges(StorageChanges::Changes(storage_changes));
        params
    }

    fn mutate_precomputed_changes_to_guarded_state(
        params: &mut BlockImportParams<Block>,
        client: &Arc<FullClient>,
        backend: &Arc<FullBackend>,
        update_header_root: bool,
    ) {
        let parent_hash = *params.header.parent_hash();
        let StateAction::ApplyChanges(StorageChanges::Changes(changes)) = &mut params.state_action
        else {
            panic!("legal remark block must carry precomputed storage changes");
        };
        assert!(
            changes.child_storage_changes.is_empty(),
            "本坏块样本只篡改主存储，若出现 child delta 必须显式扩展重算逻辑"
        );

        let guarded_key = genesis_pallet::storage_key::citizen_max();
        let guarded_value = 1_443_497_379u64.encode();
        if let Some((_, value)) = changes
            .main_storage_changes
            .iter_mut()
            .find(|(key, _)| key == &guarded_key)
        {
            *value = Some(guarded_value);
        } else {
            changes
                .main_storage_changes
                .push((guarded_key, Some(guarded_value)));
        }

        let parent_state = backend
            .state_at(parent_hash, TrieCacheContext::Untrusted)
            .expect("open parent state");
        let state_version = client
            .runtime_api()
            .version(parent_hash)
            .expect("read runtime version")
            .state_version();
        let (bad_root, bad_transaction) = parent_state.full_storage_root(
            changes
                .main_storage_changes
                .iter()
                .map(|(key, value)| (&key[..], value.as_deref())),
            std::iter::empty::<(&ChildInfo, std::iter::Empty<(&[u8], Option<&[u8]>)>)>(),
            state_version,
        );
        changes.transaction_storage_root = bad_root;
        changes.transaction = bad_transaction;
        if update_header_root {
            params.header.set_state_root(bad_root);
        }
    }

    fn import_params(number: u32) -> BlockImportParams<Block> {
        use sp_consensus::BlockOrigin;
        use sp_core::H256;
        use sp_runtime::Digest;

        let header = citizenchain::opaque::Header::new(
            number,
            H256::repeat_byte(1),
            H256::repeat_byte(2),
            H256::repeat_byte(3),
            Digest::default(),
        );
        BlockImportParams::new(BlockOrigin::NetworkInitialSync, header)
    }

    /// 构造真实完整状态导入形态，覆盖 `params.with_state()` 分支。
    fn import_params_with_state(
        number: u32,
        state: BTreeMap<Vec<u8>, Vec<u8>>,
    ) -> BlockImportParams<Block> {
        let mut params = import_params(number);
        let block = params.header.hash();
        let key_values = state.into_iter().collect::<Vec<_>>();
        let state = sc_client_api::KeyValueStates::from([(
            Vec::new(),
            (key_values, Vec::<Vec<u8>>::new()),
        )]);
        params.state_action =
            StateAction::ApplyChanges(StorageChanges::Import(sc_consensus::ImportedState {
                block,
                state,
            }));
        params
    }

    fn cid_genesis_reference(top: &BTreeMap<Vec<u8>, Vec<u8>>) -> cid_lifecycle::GenesisReference {
        let cid_keys: Vec<Vec<u8>> = top
            .keys()
            .filter(|key| cid_lifecycle::is_relevant_key(key))
            .cloned()
            .collect();
        cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| top.get(key).cloned())
            .expect("build CID genesis reference")
    }

    const NEW_BALANCES_FLAGS: u128 = 0x80000000_00000000_00000000_00000000;

    fn account(free: u128, providers: u32, flags: u128) -> Vec<u8> {
        (0u32, 0u32, providers, 0u32, (free, 0u128, 0u128, flags)).encode()
    }

    #[test]
    fn combined_rewards_for_same_account_are_checked_once() {
        let recipient_account_id = [9u8; 32];
        let total_key = fullnode_issuance::storage_key::total_issuance();
        let account_key = fullnode_issuance::storage_key::system_account(&recipient_account_id);
        let mut pre = BTreeMap::new();
        let mut post = BTreeMap::new();
        pre.insert(total_key.clone(), 1_000_000u128.encode());
        post.insert(total_key.clone(), 2_999_800u128.encode());
        pre.insert(account_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
        post.insert(
            account_key.clone(),
            account(1_999_900, 1, NEW_BALANCES_FLAGS),
        );
        let pre_delta = BTreeMap::new();
        let post_delta = BTreeMap::from([
            (total_key, Some(2_999_800u128.encode())),
            (account_key, Some(account(1_999_900, 1, NEW_BALANCES_FLAGS))),
        ]);
        let mut plan = FinalizeIssuancePlan::default();
        plan.add(recipient_account_id, 999_900)
            .expect("first reward");
        plan.add(recipient_account_id, 999_900)
            .expect("second reward");
        assert_eq!(
            verify_finalize_issuance(
                &pre_delta,
                &post_delta,
                &|key| pre.get(key).cloned(),
                &|key| post.get(key).cloned(),
                &plan,
            ),
            Ok(())
        );
    }

    #[test]
    fn unexpected_finalize_account_change_is_rejected() {
        let recipient_account_id = [1u8; 32];
        let attacker = [2u8; 32];
        let total_key = fullnode_issuance::storage_key::total_issuance();
        let recipient_key = fullnode_issuance::storage_key::system_account(&recipient_account_id);
        let attacker_key = fullnode_issuance::storage_key::system_account(&attacker);
        let mut pre = BTreeMap::new();
        let mut post = BTreeMap::new();
        pre.insert(total_key.clone(), 1_000u128.encode());
        post.insert(total_key.clone(), 1_100u128.encode());
        pre.insert(recipient_key.clone(), account(0, 1, NEW_BALANCES_FLAGS));
        post.insert(recipient_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
        pre.insert(attacker_key.clone(), account(50, 1, NEW_BALANCES_FLAGS));
        post.insert(attacker_key.clone(), account(51, 1, NEW_BALANCES_FLAGS));
        let post_delta = BTreeMap::from([
            (total_key, Some(1_100u128.encode())),
            (recipient_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
            (attacker_key, Some(account(51, 1, NEW_BALANCES_FLAGS))),
        ]);
        let mut plan = FinalizeIssuancePlan::default();
        plan.add(recipient_account_id, 100).expect("reward");
        let error = verify_finalize_issuance(
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        )
        .expect_err("attacker change must fail");
        assert!(error.contains("未登记账户变化"));
    }

    #[test]
    fn initialize_business_transfer_is_not_misclassified_as_finalize_issuance() {
        let payer = [3u8; 32];
        let payee = [4u8; 32];
        let total_key = fullnode_issuance::storage_key::total_issuance();
        let payer_key = fullnode_issuance::storage_key::system_account(&payer);
        let payee_key = fullnode_issuance::storage_key::system_account(&payee);
        // on_initialize 的零和业务转账在 finalize 前、后读取结果完全相同；NodeGuard
        // 只应核对 finalize 自身变化，不能把合法订阅续费误判成原生发行。
        let state = BTreeMap::from([
            (total_key.clone(), 1_000u128.encode()),
            (payer_key.clone(), account(900, 1, NEW_BALANCES_FLAGS)),
            (payee_key.clone(), account(100, 1, NEW_BALANCES_FLAGS)),
        ]);
        let delta = BTreeMap::from([
            (payer_key, Some(account(900, 1, NEW_BALANCES_FLAGS))),
            (payee_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
        ]);
        assert_eq!(
            verify_finalize_issuance(
                &delta,
                &delta,
                &|key| state.get(key).cloned(),
                &|key| state.get(key).cloned(),
                &FinalizeIssuancePlan::default(),
            ),
            Ok(())
        );
    }

    #[test]
    fn new_reward_account_must_match_balances_default_shape() {
        let recipient_account_id = [3u8; 32];
        let total_key = fullnode_issuance::storage_key::total_issuance();
        let account_key = fullnode_issuance::storage_key::system_account(&recipient_account_id);
        let pre = BTreeMap::from([(total_key.clone(), 1_000u128.encode())]);
        let mut post = BTreeMap::from([(total_key.clone(), 1_100u128.encode())]);
        post.insert(account_key.clone(), account(100, 1, NEW_BALANCES_FLAGS));
        let post_delta = BTreeMap::from([
            (total_key, Some(1_100u128.encode())),
            (account_key, Some(account(100, 1, NEW_BALANCES_FLAGS))),
        ]);
        let mut plan = FinalizeIssuancePlan::default();
        plan.add(recipient_account_id, 100).expect("reward");
        assert_eq!(
            verify_finalize_issuance(
                &BTreeMap::new(),
                &post_delta,
                &|key| pre.get(key).cloned(),
                &|key| post.get(key).cloned(),
                &plan,
            ),
            Ok(())
        );
    }

    #[test]
    fn guarded_import_delegates_only_after_explicit_success() {
        let inner = CountingImport::default();
        let accepted =
            futures::executor::block_on(import_if_verified(&inner, import_params(1), Ok(())))
                .expect("verified import result");
        assert_eq!(accepted, ImportResult::AlreadyInChain);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

        let rejected = futures::executor::block_on(import_if_verified(
            &inner,
            import_params(2),
            Err("malicious state".into()),
        ))
        .expect("known bad result");
        assert_eq!(rejected, ImportResult::KnownBad);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

        let rejected_again = futures::executor::block_on(import_if_verified(
            &inner,
            import_params(3),
            Err("another malicious state".into()),
        ))
        .expect("second known bad result");
        assert_eq!(rejected_again, ImportResult::KnownBad);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 1);

        // 连续拒绝不保存污染状态；随后合法区块仍只委派一次。
        let accepted_after_rejection =
            futures::executor::block_on(import_if_verified(&inner, import_params(4), Ok(())))
                .expect("verified import after rejection");
        assert_eq!(accepted_after_rejection, ImportResult::AlreadyInChain);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn imported_state_bad_cases_return_known_bad_before_inner_import() {
        let storage = full_runtime_genesis_storage();
        let top = storage.top;
        let reference = cid_genesis_reference(&top);

        for case in blockchain_harness::ImportedStateBadCaseKind::all() {
            let inner = CountingImport::default();
            let params = import_params_with_state(0, imported_state_bad_case(*case, &top));
            assert!(
                params.with_state(),
                "{} must enter with_state path",
                case.label()
            );

            let verdict = verify_imported_state_params(&params, Some(&reference));
            let err = verdict
                .as_ref()
                .expect_err("bad imported state must fail before inner import");
            assert!(
                err.starts_with(case.expected_error_prefix()),
                "{} must fail with prefix {}, actual {err}",
                case.label(),
                case.expected_error_prefix()
            );

            let result = futures::executor::block_on(import_if_verified(
                &inner,
                params,
                verdict.map(|_| ()),
            ))
            .expect("known bad import result");
            assert_eq!(result, ImportResult::KnownBad);
            assert_eq!(
                inner.imports.load(Ordering::SeqCst),
                0,
                "{} must not delegate inner import",
                case.label()
            );
        }
    }

    #[test]
    fn legal_block_zero_imported_state_delegates_after_guard_success() {
        let storage = full_runtime_genesis_storage();
        let top = storage.top;
        let reference = cid_genesis_reference(&top);
        let inner = CountingImport::default();
        let params = import_params_with_state(0, top.clone());

        let stats = verify_imported_state_params(&params, Some(&reference))
            .expect("legal block zero complete state must pass");
        assert_eq!(stats.scanned, top.len());

        let result = futures::executor::block_on(import_if_verified(&inner, params, Ok(())))
            .expect("legal imported state should delegate");
        assert_eq!(result, ImportResult::AlreadyInChain);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn precomputed_changes_must_match_reexecuted_normal_block() {
        if skip_without_wasm_binary("precomputed_changes_must_match_reexecuted_normal_block") {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let config = test_config(runtime.handle().clone());
        let sc_service::PartialComponents {
            client,
            backend,
            task_manager: _task_manager,
            ..
        } = crate::core::service::new_partial(&config).expect("create partial node service");
        let guard = NodeGuard::new(CountingImport::default(), client.clone(), backend.clone());

        let legal = legal_remark_block_params(&client);
        assert_eq!(
            guard.detect_violation(&legal),
            Ok(false),
            "真实 BlockBuilder 产物的预计算 changes 必须与节点重放结果一致"
        );

        let mut malicious = legal_remark_block_params(&client);
        mutate_precomputed_changes_to_guarded_state(&mut malicious, &client, &backend, false);
        let err = guard
            .detect_violation(&malicious)
            .expect_err("篡改预计算 changes 必须被节点守卫 fail-closed");
        assert!(
            err.contains("预计算"),
            "应由预计算 changes 一致性检查拒绝，实际错误: {err}"
        );
    }

    #[test]
    fn imported_state_rejects_when_cid_startup_baseline_unavailable() {
        let storage = full_runtime_genesis_storage();
        let params = import_params_with_state(0, storage.top);
        let err = verify_imported_state_params(&params, None)
            .expect_err("完整状态导入必须依赖 CID 启动基准");
        assert!(
            err.contains("CID 生命周期启动基准不可用"),
            "错误必须说明是启动基准不可用，实际错误: {err}"
        );
    }

    #[test]
    fn current_wasm_passes_candidate_runtime_policy_behavior_probes() {
        if skip_without_wasm_binary("current_wasm_passes_candidate_runtime_policy_behavior_probes")
        {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let config = test_config(runtime.handle().clone());
        let sc_service::PartialComponents {
            client,
            backend,
            task_manager: _task_manager,
            ..
        } = crate::core::service::new_partial(&config).expect("create partial node service");
        let genesis_hash = client.info().genesis_hash;
        let parent_state = backend
            .state_at(genesis_hash, TrieCacheContext::Untrusted)
            .expect("read genesis state");
        runtime_policy::check_candidate_runtime(
            &parent_state,
            genesis_hash,
            genesis_hash,
            0,
            &BTreeMap::new(),
            citizenchain::WASM_BINARY.expect("checked above"),
            genesis_hash,
        )
        .expect("current WASM must satisfy node-side runtime policy behavior probes");
    }

    #[test]
    fn self_consistent_bad_precomputed_block_is_known_bad_before_inner_import() {
        if skip_without_wasm_binary(
            "self_consistent_bad_precomputed_block_is_known_bad_before_inner_import",
        ) {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let config = test_config(runtime.handle().clone());
        let sc_service::PartialComponents {
            client,
            backend,
            task_manager: _task_manager,
            ..
        } = crate::core::service::new_partial(&config).expect("create partial node service");
        let inner = SharedCountingImport::default();
        let imports = inner.imports.clone();
        let guard = NodeGuard::new(inner, client.clone(), backend.clone());

        let mut malicious = legal_remark_block_params(&client);
        mutate_precomputed_changes_to_guarded_state(&mut malicious, &client, &backend, true);
        let result = runtime
            .block_on(guard.import_block(malicious))
            .expect("node guard import result");
        assert_eq!(result, ImportResult::KnownBad);
        assert_eq!(
            imports.load(Ordering::SeqCst),
            0,
            "自洽坏块必须在委派 inner import 前被拒绝"
        );
    }

    #[test]
    fn finalize_plan_rejects_overflow_wrong_total_and_non_free_mutation() {
        let mut overflow = FinalizeIssuancePlan::default();
        overflow.add([1u8; 32], u128::MAX).expect("first amount");
        assert_eq!(overflow.add([2u8; 32], 1), Err(()));

        let recipient_account_id = [4u8; 32];
        let total_key = fullnode_issuance::storage_key::total_issuance();
        let account_key = fullnode_issuance::storage_key::system_account(&recipient_account_id);
        let pre = BTreeMap::from([
            (total_key.clone(), 1_000u128.encode()),
            (account_key.clone(), account(10, 1, NEW_BALANCES_FLAGS)),
        ]);
        let mut post = BTreeMap::from([
            (total_key.clone(), 1_099u128.encode()),
            (account_key.clone(), account(110, 1, NEW_BALANCES_FLAGS)),
        ]);
        let mut post_delta = post
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect::<BTreeMap<_, _>>();
        let mut plan = FinalizeIssuancePlan::default();
        plan.add(recipient_account_id, 100).expect("reward");
        let wrong_total = verify_finalize_issuance(
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        )
        .expect_err("wrong total must fail");
        assert!(wrong_total.contains("总发行差额错误"));

        post.insert(total_key.clone(), 1_100u128.encode());
        let mutated = (
            1u32,
            0u32,
            1u32,
            0u32,
            (110u128, 0u128, 0u128, NEW_BALANCES_FLAGS),
        )
            .encode();
        post.insert(account_key.clone(), mutated.clone());
        post_delta.insert(total_key, Some(1_100u128.encode()));
        post_delta.insert(account_key, Some(mutated));
        let non_free = verify_finalize_issuance(
            &BTreeMap::new(),
            &post_delta,
            &|key| pre.get(key).cloned(),
            &|key| post.get(key).cloned(),
            &plan,
        )
        .expect_err("nonce mutation must fail");
        assert!(non_free.contains("非 free 账户字段被改写"));
    }

    #[test]
    fn imported_state_is_partitioned_in_one_shared_pass() {
        let protected = primitives::governance_skeleton::fixed_institutions()[0];
        let governance =
            governance_skeleton::storage_key::account_id(protected.cid_number.as_bytes());
        let ordinary_governance = governance_skeleton::storage_key::account_id(b"ORDINARY-CID");
        let composition = national_body_composition::storage_key::composition_keys(
            &primitives::institution_constraints::member_composition_specs()[0],
        )[0]
        .clone();
        let fullnode = fullnode_issuance::storage_key::rewarded_block_count();
        let citizen = citizen_issuance::storage_key::rewarded_count();
        let genesis = genesis_pallet::storage_key::citizen_max();
        let provincialbank = provincialbank_interest::storage_key::last_settled_year();
        let runtime_policy = runtime_policy::storage_key::max_rate();
        let cid = cid_lifecycle::storage_key::citizen_registry_prefix();
        let unrelated = b"unrelated".to_vec();
        let pairs = BTreeMap::from([
            (governance.clone(), vec![1]),
            (ordinary_governance, vec![1]),
            (composition.clone(), vec![1]),
            (fullnode.clone(), vec![2]),
            (citizen.clone(), vec![3]),
            (genesis.clone(), vec![4]),
            (provincialbank.clone(), vec![4]),
            (runtime_policy.clone(), 10u32.encode()),
            (cid.clone(), vec![4]),
            (unrelated, vec![5]),
        ]);
        let state = partition_imported_state(pairs.iter());
        assert_eq!(state.scanned, 10);
        assert_eq!(
            state.governance.keys().cloned().collect::<Vec<_>>(),
            [governance]
        );
        assert_eq!(
            state
                .national_body_composition
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [composition]
        );
        assert_eq!(
            state.fullnode_issuance.keys().cloned().collect::<Vec<_>>(),
            [fullnode]
        );
        assert_eq!(
            state.citizen_issuance.keys().cloned().collect::<Vec<_>>(),
            [citizen]
        );
        assert_eq!(
            state.genesis_pallet.keys().cloned().collect::<Vec<_>>(),
            [genesis]
        );
        assert_eq!(
            state
                .provincialbank_interest
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [provincialbank]
        );
        assert_eq!(
            state.runtime_policy.keys().cloned().collect::<Vec<_>>(),
            [runtime_policy]
        );
        assert_eq!(state.cid.keys().cloned().collect::<Vec<_>>(), [cid]);
    }

    #[test]
    fn real_genesis_complete_state_passes_all_policies_in_one_scan() {
        let storage = full_runtime_genesis_storage();
        let top = storage.top;
        let cid_keys: Vec<Vec<u8>> = top
            .keys()
            .filter(|key| cid_lifecycle::is_relevant_key(key))
            .cloned()
            .collect();
        let reference =
            cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| top.get(key).cloned())
                .expect("build CID genesis reference");
        let stats = verify_imported_policy_state(0, top.iter(), &reference)
            .expect("real block zero state must pass every policy");
        assert_eq!(stats.scanned, top.len());
        assert!(stats.governance > 0);
        assert!(stats.fullnode_issuance > 0);
        assert!(stats.genesis_pallet > 0);
        assert!(stats.provincialbank_interest > 0);
        assert!(stats.cid > 0);
    }

    #[test]
    fn complete_state_rejects_missing_fixed_role_and_allows_dynamic_role_key() {
        let storage = full_runtime_genesis_storage();
        let top = storage.top;
        let cid_keys: Vec<Vec<u8>> = top
            .keys()
            .filter(|key| cid_lifecycle::is_relevant_key(key))
            .cloned()
            .collect();
        let reference =
            cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| top.get(key).cloned())
                .expect("build CID genesis reference");
        let institution = primitives::governance_skeleton::fixed_institutions()[0];
        let role = primitives::governance_skeleton::fixed_role_specs(institution.code)[0];

        let mut missing = top.clone();
        missing.remove(&governance_skeleton::storage_key::institution_role(
            institution.cid_number.as_bytes(),
            role.role_code,
        ));
        let err = verify_imported_policy_state(0, missing.iter(), &reference)
            .expect_err("missing fixed role must fail");
        assert!(err.starts_with("固定治理骨架:RoleMissing"));

        let mut extra = top;
        extra.insert(
            governance_skeleton::storage_key::institution_role(
                institution.cid_number.as_bytes(),
                b"EXTRA_ROLE",
            ),
            Vec::new(),
        );
        verify_imported_policy_state(0, extra.iter(), &reference)
            .expect("受保护机构的动态岗位由 runtime 治理，不得被误判为额外固定岗位");
    }

    #[test]
    fn complete_state_rejects_each_policy_before_inner_import() {
        let storage = full_runtime_genesis_storage();
        let top = storage.top;
        let cid_keys: Vec<Vec<u8>> = top
            .keys()
            .filter(|key| cid_lifecycle::is_relevant_key(key))
            .cloned()
            .collect();
        let reference =
            cid_lifecycle::GenesisReference::from_genesis(&cid_keys, |key| top.get(key).cloned())
                .expect("build CID genesis reference");

        for case in blockchain_harness::ImportedStateBadCaseKind::all() {
            let bad_state = imported_state_bad_case(*case, &top);
            let err = verify_imported_policy_state(0, bad_state.iter(), &reference)
                .expect_err(&format!("{} must fail", case.label()));
            assert!(
                err.starts_with(case.expected_error_prefix()),
                "{} must fail with prefix {}, actual {err}",
                case.label(),
                case.expected_error_prefix()
            );
        }
    }

    /// 按 harness 定义的坏样本矩阵构造完整导入态。
    ///
    /// 这里故意只放在 node 内部测试中：harness 负责枚举制度坏样本，node 测试负责
    /// 使用私有 storage key 精确改写真实创世态，避免生产守卫为了测试而扩大公开接口。
    fn imported_state_bad_case(
        case: blockchain_harness::ImportedStateBadCaseKind,
        top: &BTreeMap<Vec<u8>, Vec<u8>>,
    ) -> BTreeMap<Vec<u8>, Vec<u8>> {
        use codec::Encode;

        let mut bad_state = top.clone();
        match case {
            blockchain_harness::ImportedStateBadCaseKind::MissingGovernanceAdmin => {
                let fixed = primitives::governance_skeleton::fixed_institutions()[0];
                bad_state.remove(&governance_skeleton::storage_key::account_id(
                    fixed.cid_number.as_bytes(),
                ));
            }
            blockchain_harness::ImportedStateBadCaseKind::NonZeroFullnodeIssued => {
                bad_state.insert(
                    fullnode_issuance::storage_key::rewarded_block_count(),
                    1u32.encode(),
                );
            }
            blockchain_harness::ImportedStateBadCaseKind::UnknownCitizenIssuanceKey => {
                let mut unknown = citizen_issuance::storage_key::pallet_prefix().to_vec();
                unknown.extend_from_slice(b"UnknownGuardState");
                bad_state.insert(unknown, vec![1]);
            }
            blockchain_harness::ImportedStateBadCaseKind::ChangedGenesisCitizenMax => {
                bad_state.insert(
                    genesis_pallet::storage_key::citizen_max(),
                    1_443_497_379u64.encode(),
                );
            }
            blockchain_harness::ImportedStateBadCaseKind::MissingProvincialBankStake => {
                let first_bank = &primitives::cid::china::china_ch::CHINA_CH[0];
                bad_state.remove(&provincialbank_interest::storage_key::system_account(
                    &first_bank.stake_account,
                ));
            }
            blockchain_harness::ImportedStateBadCaseKind::UnknownProvincialBankStorage => {
                let mut unknown_interest_key =
                    provincialbank_interest::storage_key::pallet_prefix().to_vec();
                unknown_interest_key
                    .extend_from_slice(&sp_crypto_hashing::twox_128(b"ShadowInterest"));
                bad_state.insert(unknown_interest_key, 1u32.encode());
            }
        }
        bad_state
    }

    #[test]
    fn cid_policy_accepts_complete_state_import_at_non_genesis_height() {
        assert_eq!(cid_lifecycle::check_state_import_height(1), Ok(()));
    }
}
