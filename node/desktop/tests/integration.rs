use super::*;

use crate::storage::blake2_128_concat;
use crate::storage::blake2_map;
use crate::storage::prefix;
use crate::storage::to_hex;
use crate::storage::twox64_map;
use sp_crypto_hashing::twox_64;

#[test]
fn restricted_atomic_configuration_replaces_complete_content_and_cleans_failed_rename() {
    use crate::security::write_text_atomic_restricted;
    let directory = std::env::temp_dir().join(format!(
        "node-atomic-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let target = directory.join("public.txt");
    write_text_atomic_restricted(&target, "公开合成配置一").unwrap();
    write_text_atomic_restricted(&target, "公开合成配置二").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "公开合成配置二");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let blocked = directory.join("directory");
    std::fs::create_dir(&blocked).unwrap();
    assert!(write_text_atomic_restricted(&blocked, "公开合成配置").is_err());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "公开合成配置二");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn prefix_matches_known_system_account() {
    // 规范值:twox_128("System") ++ twox_128("Account");与 Substrate 生态公认常量逐字节一致,
    // 证明本模块 twox_128 就是规范实现(据此保证委托来的旧调用点键不变)。
    assert_eq!(
        to_hex(&prefix(b"System", b"Account")),
        "0x26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9"
    );
}

#[test]
fn blake2_map_layout_is_prefix_plus_concat() {
    let k = b"\x01\x02\x03";
    let full = blake2_map(b"Foo", b"Bar", k);
    let mut expect = prefix(b"Foo", b"Bar");
    expect.extend_from_slice(&blake2_128_concat(k));
    assert_eq!(full, expect);
}

#[test]
fn twox64_map_layout_is_prefix_plus_concat() {
    let k = 7u32.to_le_bytes();
    let full = twox64_map(b"Foo", b"Bar", &k);
    let mut expect = prefix(b"Foo", b"Bar");
    expect.extend_from_slice(&twox_64(&k));
    expect.extend_from_slice(&k);
    assert_eq!(full, expect);
}
