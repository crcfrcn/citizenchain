//! 验证当前选举占位模块的真实元数据与空维护行为。

use super::*;
use frame_support::{derive_impl, traits::Hooks, weights::Weight};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;
    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type Election = super;
}
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
}
impl Config for Test {}

#[test]
fn placeholder_has_no_business_calls_events_or_storage() {
    let metadata = Test::metadata_ir();
    let pallet = metadata
        .pallets
        .iter()
        .find(|p| p.name == "Election")
        .expect("实际注册模块");
    assert!(pallet.calls.is_none());
    assert!(pallet.event.is_none());
    assert!(pallet.storage.is_none());
}

#[test]
fn default_hooks_have_no_weight_or_storage_effect() {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("测试存储");
    sp_io::TestExternalities::new(storage).execute_with(|| {
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert_eq!(<Election as Hooks<u64>>::on_initialize(1), Weight::zero());
        <Election as Hooks<u64>>::on_finalize(1);
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}
