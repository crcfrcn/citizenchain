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

#[test]
fn default_phase_is_genesis() {
    new_test_ext().execute_with(|| {
        assert_eq!(GenesisPallet::phase(), pallet::ChainPhase::Genesis);
    });
}

#[test]
fn default_developer_upgrade_enabled() {
    new_test_ext().execute_with(|| {
        assert!(GenesisPallet::developer_upgrade_enabled());
    });
}

#[test]
fn developer_upgrade_check_trait_reads_storage() {
    new_test_ext().execute_with(|| {
        assert!(<GenesisPallet as DeveloperUpgradeCheck>::is_enabled());

        pallet::DeveloperUpgradeEnabled::<Test>::put(false);
        assert!(!<GenesisPallet as DeveloperUpgradeCheck>::is_enabled());
    });
}

#[test]
fn storage_can_be_switched_to_operation() {
    new_test_ext().execute_with(|| {
        // 模拟 on_runtime_upgrade 迁移写入
        pallet::Phase::<Test>::put(pallet::ChainPhase::Operation);
        pallet::DeveloperUpgradeEnabled::<Test>::put(false);

        assert_eq!(GenesisPallet::phase(), pallet::ChainPhase::Operation);
        assert!(!GenesisPallet::developer_upgrade_enabled());
        assert!(!<GenesisPallet as DeveloperUpgradeCheck>::is_enabled());
    });
}

#[test]
fn genesis_config_initializes_declarations() {
    let citizens = "创世宣言测试".as_bytes().to_vec();
    let country = "公民宣言测试".as_bytes().to_vec();
    let citizen_max = 1_443_497_378u64;

    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");

    pallet::GenesisConfig::<Test> {
        citizens_declaration: citizens.clone(),
        country_declaration: country.clone(),
        citizen_max,
        _phantom: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .expect("genesis config should assimilate");

    sp_io::TestExternalities::new(storage).execute_with(|| {
        assert_eq!(GenesisPallet::citizens_declaration().to_vec(), citizens);
        assert_eq!(GenesisPallet::country_declaration().to_vec(), country);
        assert_eq!(GenesisPallet::citizen_max(), citizen_max);
    });
}
