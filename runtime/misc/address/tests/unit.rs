//! 原有行为回归完整迁入，复用所属包测试夹具。
use super::*;

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
