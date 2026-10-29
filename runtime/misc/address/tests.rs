//! 地址版本、授权上下文、真实存储和输入边界的单元测试。

use super::*;
use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32};
use sp_runtime::{BuildStorage, DispatchError};

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
    pub type Address = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
}

pub struct Authority;
impl AddressUpdateAuthority<u64> for Authority {
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority(action_code: u32) -> Option<crate::benchmarks::Authority<u64>> {
        if action_code > 4 {
            return None;
        }
        // 与本夹具现有权限检查完全一致，基准不能扩大主体或辖区。
        Some(crate::benchmarks::Authority {
            caller: 1,
            actor_cid_number: b"registrar".to_vec().try_into().ok()?,
            actor_role_code: b"REG".to_vec().try_into().ok()?,
            province_code: b"GD".to_vec(),
            city_code: b"001".to_vec(),
            town_code: b"002".to_vec(),
        })
    }

    fn can_update_catalog(who: &u64, cid: &[u8], role: &[u8], action: u32) -> bool {
        *who == 1 && cid == b"registrar" && role == b"REG" && action == 0
    }
    fn can_update_address(
        who: &u64,
        cid: &[u8],
        role: &[u8],
        province: &[u8],
        city: &[u8],
        action: u32,
    ) -> bool {
        *who == 1
            && cid == b"registrar"
            && role == b"REG"
            && province == b"GD"
            && city == b"001"
            && (1..=4).contains(&action)
    }
}
impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AddressAuthority = Authority;
    type MaxCodeLen = ConstU32<3>;
    type MaxVersionLen = ConstU32<8>;
    type MaxAddressNameCodeLen = ConstU32<3>;
    type MaxAddressLocalNoLen = ConstU32<4>;
    type MaxAddressNameLen = ConstU32<16>;
    type MaxAddressDetailLen = ConstU32<16>;
}
fn ext() -> sp_io::TestExternalities {
    let mut ext = sp_io::TestExternalities::new(
        frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("测试创世存储"),
    );
    ext.execute_with(|| System::set_block_number(1));
    ext
}

// 每个真实基准只从根目录执行一次；所有权限、存储、事件断言归原基准实现。
#[cfg(feature = "runtime-benchmarks")]
mod benchmark_tests {
    use super::*;

    fn run_case(name: &[u8]) {
        ext().execute_with(|| {
            crate::benchmarks::test_case::<Test>(name).expect("真实地址基准必须通过");
        });
    }

    #[test]
    fn set_catalog_version() {
        run_case(b"set_catalog_version");
    }

    #[test]
    fn set_address_name() {
        run_case(b"set_address_name");
    }

    #[test]
    fn remove_address_name() {
        run_case(b"remove_address_name");
    }

    #[test]
    fn set_address() {
        run_case(b"set_address");
    }

    #[test]
    fn remove_address() {
        run_case(b"remove_address");
    }
}

fn actor() -> ActorCidNumber {
    b"registrar".to_vec().try_into().expect("测试CID")
}
fn role() -> ActorRoleCode {
    b"REG".to_vec().try_into().expect("测试岗位")
}
fn name_key() -> pallet::AddressNameKeyOf<Test> {
    (
        b"GD".to_vec().try_into().unwrap(),
        b"001".to_vec().try_into().unwrap(),
        b"002".to_vec().try_into().unwrap(),
        b"101".to_vec().try_into().unwrap(),
    )
}
fn address_key() -> pallet::AddressKeyOf<Test> {
    let (p, c, t, n) = name_key();
    (
        p,
        c,
        t,
        n,
        b"0001".to_vec().try_into().unwrap(),
        b"house".to_vec().try_into().unwrap(),
    )
}
fn set_name(who: u64, code: Vec<u8>, name: Vec<u8>) -> frame_support::dispatch::DispatchResult {
    Address::set_address_name(
        RuntimeOrigin::signed(who),
        actor(),
        role(),
        b"GD".to_vec(),
        b"001".to_vec(),
        b"002".to_vec(),
        code,
        name,
    )
}
fn set_address(
    who: u64,
    local: Vec<u8>,
    detail: Vec<u8>,
) -> frame_support::dispatch::DispatchResult {
    Address::set_address(
        RuntimeOrigin::signed(who),
        actor(),
        role(),
        b"GD".to_vec(),
        b"001".to_vec(),
        b"002".to_vec(),
        b"101".to_vec(),
        local,
        detail,
    )
}

