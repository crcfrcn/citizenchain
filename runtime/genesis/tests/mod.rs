#![cfg(test)]

use super::*;
use frame_support::derive_impl;
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, BuildStorage};

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
    pub type GenesisPallet = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

frame_support::parameter_types! {
    pub const MaxDeclarationLen: u32 = 2048;
}

/// 测试用空 seeder:genesis 单测只碰非治理 storage(Phase/declarations),
/// 不触发机构 seeding,故给空实现即可,无需 mock 整套治理栈。
pub struct NoopSeeder;
impl GenesisInstitutionSeeder for NoopSeeder {
    fn seed() {}
}

impl pallet::Config for Test {
    type WeightInfo = ();
    type MaxDeclarationLen = MaxDeclarationLen;
    type InstitutionSeeder = NoopSeeder;
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    sp_io::TestExternalities::new(storage)
}

mod phase;
