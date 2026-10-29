//! 阶段、开发者开关与创世常量存储测试。
use super::*;
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
