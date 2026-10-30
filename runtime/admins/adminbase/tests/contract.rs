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
