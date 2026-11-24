//! 验证四类宪法篡改被拒绝且不阻断后续合法区块导入。

use super::*;

use runtime::opaque::Block;
use sc_consensus::BlockCheckParams;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ImportResult;
use sp_consensus::Error as ConsensusError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

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

fn import_params(number: u32) -> BlockImportParams<Block> {
    use sp_consensus::BlockOrigin;
    use sp_core::H256;
    use sp_runtime::Digest;

    let header = runtime::opaque::Header::new(
        number,
        H256::repeat_byte(1),
        H256::repeat_byte(2),
        H256::repeat_byte(3),
        Digest::default(),
    );
    BlockImportParams::new(BlockOrigin::NetworkInitialSync, header)
}

#[test]
fn four_constitution_tamper_classes_return_known_bad_without_stopping_next_import() {
    let inner = crate::constitution::guard::tests::CountingImport::default();
    for (number, reason) in [
        (1, "不可修改条款被篡改"),
        (2, "manifest 被篡改"),
        (3, "历史版本被篡改"),
        (4, "修宪凭据被篡改"),
    ] {
        let result = futures::executor::block_on(crate::guard::import_if_verified(
            &inner,
            import_params(number),
            Err(reason.into()),
        ))
        .expect("非法宪法区块应返回导入结果而不是终止节点");
        assert_eq!(result, ImportResult::KnownBad);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 0);
    }

    // 闸门无跨块污染：连续拒绝后，下一合法区块仍可正常委派给旧 runtime 的内层导入器。
    let accepted = futures::executor::block_on(crate::guard::import_if_verified(
        &inner,
        import_params(5),
        Ok(()),
    ))
    .expect("合法区块应在拒绝后继续导入");
    assert_eq!(accepted, ImportResult::AlreadyInChain);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);
}

#[test]
fn genesis_input_errors_are_retained_as_diagnostics() {
    let error =
        ConstitutionGuard::<CountingImport>::build_reference(Err("固定输入验证失败".into()))
            .err()
            .unwrap();
    assert_eq!(error, "固定输入验证失败");
}

#[test]
fn frozen_chainspec_builds_verified_constitution_reference_without_block_zero_database() {
    use sc_service::ChainSpec;
    let spec =
        node::chain::ChainSpec::from_json_bytes(include_bytes!("../../citizenchain.json").to_vec())
            .unwrap();
    let mut storage = spec.as_storage_builder().build_storage().unwrap();
    let executor = sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::default();
    let version =
        sc_service::resolve_state_version_from_wasm::<_, sp_runtime::traits::BlakeTwo256>(
            &storage, &executor,
        )
        .unwrap();
    let genesis = crate::guard::GenesisState::new(storage.clone(), version).unwrap();
    assert!(genesis.is_frozen());
    let current_law = crate::constitution::storage_key::law(CONSTITUTION_LAW_ID);
    assert!(
        genesis.read(&current_law).is_none(),
        "冻结输入不能伪装成迁移后的状态"
    );
    assert!(
        genesis.read_baseline(&current_law).is_some(),
        "固定原始前缀必须能构造基准"
    );
    assert!(
        ConstitutionGuard::<CountingImport>::build_reference(Ok(std::sync::Arc::new(genesis)))
            .is_ok()
    );
    storage.top.insert(b"tampered-genesis".to_vec(), vec![1]);
    assert!(
        crate::guard::GenesisState::new(storage, version).is_err(),
        "改动输入不能冒用冻结信任锚"
    );
}
