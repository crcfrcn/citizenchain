//! 构造节点守卫的测试创世身份、状态及链配置夹具。

use crate::guard::NodeGuard;

use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;

use std::sync::Arc;

// 临时链身份只存在于测试构建，由声明的创世 storage 独立计算。
// 不使用数据库自身的哈希/头作为期望值，且必须使用本次内置的准确 WASM。
#[derive(Clone, Copy)]
pub struct TestGenesisIdentity {
    pub(super) hash: sp_core::H256,
    pub(super) state_root: sp_core::H256,
}

impl TestGenesisIdentity {
    /// 测试也使用声明的创世输入；不把数据库当前状态作为期望基准。
    pub fn genesis_state(
        spec: &dyn sc_service::ChainSpec,
    ) -> Result<Arc<crate::guard::GenesisState>, String> {
        let storage = spec.as_storage_builder().build_storage()?;
        if storage
            .top
            .get(sp_core::storage::well_known_keys::CODE)
            .map(Vec::as_slice)
            != runtime::WASM_BINARY
        {
            return Err("测试创世必须使用本次内置的准确WASM".into());
        }
        Ok(Arc::new(crate::guard::GenesisState::from_storage(
            storage,
            runtime::VERSION.state_version(),
        )))
    }

    pub fn from_chain_spec(spec: &dyn sc_service::ChainSpec) -> Result<Self, String> {
        let storage = spec.as_storage_builder().build_storage()?;
        if storage
            .top
            .get(sp_core::storage::well_known_keys::CODE)
            .map(Vec::as_slice)
            != runtime::WASM_BINARY
        {
            return Err("测试创世必须使用本次内置的准确 WASM".into());
        }
        let state =
            crate::guard::GenesisState::from_storage(storage, runtime::VERSION.state_version());
        Ok(Self {
            hash: state.hash,
            state_root: state.state_root,
        })
    }
}

impl<I> NodeGuard<I> {
    // 测试仅替换声明的信任输入；启动保存诊断，导入前仍执行与生产相同的身份核验。
    pub fn new_for_test(
        inner: I,
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        identity: TestGenesisIdentity,
        genesis: Result<Arc<crate::guard::GenesisState>, String>,
    ) -> Self {
        Self::assemble(
            inner,
            client,
            backend,
            genesis,
            identity.hash,
            identity.state_root,
        )
    }
}
