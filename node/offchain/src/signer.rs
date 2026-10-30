use crate::keystore::SigningKey;
use crate::packer::BatchSigner;
use sp_core::sr25519;
use sp_core::Pair;
use std::sync::Arc;
use std::sync::RwLock;

/// 基于 `SigningKey` 的 `BatchSigner` 实现。
pub(crate) struct KeystoreBatchSigner {
    signing_key: Arc<RwLock<Option<SigningKey>>>,
}

impl KeystoreBatchSigner {
    /// 构造签名器。
    ///
    /// [`signing_key`] 外部加载(解密)完成后的密钥容器。None 表示尚未加载,
    ///                 此时调用 `sign_batch` 会返回 Err。
    pub fn new(signing_key: Arc<RwLock<Option<SigningKey>>>) -> Self {
        Self { signing_key }
    }
}

impl BatchSigner for KeystoreBatchSigner {
    fn sign_batch(&self, message: &[u8]) -> Result<[u8; 64], String> {
        let guard = self
            .signing_key
            .read()
            .map_err(|e| format!("签名密钥锁读取失败:{e}"))?;
        let key = guard
            .as_ref()
            .ok_or_else(|| "清算行签名管理员密钥未加载(密码错误或节点未提供密码)".to_string())?;
        let signature = <sr25519::Pair as Pair>::sign(&key.pair, message);
        Ok(signature.0)
    }
}
#[cfg(test)]
#[path = "../tests/signer.rs"]
mod tests;
