//! 在区块导入时核对宪法变更及受保护条款，拒绝违规状态。

use super::*;

use crate::constitution::check_frozen_constitution_records;
use crate::constitution::check_immutable_articles;
use crate::constitution::check_immutable_version;
use crate::constitution::check_imported_state_immutable;
use crate::constitution::check_version_key_range;
use crate::constitution::declared_constitution_versions;
use crate::constitution::decode_law_head;
use crate::constitution::needs_full_invariant_check;
use crate::constitution::verify_manifest_from_reader;
use crate::constitution::ImmutableReference;
use crate::constitution::CONSTITUTION_LAW_ID;
use nodeclient::codec::RuntimeStorage;
use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;
use runtime::opaque::Block;
use sc_consensus::BlockCheckParams;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ImportResult;
use sc_consensus::ImportedState;
use sp_api::Metadata as _;
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 宪法判定只消费同一已执行区块的逻辑视图；物理定位和真实搬迁由共有接口解释。
/// 原始delta、父状态及全部永久断言保持，不把set_code所在块误作已完成名称迁移。
pub(crate) fn check_transition(
    reference: &ImmutableReference,
    before: &RuntimeStorage,
    after: &RuntimeStorage,
    raw: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    read_parent: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    let delta = before.transition_changes(after, raw, &read_parent)?;
    let raw_post = |key: &[u8]| match raw.get(key) {
        Some(value) => Ok(value.clone()),
        None => read_parent(key),
    };
    let prefix = crate::constitution::storage_key::pallet_prefix();
    let moved = before.key(&prefix, true) != after.key(&prefix, true);
    if moved {
        for key in raw.keys() {
            let logical = before.logical_key(key)?;
            if logical.starts_with(&prefix) {
                let source = before.key(&logical, true);
                if read_parent(&source)?.is_some() && raw_post(&source)?.is_some() {
                    return Err("宪法搬迁后仍保留已存在的来源记录".into());
                }
            }
        }
    }
    if !moved && !needs_full_invariant_check(&delta) {
        return Ok(());
    }
    let parent = |key: &[u8]| before.read(key, &read_parent);
    check_frozen_constitution_records(&delta, &parent)?;
    // 永久规则的Option接口只表达缺失；读取错误另行保留，不能被当成合法空值。
    let read_error = std::cell::RefCell::new(None);
    let post = |key: &[u8]| match after.read(key, &raw_post) {
        Ok(value) => value,
        Err(reason) => {
            read_error.borrow_mut().get_or_insert(reason);
            None
        }
    };
    let verdict = (|| {
        let law_bytes = post(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
            .ok_or_else(|| "目标状态缺宪法 Law(0)".to_string())?;
        let law = decode_law_head(&law_bytes).map_err(|e| format!("宪法 Law 解码失败:{e:?}"))?;
        check_version_key_range(
            delta
                .iter()
                .filter_map(|(key, value)| value.as_ref().map(|_| key)),
            law.latest_version,
        )
        .map_err(|e| format!("宪法版本键范围非法:{e:?}"))?;
        let parent_law_bytes = parent(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))?
            .ok_or_else(|| "父状态缺宪法 Law(0)".to_string())?;
        let parent_law = decode_law_head(&parent_law_bytes)
            .map_err(|e| format!("父状态宪法 Law 解码失败:{e:?}"))?;
        if law.latest_version < parent_law.latest_version {
            return Err(format!(
                "宪法 latest_version 从 {} 回退到 {}",
                parent_law.latest_version, law.latest_version
            ));
        }
        check_immutable_articles(&post, reference)
            .map_err(|reason| format!("宪法不变式被破坏:{reason:?}"))?;
        for key in delta.keys() {
            if let Some(version) =
                crate::constitution::storage_key::constitution_version_from_key(key).or_else(|| {
                    crate::constitution::storage_key::constitution_proof_version_from_key(key)
                })
            {
                check_immutable_version(&post, reference, version)
                    .map_err(|reason| format!("宪法历史版本 {version} 非法:{reason:?}"))?;
            }
        }
        Ok(())
    })();
    match read_error.into_inner() {
        Some(reason) => Err(reason),
        None => verdict,
    }
}