#[test]
fn catalog_writes_version_hash_and_event() {
    ext().execute_with(|| {
        assert!(pallet::CatalogVersion::<Test>::get().is_none());
        let hash = <Test as frame_system::Config>::Hash::repeat_byte(9);
        assert_ok!(Address::set_catalog_version(
            RuntimeOrigin::signed(1),
            actor(),
            role(),
            b"version1".to_vec(),
            hash
        ));
        assert_eq!(
            pallet::CatalogVersion::<Test>::get().unwrap().as_slice(),
            b"version1"
        );
        assert_eq!(pallet::CatalogHash::<Test>::get(), Some(hash));
        System::assert_last_event(
            Event::<Test>::CatalogVersionSet {
                actor_cid_number: actor(),
                catalog_version: b"version1".to_vec().try_into().unwrap(),
                catalog_hash: hash,
            }
            .into(),
        );
    });
}

#[test]
fn name_updates_increment_version_and_remove_hash() {
    ext().execute_with(|| {
        assert_ok!(set_name(1, b"101".to_vec(), b"first".to_vec()));
        let first = pallet::AddressNameHashes::<Test>::get(name_key()).unwrap();
        assert_ok!(set_name(1, b"101".to_vec(), b"second".to_vec()));
        assert_eq!(pallet::AddressNameVersions::<Test>::get(name_key()), 2);
        assert_ne!(
            pallet::AddressNameHashes::<Test>::get(name_key()),
            Some(first)
        );
        assert_ok!(Address::remove_address_name(
            RuntimeOrigin::signed(1),
            actor(),
            role(),
            b"GD".to_vec(),
            b"001".to_vec(),
            b"002".to_vec(),
            b"101".to_vec()
        ));
        assert_eq!(pallet::AddressNameVersions::<Test>::get(name_key()), 3);
        assert!(pallet::AddressNameHashes::<Test>::get(name_key()).is_none());
        assert!(matches!(
            System::events().last().unwrap().event,
            RuntimeEvent::Address(Event::AddressNameRemoved {
                address_version: 3,
                ..
            })
        ));
    });
}

#[test]
fn complete_address_updates_and_removes_real_key() {
    ext().execute_with(|| {
        assert_ok!(set_address(1, b"0001".to_vec(), b"house".to_vec()));
        assert_eq!(pallet::AddressVersions::<Test>::get(address_key()), 1);
        assert!(pallet::AddressHashes::<Test>::get(address_key()).is_some());
        assert_ok!(Address::remove_address(
            RuntimeOrigin::signed(1),
            actor(),
            role(),
            b"GD".to_vec(),
            b"001".to_vec(),
            b"002".to_vec(),
            b"101".to_vec(),
            b"0001".to_vec(),
            b"house".to_vec()
        ));
        assert_eq!(pallet::AddressVersions::<Test>::get(address_key()), 2);
        assert!(pallet::AddressHashes::<Test>::get(address_key()).is_none());
        assert!(matches!(
            System::events().last().unwrap().event,
            RuntimeEvent::Address(Event::AddressRemoved {
                address_version: 2,
                ..
            })
        ));
    });
}

