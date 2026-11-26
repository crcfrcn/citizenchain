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

fn constitution_fixture() -> &'static (ImmutableReference, BTreeMap<Vec<u8>, Vec<u8>>) {
    static FIXTURE: std::sync::OnceLock<(ImmutableReference, BTreeMap<Vec<u8>, Vec<u8>>)> =
        std::sync::OnceLock::new();
    FIXTURE.get_or_init(|| {
        use sc_service::ChainSpec as _;
        let spec = node::chain::ChainSpec::from_json_bytes(
            include_bytes!("../../citizenchain.json").to_vec(),
        )
        .unwrap();
        let storage = spec.as_storage_builder().build_storage().unwrap();
        let executor = sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::default();
        let version = sc_service::resolve_state_version_from_wasm::<
            _,
            sp_runtime::traits::BlakeTwo256,
        >(&storage, &executor)
        .unwrap();
        let genesis = crate::guard::GenesisState::new(storage, version).unwrap();
        let prefix = crate::constitution::storage_key::pallet_prefix();
        let rows = genesis
            .baseline_entries()
            .into_iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .collect();
        let reference =
            ConstitutionGuard::<CountingImport>::build_reference(Ok(Arc::new(genesis))).unwrap();
        (reference, rows)
    })
}

fn legislation_layout(name: &str) -> RuntimeStorage {
    use codec::Encode as _;
    let bytes = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = nodeclient::codec::decode_runtime_metadata(bytes.as_slice()).unwrap();
    let frame_metadata::RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("需要V16元数据")
    };
    let pallet = value.pallets.iter_mut().find(|p| p.index == 25).unwrap();
    pallet.name = name.into();
    pallet.storage.as_mut().unwrap().prefix = name.into();
    RuntimeStorage::from_metadata(
        &frame_metadata::RuntimeMetadataPrefixed(0x6174_656d, metadata).encode(),
    )
    .unwrap()
}

fn physical_constitution(layout: &RuntimeStorage) -> BTreeMap<Vec<u8>, Vec<u8>> {
    constitution_fixture()
        .1
        .iter()
        .map(|(key, value)| (layout.key(key, true), value.clone()))
        .collect()
}

#[test]
fn code_upgrade_checks_constitution_using_the_writer_metadata() {
    // 任意实际metadata名称均可定位；不存在新名称下的值时，不能因此把旧合法状态当作缺失。
    for name in ["LegislationYuan", "ExecutedLaws", "Legislation"] {
        let layout = legislation_layout(name);
        let state = physical_constitution(&layout);
        let untouched = state.clone();
        let raw = BTreeMap::from([(sp_storage::well_known_keys::CODE.to_vec(), Some(vec![1]))]);
        assert_eq!(
            check_transition(&constitution_fixture().0, &layout, &layout, &raw, |key| Ok(
                state.get(key).cloned()
            )),
            Ok(()),
            "{name}"
        );
        assert_eq!(state, untouched);
        assert_eq!(
            check_imported_constitution(
                &constitution_fixture().0,
                &layout,
                sp_core::H256::repeat_byte(1),
                &state
            ),
            Ok(())
        );
    }
}

#[test]
fn constitution_prefix_move_requires_same_frozen_records_and_deleted_sources() {
    let before = legislation_layout("LegislationYuan");
    let after = legislation_layout("Legislation");
    let state = physical_constitution(&before);
    let mut moved = BTreeMap::new();
    for (logical, value) in &constitution_fixture().1 {
        moved.insert(before.key(logical, true), None);
        moved.insert(after.key(logical, true), Some(value.clone()));
    }
    let verify = |raw: &BTreeMap<Vec<u8>, Option<Vec<u8>>>| {
        check_transition(&constitution_fixture().0, &before, &after, raw, |key| {
            Ok(state.get(key).cloned())
        })
    };
    assert_eq!(verify(&moved), Ok(()));
    assert!(
        verify(&BTreeMap::new()).is_err(),
        "不能仅换metadata而不搬迁真实宪法"
    );
    let law = crate::constitution::storage_key::law(CONSTITUTION_LAW_ID);
    let mut retained_source = moved.clone();
    retained_source.remove(&before.key(&law, true));
    assert!(verify(&retained_source).is_err());
    for logical in [
        crate::constitution::storage_key::manifest(),
        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
    ] {
        let mut tampered = moved.clone();
        tampered.insert(after.key(&logical, true), Some(vec![0]));
        assert!(verify(&tampered).is_err(), "搬迁不能掩盖manifest或历史篡改");
    }
    let mut deleted = moved;
    deleted.insert(after.key(&law, true), None);
    assert!(verify(&deleted).is_err());
}

#[test]
fn old_namespace_tampering_and_wrong_namespace_rows_remain_rejected() {
    let layout = legislation_layout("LegislationYuan");
    let state = physical_constitution(&layout);
    for logical in [
        crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
        crate::constitution::storage_key::manifest(),
        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
        crate::constitution::storage_key::constitution_guard_vote_proof(1),
    ] {
        let raw = BTreeMap::from([(layout.key(&logical, true), Some(vec![0]))]);
        assert!(
            check_transition(&constitution_fixture().0, &layout, &layout, &raw, |key| Ok(
                state.get(key).cloned()
            ))
            .is_err()
        );
    }
    let law = crate::constitution::storage_key::law(CONSTITUTION_LAW_ID);
    let wrong = BTreeMap::from([(law.clone(), state[&layout.key(&law, true)].clone())]);
    assert!(check_imported_constitution(
        &constitution_fixture().0,
        &layout,
        sp_core::H256::repeat_byte(1),
        &wrong
    )
    .is_err());
    let mut collision = state.clone();
    collision.extend(wrong);
    assert!(check_imported_constitution(
        &constitution_fixture().0,
        &layout,
        sp_core::H256::repeat_byte(1),
        &collision
    )
    .is_err());
    let raw = BTreeMap::from([(sp_storage::well_known_keys::CODE.to_vec(), Some(vec![1]))]);
    assert_eq!(
        check_transition(&constitution_fixture().0, &layout, &layout, &raw, |_| Err(
            "实际存储读取失败".into()
        )),
        Err("实际存储读取失败".into())
    );
}
