use crate::keystore::ensure_directory_secure;
use crate::keystore::has_key_in_keystore;
use crate::keystore::read_key_from_keystore;
use crate::keystore::remove_key_from_keystore;
use crate::keystore::write_key_to_keystore;
use crate::keystore::write_key_to_keystore_preserving_others;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

fn unique_temp_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .unwrap_or(0);
    std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("node-keystore-{label}-{stamp}"))
}

#[test]
fn ensure_directory_secure_creates_path() {
    let path = unique_temp_dir("create")
        .join("chains")
        .join("citizenchain")
        .join("keystore");
    ensure_directory_secure(&path).unwrap();
    assert!(path.is_dir());
    let _ = fs::remove_dir_all(path.ancestors().nth(3).unwrap());
}

#[cfg(unix)]
#[test]
fn ensure_directory_secure_sets_permissions_to_0700() {
    let path = unique_temp_dir("mode")
        .join("chains")
        .join("citizenchain")
        .join("keystore");
    ensure_directory_secure(&path).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let _ = fs::remove_dir_all(path.ancestors().nth(3).unwrap());
}

#[test]
fn grandpa_rotation_preserves_both_keys_then_removes_only_old_key() {
    let root = unique_temp_dir("grandpa-rotation");
    let keystore = root.join("chains").join("citizenchain").join("keystore");
    ensure_directory_secure(&keystore).unwrap();
    let dirs = vec![keystore];
    let old_public_key = "11".repeat(32);
    let new_public_key = "22".repeat(32);

    write_key_to_keystore(&dirs, "6772616e", &old_public_key, "\"0xold\"\n").unwrap();
    write_key_to_keystore_preserving_others(&dirs, "6772616e", &new_public_key, "\"0xnew\"\n")
        .unwrap();
    assert!(has_key_in_keystore(&dirs, "6772616e", &old_public_key));
    assert!(has_key_in_keystore(&dirs, "6772616e", &new_public_key));
    assert_eq!(
        read_key_from_keystore(&dirs, "6772616e", &new_public_key).unwrap(),
        Some("\"0xnew\"\n".to_string())
    );

    remove_key_from_keystore(&dirs, "6772616e", &old_public_key).unwrap();
    assert!(!has_key_in_keystore(&dirs, "6772616e", &old_public_key));
    assert!(has_key_in_keystore(&dirs, "6772616e", &new_public_key));
    let _ = fs::remove_dir_all(root);
}
