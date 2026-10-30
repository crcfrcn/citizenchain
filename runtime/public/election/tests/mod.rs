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

mod unit;

mod contract;
