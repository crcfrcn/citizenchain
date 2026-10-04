//! TLS 自签证书自动生成与持久化模块。
//!
//! 节点启动时自动检查 `<base-path>/tls/` 目录：
//! - 如果已有证书和密钥文件 → 直接加载
//! - 如果没有 → 用 rcgen 生成自签 ed25519 证书，写入磁盘
//!
//! TLS 层只负责传输加密，身份认证由 Noise 协议通过 peer ID 完成。

use std::{
    fs,
    path::{Path, PathBuf},
};

/// TLS 证书数据（DER 编码），用于传入 libp2p WSS transport。
pub struct TlsCertData {
    /// 私钥（DER 编码，PKCS#8 格式）。
    pub private_key_der: Vec<u8>,
    /// 证书链（DER 编码），通常只有一个自签证书。
    pub certificate_chain_der: Vec<Vec<u8>>,
}

/// 证书和密钥的持久化路径。
fn tls_dir(base_path: &Path) -> PathBuf {
    base_path.join("tls")
}

fn cert_path(base_path: &Path) -> PathBuf {
    tls_dir(base_path).join("cert.der")
}

fn key_path(base_path: &Path) -> PathBuf {
    tls_dir(base_path).join("key.der")
}

/// 加载已有证书，或在首次启动时自动生成并持久化。
pub fn load_or_generate_tls_cert(base_path: &Path) -> Result<TlsCertData, String> {
    let cert_file = cert_path(base_path);
    let key_file = key_path(base_path);

    if cert_file.is_file() && key_file.is_file() {
        // 已有证书，直接加载。
        let cert_der = fs::read(&cert_file).map_err(|e| format!("读取 TLS 证书失败: {e}"))?;
        let key_der = fs::read(&key_file).map_err(|e| format!("读取 TLS 私钥失败: {e}"))?;

        log::info!("已加载现有 TLS 自签证书: {}", cert_file.display());

        return Ok(TlsCertData {
            private_key_der: key_der,
            certificate_chain_der: vec![cert_der],
        });
    }

    // 首次启动，生成自签证书。
    log::info!("首次启动，生成 TLS 自签证书...");

    let certified_key = rcgen::generate_simple_self_signed(vec!["citizenchain-node".to_string()])
        .map_err(|e| format!("自签证书生成失败: {e}"))?;

    let cert_der = certified_key.cert.der().to_vec();
    let key_der = certified_key.key_pair.serialize_der();

    // 创建目录并写入文件。
    let dir = tls_dir(base_path);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 TLS 目录失败: {e}"))?;

    // Unix 系统设置目录权限为 0o700（仅所有者可访问）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(&dir, perms).map_err(|e| format!("设置 TLS 目录权限失败: {e}"))?;
    }

    // 原子写入：先写临时文件，再重命名。
    write_atomic(&cert_file, &cert_der)?;
    write_atomic(&key_file, &key_der)?;

    // Unix 系统设置密钥文件权限为 0o600（仅所有者可读写）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&key_file, perms).map_err(|e| format!("设置 TLS 私钥权限失败: {e}"))?;
    }

    log::info!("TLS 自签证书已生成并保存到: {}", dir.display());

    Ok(TlsCertData {
        private_key_der: key_der,
        certificate_chain_der: vec![cert_der],
    })
}

/// 原子写入文件：先写临时文件，再重命名，避免写入中断导致文件损坏。
fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, data)
        .map_err(|e| format!("写入临时文件失败 {}: {e}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).map_err(|e| {
        format!(
            "重命名文件失败 {} → {}: {e}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}
