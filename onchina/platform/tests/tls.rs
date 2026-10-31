//! 原所属模块回归，断言与输入保持。
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_tls_dir(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock before unix epoch: {e}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "onchina-tls-{label}-{}-{nonce}",
        std::process::id()
    )))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("read fingerprint input {} failed: {e}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[test]
fn restart_and_profile_change_never_rotate_root_ca() -> Result<(), String> {
    let dir = test_tls_dir("persistent-root")?;
    let now = OffsetDateTime::now_utc();
    ensure_certificate_material_at(&dir, now)?;
    let ca_cert_hash = sha256_file(&ca_cert_path(&dir))?;
    let ca_key_hash = sha256_file(&ca_key_path(&dir))?;
    let server_cert_hash = sha256_file(&server_cert_path(&dir))?;

    ensure_certificate_material_at(&dir, now)?;
    assert_eq!(sha256_file(&ca_cert_path(&dir))?, ca_cert_hash);
    assert_eq!(sha256_file(&ca_key_path(&dir))?, ca_key_hash);
    assert_eq!(sha256_file(&server_cert_path(&dir))?, server_cert_hash);

    fs::write(profile_marker_path(&dir), "renamed-profile")
        .map_err(|e| format!("change profile marker failed: {e}"))?;
    ensure_certificate_material_at(&dir, now)?;
    assert_eq!(sha256_file(&ca_cert_path(&dir))?, ca_cert_hash);
    assert_eq!(sha256_file(&ca_key_path(&dir))?, ca_key_hash);
    assert_ne!(sha256_file(&server_cert_path(&dir))?, server_cert_hash);
    fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
    Ok(())
}

#[test]
fn incomplete_root_ca_pair_fails_without_overwrite() -> Result<(), String> {
    let dir = test_tls_dir("missing-root")?;
    let now = OffsetDateTime::now_utc();
    ensure_certificate_material_at(&dir, now)?;
    let key_hash = sha256_file(&ca_key_path(&dir))?;
    fs::remove_file(ca_cert_path(&dir))
        .map_err(|e| format!("remove test certificate failed: {e}"))?;

    let error = ensure_certificate_material_at(&dir, now)
        .err()
        .ok_or_else(|| "incomplete root CA pair did not fail closed".to_string())?;
    assert!(error.contains("certificate is missing while private key exists"));
    assert_eq!(sha256_file(&ca_key_path(&dir))?, key_hash);
    assert!(!ca_cert_path(&dir).exists());
    fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
    Ok(())
}

#[test]
fn mismatched_root_ca_key_fails_without_certificate_overwrite() -> Result<(), String> {
    let dir = test_tls_dir("mismatched-root")?;
    let other_dir = test_tls_dir("other-root")?;
    let now = OffsetDateTime::now_utc();
    ensure_certificate_material_at(&dir, now)?;
    ensure_certificate_material_at(&other_dir, now)?;
    let cert_hash = sha256_file(&ca_cert_path(&dir))?;
    fs::copy(ca_key_path(&other_dir), ca_key_path(&dir))
        .map_err(|e| format!("install mismatched test key failed: {e}"))?;

    let error = ensure_certificate_material_at(&dir, now)
        .err()
        .ok_or_else(|| "mismatched root CA key was accepted".to_string())?;
    assert!(error.contains("certificate and private key do not match"));
    assert_eq!(sha256_file(&ca_cert_path(&dir))?, cert_hash);
    fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
    fs::remove_dir_all(other_dir).map_err(|e| format!("clean second test tls dir failed: {e}"))?;
    Ok(())
}
