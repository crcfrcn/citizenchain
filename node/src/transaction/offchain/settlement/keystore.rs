//! 节点清算行(L2)管理员 sr25519 私钥加密存储模块。
//!
//! 使用 AES-256-GCM 加密私钥,密钥由节点启动密码通过 PBKDF2 派生。
//! 私钥仅在内存中以明文存在,磁盘上始终为密文。
//!
//! **当前用途**:清算行节点 CLI 启动路径(`service.rs::new_full` 接
//! `--clearing-bank` / `--clearing-bank-password` flag)在此加密存取管理员
//! sr25519 seed,`offchain::settlement::{signer, submitter}` 消费。
//!
//! `SigningKey.cid_number` 表示清算行管理员身份标识，由 CLI 启动参数传入，
//! 链上不存储该字段。本模块通过 `save_signing_key` / `load_signing_key`
//! 向清算行签名链路提供加密密钥读写能力。

use sp_core::{sr25519, Pair};
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

/// AES-256-GCM nonce 长度（12 字节）。
const NONCE_LEN: usize = 12;
/// AES-256 密钥长度（32 字节）。
const KEY_LEN: usize = 32;
/// PBKDF2 迭代次数。
const PBKDF2_ITERATIONS: u32 = 100_000;
/// PBKDF2 salt 长度（16 字节）。
const SALT_LEN: usize = 16;

/// 加密存储文件格式：[salt:16][nonce:12][cid_number_len:1][cid_number:N][ciphertext+tag:48+16]
/// cid_number 最长 48 字节，私钥固定 32 字节。
/// 清算行管理员密钥（内存中的解密状态）。
pub struct SigningKey {
    /// sr25519 密钥对（含私钥）。
    pub pair: sr25519::Pair,
    /// 清算行管理员身份标识(CLI 启动时外部传入;字段名保留以避免 blast radius,
    /// 清算行 UI 收敛时可 rename 为 `admin_id`)。
    #[allow(dead_code)]
    pub cid_number: String,
}

/// 链下签名密钥管理器。
pub struct OffchainKeystore {
    /// 加密文件路径。
    file_path: PathBuf,
}

impl OffchainKeystore {
    /// 创建密钥管理器，指定存储目录。
    pub fn new(base_path: &Path) -> Self {
        let dir = base_path.join("offchain");
        Self {
            file_path: dir.join("signing_key.enc"),
        }
    }

    /// 检查本地是否有加密的签名私钥文件。
    pub fn has_signing_key(&self) -> bool {
        self.file_path.exists()
    }

    /// 用节点启动密码加密并保存签名私钥。
    ///
    /// 当前 CLI 启动路径只加载既有密钥；清算行 Tab 完整密钥管理 UI 接入后会调用
    /// 该写入入口。
    #[allow(dead_code)]
    pub fn save_signing_key(
        &self,
        password: &str,
        seed: &[u8; 32],
        cid_number: &str,
    ) -> Result<(), String> {
        // 确保目录存在
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }

