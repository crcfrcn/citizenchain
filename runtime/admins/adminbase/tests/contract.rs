//! 管理员编码必须严格遵守容量和完整解码，不允许尾随或超长字段绕过约束。
use super::*;
use codec::{DecodeAll, Encode};

#[test]
fn administrator_decode_rejects_oversized_names_and_trailing_bytes() {
    let too_long = (7u64, Vec::<u8>::new(), vec![b'a'; 129], Vec::<u8>::new()).encode();
    assert!(Admin::<u64>::decode_all(&mut &too_long[..]).is_err());
    let mut valid = (7u64, Vec::<u8>::new(), vec![b'a'; 128], vec![b'b'; 128]).encode();
    assert!(Admin::<u64>::decode_all(&mut &valid[..]).is_ok());
    valid.push(0);
    assert!(Admin::<u64>::decode_all(&mut &valid[..]).is_err());
}

#[test]
fn administrator_kind_and_lifecycle_discriminants_are_fixed() {
    assert_eq!(AdminAccountKind::PublicInstitution.encode(), vec![0]);
    assert_eq!(AdminAccountKind::PrivateInstitution.encode(), vec![1]);
    assert_eq!(AdminAccountKind::PersonalMultisig.encode(), vec![2]);
    assert_eq!(AdminAccountStatus::Pending.encode(), vec![0]);
    assert_eq!(AdminAccountStatus::Active.encode(), vec![1]);
    assert_eq!(AdminAccountStatus::Closed.encode(), vec![2]);
}

#[test]
fn institution_wallet_state_and_change_have_separate_strict_codec_contracts() {
    let state = InstitutionAdminWalletState {
        wallet: Some(9u8),
        revision: 17,
    };
    let bytes = state.encode();
    assert_eq!(bytes, (Some(9u8), 17u64).encode());
    assert_eq!(
        InstitutionAdminWalletState::<u8>::decode_all(&mut &bytes[..]).unwrap(),
        state
    );
    let retired = InstitutionAdminWalletState::<u8> {
        wallet: None,
        revision: 18,
    };
    assert_eq!(retired.encode(), (None::<u8>, 18u64).encode());
    let change = InstitutionAdminWalletChange {
        admin_account: 1u8,
        expected_wallet: 9u8,
        new_wallet: 10u8,
        expected_revision: 17,
    };
    let bytes = change.encode();
    assert_eq!(bytes, (1u8, 9u8, 10u8, 17u64).encode());
    assert_eq!(
        InstitutionAdminWalletChange::<u8>::decode_all(&mut &bytes[..]).unwrap(),
        change
    );
    for end in 0..bytes.len() {
        assert!(InstitutionAdminWalletChange::<u8>::decode_all(&mut &bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(InstitutionAdminWalletChange::<u8>::decode_all(&mut &trailing[..]).is_err());
    let mut trailing_state = state.encode();
    trailing_state.push(0);
    assert!(InstitutionAdminWalletState::<u8>::decode_all(&mut &trailing_state[..]).is_err());
}
