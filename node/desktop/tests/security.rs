//! 验证映射审计写入、并发追加及受控路径边界。

use crate::validation::normalize_account_id;
use crate::validation::normalize_grandpa_key;
use crate::validation::normalize_node_key;
use crate::validation::normalize_public_key;

/// 审计回归只使用本用例独占的临时目录。
struct AuditDirectory(std::path::PathBuf);

impl AuditDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "gmb-mapping-audit-desktop-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir(&base).unwrap();
        Self(base)
    }
}

impl Drop for AuditDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mapping_audit_appends_to_the_existing_file() {
    let directory = AuditDirectory::new();
    let path = directory.0.join("security-audit.log");
    std::fs::write(&path, "既有审计记录\n").unwrap();
    crate::security::append_audit_log_at(&directory.0, "evm_mapping", "waiting_idle").unwrap();
    crate::security::append_audit_log_at(&directory.0, "evm_mapping", "complete").unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "既有审计记录");
    assert!(lines[1].ends_with("\taction=evm_mapping\tstatus=waiting_idle"));
    assert!(lines[2].ends_with("\taction=evm_mapping\tstatus=complete"));
}

#[test]
fn mapping_audit_serializes_concurrent_appends() {
    let directory = AuditDirectory::new();
    let threads = (0..4)
        .map(|worker| {
            let base = directory.0.clone();
            std::thread::spawn(move || {
                for seq in 0..8 {
                    let status = serde_json::json!({"worker": worker, "seq": seq}).to_string();
                    crate::security::append_audit_log_at(&base, "evm_mapping", &status).unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    let text = std::fs::read_to_string(directory.0.join("security-audit.log")).unwrap();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let (prefix, status) = line.split_once("\tstatus=").unwrap();
        assert!(prefix.ends_with("\taction=evm_mapping"));
        let value: serde_json::Value = serde_json::from_str(status).unwrap();
        assert!(seen.insert((
            value["worker"].as_u64().unwrap(),
            value["seq"].as_u64().unwrap()
        )));
    }
    assert_eq!(seen.len(), 32);
}

#[test]
fn mapping_audit_preserves_the_existing_rotation_limit() {
    let directory = AuditDirectory::new();
    let path = directory.0.join("security-audit.log");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(5 * 1024 * 1024).unwrap();
    drop(file);
    for i in 1..=5 {
        std::fs::write(
            directory.0.join(format!("security-audit.log.{i}")),
            format!("历史{i}"),
        )
        .unwrap();
    }
    crate::security::append_audit_log_at(&directory.0, "evm_mapping", "started").unwrap();
    assert_eq!(
        std::fs::metadata(directory.0.join("security-audit.log.1"))
            .unwrap()
            .len(),
        5 * 1024 * 1024
    );
    for i in 2..=5 {
        assert_eq!(
            std::fs::read_to_string(directory.0.join(format!("security-audit.log.{i}"))).unwrap(),
            format!("历史{}", i - 1)
        );
    }
    assert!(!directory.0.join("security-audit.log.6").exists());
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .ends_with("\taction=evm_mapping\tstatus=started\n"));
}

#[test]
fn mapping_audit_reports_write_failure_without_creating_a_parent() {
    let directory = AuditDirectory::new();
    let missing = directory.0.join("missing");
    assert!(crate::security::append_audit_log_at(&missing, "evm_mapping", "started").is_err());
    assert!(!missing.exists());
}

#[test]
fn miner_transfer_capability_is_single_use_and_can_be_revoked() {
    use crate::security::{
        consume_miner_transfer_token, issue_miner_transfer_token, revoke_miner_transfer_token,
    };
    assert!(!consume_miner_transfer_token("unissued-test-capability"));
    let token = issue_miner_transfer_token().unwrap();
    assert!(consume_miner_transfer_token(&token));
    assert!(!consume_miner_transfer_token(&token));
    let revoked = issue_miner_transfer_token().unwrap();
    revoke_miner_transfer_token(&revoked);
    assert!(!consume_miner_transfer_token(&revoked));
}

#[test]
fn empty_unlock_input_is_rejected_before_any_device_authentication() {
    use crate::security::ensure_unlock_password;
    assert!(ensure_unlock_password("").is_err());
    assert!(ensure_unlock_password(" \n\t ").is_err());
    assert_eq!(ensure_unlock_password(" 合成输入 ").unwrap(), "合成输入");
}

// --- normalize_account_id ---

#[test]
fn account_id_empty_rejected() {
    assert!(normalize_account_id("").is_err());
    assert!(normalize_account_id("   ").is_err());
}

#[test]
fn account_id_valid_hex() {
    let hex = format!("0x{}", "a1b2c3d4".repeat(8));
    let result = normalize_account_id(&hex).unwrap();
    assert!(result.starts_with("0x"));
    assert_eq!(result.len(), 66);
    assert_eq!(result, result.to_ascii_lowercase());
}

#[test]
fn account_id_hex_wrong_length() {
    assert!(normalize_account_id("0xabcd").is_err());
}

#[test]
fn account_id_hex_non_hex_chars() {
    let bad = format!("0x{}zz", "a1".repeat(31));
    assert!(normalize_account_id(&bad).is_err());
}

#[test]
fn account_id_hex_uppercase_rejected() {
    let hex = format!("0x{}", "A1B2C3D4".repeat(8));
    assert!(normalize_account_id(&hex).is_err());
}

#[test]
fn account_id_requires_0x_prefix() {
    assert!(normalize_account_id(&"a1".repeat(32)).is_err());
}

#[test]
fn public_key_uses_prefixed_lowercase_hex() {
    let input = format!("0x{}", "a1b2".repeat(16));
    assert_eq!(normalize_public_key(&input).unwrap(), input);
    assert!(normalize_public_key(&format!("0x{}", "A1B2".repeat(16))).is_err());
    assert!(normalize_public_key(&"a1".repeat(32)).is_err());
}

// --- normalize_node_key ---

#[test]
fn node_key_empty_rejected() {
    assert!(normalize_node_key("").is_err());
}

#[test]
fn node_key_valid_no_prefix() {
    let key = "a1b2c3d4".repeat(8);
    let result = normalize_node_key(&key).unwrap();
    assert_eq!(result.len(), 64);
    assert!(!result.starts_with("0x"));
}

#[test]
fn node_key_valid_with_0x_prefix() {
    let key = format!("0x{}", "a1b2c3d4".repeat(8));
    let result = normalize_node_key(&key).unwrap();
    assert_eq!(result.len(), 64);
    assert!(!result.starts_with("0x"));
}

#[test]
fn node_key_wrong_length() {
    assert!(normalize_node_key("abcdef").is_err());
}

// --- normalize_grandpa_key ---

#[test]
fn grandpa_key_empty_rejected() {
    assert!(normalize_grandpa_key("").is_err());
}

#[test]
fn grandpa_key_valid() {
    let key = "a1b2c3d4".repeat(8);
    let result = normalize_grandpa_key(&key).unwrap();
    assert_eq!(result.len(), 64);
}

#[test]
fn grandpa_key_strips_0x() {
    let key = format!("0x{}", "A1B2C3D4".repeat(8));
    let result = normalize_grandpa_key(&key).unwrap();
    assert_eq!(result, "a1b2c3d4".repeat(8));
}

#[test]
fn grandpa_key_wrong_length() {
    assert!(normalize_grandpa_key("abcd").is_err());
}