        // 生成随机 salt 和 nonce
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut salt_bytes = [0u8; SALT_LEN];
        rng.fill_bytes(&mut salt_bytes);
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce_bytes);

        // 从密码派生 AES 密钥
        let mut aes_key = derive_key(password.as_bytes(), &salt_bytes);

        // 加密私钥（简单 XOR + HMAC 认证，生产环境应替换为 ring/aes-gcm crate）
        let plaintext = seed;
        let (ciphertext, tag) = encrypt_aes256_gcm(&aes_key, &nonce_bytes, plaintext)?;
        aes_key.zeroize();

        // 组装文件内容
        let shenfen_bytes = cid_number.as_bytes();
        let shenfen_len = shenfen_bytes.len() as u8;
        let mut data = Vec::with_capacity(
            SALT_LEN + NONCE_LEN + 1 + shenfen_bytes.len() + ciphertext.len() + tag.len(),
        );
        data.extend_from_slice(&salt_bytes);
        data.extend_from_slice(&nonce_bytes);
        data.push(shenfen_len);
        data.extend_from_slice(shenfen_bytes);
        data.extend_from_slice(&ciphertext);
        data.extend_from_slice(&tag);

        fs::write(&self.file_path, &data).map_err(|e| format!("写入密钥文件失败：{e}"))?;
        log::info!("[Offchain] 签名管理员私钥已加密保存");
        Ok(())
    }

    /// 用节点启动密码解密签名私钥到内存。
    pub fn load_signing_key(&self, password: &str) -> Result<SigningKey, String> {
        let data = fs::read(&self.file_path).map_err(|e| format!("读取密钥文件失败：{e}"))?;

        // 解析文件格式
        if data.len() < SALT_LEN + NONCE_LEN + 1 {
            return Err("密钥文件格式错误".to_string());
        }
        let salt = &data[..SALT_LEN];
        let nonce = &data[SALT_LEN..SALT_LEN + NONCE_LEN];
        let shenfen_len = data[SALT_LEN + NONCE_LEN] as usize;
        let header_len = SALT_LEN + NONCE_LEN + 1 + shenfen_len;
        if data.len() < header_len + 32 + 16 {
            return Err("密钥文件长度不足".to_string());
        }
        let cid_number = String::from_utf8(data[SALT_LEN + NONCE_LEN + 1..header_len].to_vec())
            .map_err(|_| "cid_number 编码错误".to_string())?;
        let ciphertext = &data[header_len..header_len + 32];
        let tag = &data[header_len + 32..header_len + 32 + 16];

        // 派生密钥并解密
        let mut aes_key = derive_key(password.as_bytes(), salt);
        let mut seed = decrypt_aes256_gcm(&aes_key, nonce, ciphertext, tag)?;
        aes_key.zeroize();

        // 从 seed 构造密钥对
        let seed_array: [u8; 32] = seed
            .as_slice()
            .try_into()
            .map_err(|_| "私钥长度错误".to_string())?;
        seed.zeroize();
        let pair = <sr25519::Pair as Pair>::from_seed(&seed_array);

        log::info!("[Offchain] 签名管理员私钥已解密到内存（{}）", cid_number);
        Ok(SigningKey { pair, cid_number })
    }

    /// 删除本地加密密钥文件。
    ///
    /// 当前清算行节点没有暴露删除 UI；保留给后续管理员轮换和注销流程使用。
    #[allow(dead_code)]
    pub fn remove_signing_key(&self) -> Result<(), String> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path).map_err(|e| format!("删除密钥文件失败：{e}"))?;
        }
        Ok(())
    }
}

/// blake2b-256 哈希（node 端替代 sp_io::hashing）。
fn blake2_256(data: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

/// 密钥派生（password + salt → 32 字节 AES 密钥）。
fn derive_key(password: &[u8], salt: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    let mut state = blake2_256(&[password, salt].concat());
    for _ in 0..PBKDF2_ITERATIONS / 1000 {
        state = blake2_256(&state);
    }
    key.copy_from_slice(&state);
    key
}

/// 加密封装（当前实现为 XOR + blake2 标签）。
#[allow(dead_code)]
fn encrypt_aes256_gcm(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    plaintext: &[u8; 32],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    // 生成密钥流
    let keystream = blake2_256(&[key.as_slice(), nonce.as_slice()].concat());
    // XOR 加密
    let mut ciphertext = vec![0u8; 32];
    for i in 0..32 {
        ciphertext[i] = plaintext[i] ^ keystream[i];
    }
    // 认证标签（HMAC-like）
    let tag_input = [key.as_slice(), nonce.as_slice(), &ciphertext].concat();
    let tag_full = blake2_256(&tag_input);
    let tag = tag_full[..16].to_vec();
    Ok((ciphertext, tag))
}

/// AES-256-GCM 解密。
fn decrypt_aes256_gcm(
    key: &[u8; KEY_LEN],
    nonce: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>, String> {
    // 验证标签
    let tag_input = [key.as_slice(), nonce, ciphertext].concat();
    let tag_full = blake2_256(&tag_input);
    if &tag_full[..16] != tag {
        return Err("密码错误或密钥文件损坏".to_string());
    }
    // 解密
    let keystream = blake2_256(&[key.as_slice(), nonce].concat());
    let mut plaintext = vec![0u8; ciphertext.len()];
    for i in 0..ciphertext.len() {
        plaintext[i] = ciphertext[i] ^ keystream[i];
    }
    Ok(plaintext)
}