#[test]
fn all_five_calls_reject_unauthorized_origin_without_writes() {
    ext().execute_with(|| {
        assert_noop!(
            Address::set_catalog_version(
                RuntimeOrigin::signed(2),
                actor(),
                role(),
                b"v1".to_vec(),
                Default::default()
            ),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            set_name(2, b"101".to_vec(), b"name".to_vec()),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            set_address(2, b"0001".to_vec(), b"house".to_vec()),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            Address::remove_address_name(
                RuntimeOrigin::signed(2),
                actor(),
                role(),
                b"GD".to_vec(),
                b"001".to_vec(),
                b"002".to_vec(),
                b"101".to_vec()
            ),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            Address::remove_address(
                RuntimeOrigin::signed(2),
                actor(),
                role(),
                b"GD".to_vec(),
                b"001".to_vec(),
                b"002".to_vec(),
                b"101".to_vec(),
                b"0001".to_vec(),
                b"house".to_vec()
            ),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            Address::set_catalog_version(
                RuntimeOrigin::root(),
                actor(),
                role(),
                b"v1".to_vec(),
                Default::default()
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn authority_receives_actor_role_and_geographic_scope() {
    ext().execute_with(|| {
        assert_noop!(
            Address::set_address_name(
                RuntimeOrigin::signed(1),
                actor(),
                role(),
                b"AH".to_vec(),
                b"001".to_vec(),
                b"002".to_vec(),
                b"101".to_vec(),
                b"name".to_vec()
            ),
            Error::<Test>::Unauthorized
        );
        assert_noop!(
            Address::set_catalog_version(
                RuntimeOrigin::signed(1),
                actor(),
                b"OTHER".to_vec().try_into().unwrap(),
                b"v1".to_vec(),
                Default::default()
            ),
            Error::<Test>::Unauthorized
        );
    });
}

#[test]
fn catalog_and_name_lengths_accept_exact_limit_and_reject_overflow() {
    ext().execute_with(|| {
        assert_noop!(
            Address::set_catalog_version(
                RuntimeOrigin::signed(1),
                actor(),
                role(),
                vec![],
                Default::default()
            ),
            Error::<Test>::EmptyCatalogVersion
        );
        assert_noop!(
            Address::set_catalog_version(
                RuntimeOrigin::signed(1),
                actor(),
                role(),
                vec![b'x'; 9],
                Default::default()
            ),
            Error::<Test>::FieldTooLong
        );
        assert_ok!(Address::set_catalog_version(
            RuntimeOrigin::signed(1),
            actor(),
            role(),
            vec![b'x'; 8],
            Default::default()
        ));
        assert_noop!(
            set_name(1, b"101".to_vec(), vec![]),
            Error::<Test>::EmptyAddressName
        );
        assert_noop!(
            set_name(1, b"101".to_vec(), vec![b'x'; 17]),
            Error::<Test>::FieldTooLong
        );
        assert_ok!(set_name(1, b"101".to_vec(), vec![b'x'; 16]));
    });
}

#[test]
fn code_and_local_number_boundaries_preserve_storage_on_failure() {
    ext().execute_with(|| {
        for code in [
            b"000".to_vec(),
            b"01".to_vec(),
            b"0001".to_vec(),
            b"a01".to_vec(),
        ] {
            assert_noop!(
                set_name(1, code, b"name".to_vec()),
                Error::<Test>::InvalidAddressNameCode
            );
        }
        for local in [b"0000".to_vec(), b"001".to_vec(), b"a001".to_vec()] {
            assert_noop!(
                set_address(1, local, b"house".to_vec()),
                Error::<Test>::InvalidAddressLocalNo
            );
        }
        assert_ok!(set_address(1, vec![], vec![b'x'; 16]));
        assert_noop!(
            set_address(1, vec![], vec![b'x'; 17]),
            Error::<Test>::FieldTooLong
        );
        assert_noop!(
            Address::set_address_name(
                RuntimeOrigin::signed(1),
                actor(),
                role(),
                vec![],
                b"001".to_vec(),
                b"002".to_vec(),
                b"101".to_vec(),
                b"name".to_vec()
            ),
            Error::<Test>::EmptyCode
        );
    });
}

#[test]
fn default_authority_denies_both_entrypoints() {
    assert!(!<() as AddressUpdateAuthority<u64>>::can_update_catalog(
        &1,
        b"registrar",
        b"REG",
        0
    ));
    assert!(!<() as AddressUpdateAuthority<u64>>::can_update_address(
        &1,
        b"registrar",
        b"REG",
        b"GD",
        b"001",
        1
    ));
}