/// 完整导入仍只承认主trie；同一逻辑宪法键的重复来源或非所属前缀不能伪装为有效状态。
pub(crate) fn check_imported_constitution(
    reference: &ImmutableReference,
    layout: &RuntimeStorage,
    block: <Block as BlockT>::Hash,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<(), String> {
    let prefix = crate::constitution::storage_key::pallet_prefix();
    let mut logical = BTreeMap::new();
    for (key, value) in rows {
        let mapped = layout.logical_key(key)?;
        if !mapped.starts_with(&prefix) {
            continue;
        }
        if layout.key(&mapped, true) != *key || logical.insert(mapped, value.clone()).is_some() {
            return Err("完整状态宪法存储键归属冲突".into());
        }
    }
    // 仅把主trie逻辑键交给原判定；此视图不提交、不计算物理根，原导入材料保持原样。
    let view = ImportedState {
        block,
        state: sc_client_api::KeyValueStates::from([(
            Vec::new(),
            (logical.into_iter().collect::<Vec<_>>(), Vec::new()),
        )]),
    };
    check_imported_state_immutable(&view, reference)
}

/// 在区块进入规范链之前校验宪法四类永久记录；合法返回 `Ok`，非法统一拒块。
pub struct ConstitutionGuard<I> {
    inner: I,
    client: Arc<FullClient>,
    backend: Arc<FullBackend>,
    reference: Result<ImmutableReference, String>,
}

impl<I> ConstitutionGuard<I> {
    /// 自检失败保留诊断，仍装配导入器；基准不能验证时严格拒绝入链。
    pub fn new(
        inner: I,
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        genesis: Result<Arc<crate::guard::GenesisState>, String>,
    ) -> Self {
        let reference = Self::build_reference(genesis);
        if let Err(reason) = &reference {
            log::error!(target: "constitution-guard", "宪法守卫启动自检诊断，服务继续运行，入链校验保持严格:{reason}");
        }
        Self {
            inner,
            client,
            backend,
            reference,
        }
    }

    pub(crate) fn build_reference(
        genesis: Result<Arc<crate::guard::GenesisState>, String>,
    ) -> Result<ImmutableReference, String> {
        let state = genesis?;
        let read = |key: &[u8]| state.read_baseline(key);
        let reference = ImmutableReference::from_raw_reader(&read)
            .map_err(|error| format!("宪法创世基准无效:{error:?}"))?;
        verify_manifest_from_reader(&read, &reference)
            .map_err(|error| format!("宪法manifest交叉校验失败:{error}"))?;
        check_immutable_articles(&read, &reference)
            .map_err(|error| format!("创世宪法不变式校验失败:{error:?}"))?;
        let law = decode_law_head(
            &read(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
                .ok_or("创世输入缺少宪法Law(0)")?,
        )
        .map_err(|error| format!("创世Law解码失败:{error:?}"))?;
        let keys = state.baseline_entries().into_keys().collect::<Vec<_>>();
        let versions = declared_constitution_versions(keys.iter(), law.latest_version)
            .map_err(|error| format!("创世宪法版本集合非法:{error:?}"))?;
        for version in versions {
            check_immutable_version(&read, &reference, version)
                .map_err(|error| format!("创世宪法历史版本{version}非法:{error:?}"))?;
        }
        Ok(reference)
    }

    /// 提交前校验 warp/状态导入携带的完整下载态。
    fn verify_imported_state(&self, params: &BlockImportParams<Block>) -> Result<(), String> {
        let imported = match &params.state_action {
            StateAction::ApplyChanges(StorageChanges::Import(imported)) => imported,
            _ => return Err("warp 状态非 ApplyChanges(Import) 形态,无法提交前校验".into()),
        };
        let mut rows = BTreeMap::new();
        for (key, value) in imported
            .state
            .0
            .iter()
            .filter(|level| level.parent_storage_keys.is_empty() && level.state_root.is_empty())
            .flat_map(|level| level.key_values.iter())
        {
            if rows.insert(key.clone(), value.clone()).is_some() {
                return Err("完整状态包含重复主存储键".into());
            }
        }
        let writer = if *params.header.number() == 0 {
            params.post_hash()
        } else {
            *params.header.parent_hash()
        };
        let metadata = self
            .client
            .runtime_api()
            .metadata(writer)
            .map_err(|e| format!("宪法状态写入Runtime元数据不可读:{e}"))?;
        let layout = RuntimeStorage::from_metadata(metadata.as_slice())?
            .with_state(|key| Ok(rows.get(key).cloned()))?;
        check_imported_constitution(
            self.reference.as_ref().map_err(Clone::clone)?,
            &layout,
            imported.block,
            &rows,
        )
    }

    /// 读取父状态并把 delta 覆盖为目标后置状态，再执行全部宪法不变式。
    fn check_delta(
        &self,
        parent_hash: <Block as BlockT>::Hash,
        delta: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ) -> Result<(), String> {
        let read_parent = |key: &[u8]| {
            self.client
                .storage(parent_hash, &StorageKey(key.to_vec()))
                .map(|value| value.map(|data| data.0))
                .map_err(|error| format!("读取父状态宪法失败:{error}"))
        };
        let read_post = |key: &[u8]| match delta.get(key) {
            Some(value) => Ok(value.clone()),
            None => read_parent(key),
        };
        // 父状态归实际写入者，本块后置状态归执行父块代码；不能用set_code后的新metadata猜前缀。
        let before =
            RuntimeStorage::at_state(&self.client, parent_hash)?.with_state(&read_parent)?;
        let metadata = self
            .client
            .runtime_api()
            .metadata(parent_hash)
            .map_err(|e| format!("执行Runtime宪法元数据不可读:{e}"))?;
        let after = RuntimeStorage::from_metadata(metadata.as_slice())?.with_state(read_post)?;
        check_transition(
            self.reference.as_ref().map_err(Clone::clone)?,
            &before,
            &after,
            &delta,
            read_parent,
        )
    }

    /// 对普通导入形态取得可验证的后置 storage delta；`Ok` 为合法，任何 `Err` 都拒块。
    fn verify_block(&self, params: &BlockImportParams<Block>) -> Result<(), String> {
        self.reference.as_ref().map_err(Clone::clone)?;
        let parent_hash = *params.header.parent_hash();
        if let Some(body) = &params.body {
            let block = Block::new(params.header.clone(), body.clone());
            let api = self.client.runtime_api();
            api.execute_block(parent_hash, block.into())
                .map_err(|e| format!("只读执行区块失败:{e}"))?;
            let parent_state = self
                .backend
                .state_at(parent_hash, TrieCacheContext::Untrusted)
                .map_err(|e| format!("取父状态失败:{e}"))?;
            let changes = api
                .into_storage_changes(&parent_state, parent_hash)
                .map_err(|e| format!("提取存储变更失败:{e}"))?;
            let delta = changes.main_storage_changes.into_iter().collect();
            return self.check_delta(parent_hash, delta);
        }

        match &params.state_action {
            // 预计算状态变化即使没有 body 也必须经过宪法校验，不能走旧快路径绕过。
            StateAction::ApplyChanges(StorageChanges::Changes(changes)) => {
                let delta = changes.main_storage_changes.iter().cloned().collect();
                self.check_delta(parent_hash, delta)
            }
            // Skip 明确定义为不执行且不导入状态，因此不可能在本次导入中改写宪法状态。
            StateAction::Skip => Ok(()),
            StateAction::Execute | StateAction::ExecuteIfPossible => {
                Err("执行型区块缺少 body,无法独立证明宪法后置状态".into())
            }
            StateAction::ApplyChanges(StorageChanges::Import(_)) => {
                Err("完整状态导入必须走 verify_imported_state".into())
            }
        }
    }
}

#[async_trait::async_trait]
impl<I> BlockImport<Block> for ConstitutionGuard<I>
where
    I: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
    type Error = ConsensusError;

    async fn check_block(
        &self,
        block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        if self.reference.is_err() {
            return Ok(ImportResult::KnownBad);
        }
        self.inner.check_block(block).await
    }

    async fn import_block(
        &self,
        params: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        if let Err(reason) = &self.reference {
            log::error!(target: "constitution-guard", "拒绝区块导入，创世基准不可验证:{reason}");
            return Ok(ImportResult::KnownBad);
        }
        if params.with_state() {
            let verdict = self.verify_imported_state(&params);
            if let Err(reason) = &verdict {
                log::error!(
                    target: "constitution-guard",
                    "拒绝 warp/状态导入 ({:?}):宪法不变式校验未通过 —— {reason}",
                    params.post_hash(),
                );
            }
            return crate::guard::import_if_verified(&self.inner, params, verdict).await;
        }

        let verdict = self.verify_block(&params);
        if let Err(reason) = &verdict {
            log::error!(
                target: "constitution-guard",
                "宪法区块非法，返回 KnownBad 并保留父状态 ({:?}):{reason}",
                params.post_hash(),
            );
        }
        crate::guard::import_if_verified(&self.inner, params, verdict).await
    }
}
#[cfg(test)]
#[path = "../tests/amendment.rs"]
mod tests;
