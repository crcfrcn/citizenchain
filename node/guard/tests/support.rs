//! 仅开发测试目标挂载的原有夹具，不进入正式构建。

use crate::guard::verify_genesis_identity_against;
use crate::guard::NodeGuard;

use nodeclient::types::FullBackend;
use nodeclient::types::FullClient;
use runtime::opaque::Block;

use sp_blockchain::HeaderBackend;

use sp_runtime::traits::Block as BlockT;
use sp_runtime::traits::Header as HeaderT;

use std::sync::Arc;

// 临时链身份只存在于测试构建，由声明的创世 storage 独立计算。
// 不使用数据库自身的哈希/头作为期望值，且必须使用本次内置的准确 WASM。
#[derive(Clone, Copy)]
pub struct TestGenesisIdentity {
    pub(super) hash: sp_core::H256,
    pub(super) state_root: sp_core::H256,
}

impl TestGenesisIdentity {
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
        let state_version = runtime::VERSION.state_version();
        let mut changes = vec![(
            None,
            storage
                .top
                .into_iter()
                .map(|(key, value)| (key, Some(value)))
                .collect::<Vec<_>>(),
        )];
        changes.extend(storage.children_default.into_values().map(|child| {
            (
                Some(child.child_info),
                child
                    .data
                    .into_iter()
                    .map(|(key, value)| (key, Some(value)))
                    .collect::<Vec<_>>(),
            )
        }));
        let mut state = sp_state_machine::new_in_mem::<sp_runtime::traits::BlakeTwo256>();
        state.insert(changes, state_version);
        let state_root = *state.root();
        let block = sc_service::construct_genesis_block::<Block>(state_root, state_version);
        Ok(Self {
            hash: block.header().hash(),
            state_root,
        })
    }
}

impl<I> NodeGuard<I> {
    // 仅测试入口可提供声明的临时链身份；身份核验与生产使用同一完整拒绝路径。
    pub fn new_for_test(
        inner: I,
        client: Arc<FullClient>,
        backend: Arc<FullBackend>,
        identity: TestGenesisIdentity,
    ) -> Self {
        verify_genesis_identity_against(client.as_ref(), identity.hash, identity.state_root)
            .unwrap_or_else(|reason| panic!("测试 NodeGuard 拒绝启动：{reason}"));
        Self::assemble(inner, client, backend)
    }
}
