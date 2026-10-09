//! 基于 `settlement::keystore::SigningKey` 的批次签名器。
//!
//!
//! - 本文件实现 `packer::BatchSigner` trait,把 batch 的签名消息转交给清算行
//!   管理员的 sr25519 私钥。
//! - `SigningKey` 沿用 `settlement/keystore.rs` 的密钥容器,
//!   持有 `Arc<RwLock<Option<..>>>`
//!   是为了支持**热切换密钥**(节点运行中重新解锁后替换 inner)+ 未加载时 None
//!   的情况下签名直接返回 Err,便于 packer 通过 `rollback` 路径回滚 pending。
//! - 本模块**不依赖 substrate client / TransactionPool**,可独立编译 + 单测。
//!
//! 与 submitter 的衔接:
//! - `service.rs` 启动时解密密钥得到 `SigningKey`,用
//!   `Arc::new(RwLock::new(Some(key)))` 包好传给 `KeystoreBatchSigner::new`,
//!   再作为 `BatchSigner` 注入 `start_clearing_bank_components`。

use sp_core::{sr25519, Pair};
use std::sync::{Arc, RwLock};

use super::keystore::SigningKey;
use super::packer::BatchSigner;

/// 基于 `SigningKey` 的 `BatchSigner` 实现。
pub struct KeystoreBatchSigner {
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
mod tests {
    use super::*;

    fn mk_signing_key(seed_byte: u8) -> SigningKey {
        let seed = [seed_byte; 32];
        let pair = <sr25519::Pair as Pair>::from_seed(&seed);
        SigningKey {
            pair,
            cid_number: "AH001-SCB0V-123456789-2026".to_string(),
        }
    }

    #[test]
    fn sign_produces_verifiable_signature() {
        let key = mk_signing_key(42);
        let public = key.pair.public();
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(Some(key)));
        let signer = KeystoreBatchSigner::new(slot);

        // 任意字节消息即可验证签/验回环(此处不构造真实批次签名域,见 packer::batch_signing_message)。
        let msg = b"batch-signer-roundtrip-fixture";
        let sig_bytes = signer.sign_batch(msg).expect("sign ok");

        let sig = sr25519::Signature::from_raw(sig_bytes);
        assert!(
            <sr25519::Pair as Pair>::verify(&sig, msg, &public),
            "签名必须对同一密钥的公钥验签通过"
        );
    }

    #[test]
    fn sign_without_key_loaded_errs() {
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(None));
        let signer = KeystoreBatchSigner::new(slot);
        let err = signer
            .sign_batch(b"x")
            .expect_err("key is None so must err");
        assert!(err.contains("未加载"), "错误信息应包含'未加载',实际:{err}");
    }

    #[test]
    fn different_messages_produce_different_signatures() {
        let key = mk_signing_key(7);
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(Some(key)));
        let signer = KeystoreBatchSigner::new(slot);

        let sig1 = signer.sign_batch(b"msg-a").unwrap();
        let sig2 = signer.sign_batch(b"msg-b").unwrap();
        assert_ne!(
            sig1, sig2,
            "不同消息必须产生不同签名(sr25519 本身带随机项,同消息也不同,因此两条也不同)"
        );
    }

    #[test]
    fn signature_does_not_verify_against_wrong_key() {
        let key_a = mk_signing_key(1);
        let key_b = mk_signing_key(2);
        let wrong_pub = key_b.pair.public();
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(Some(key_a)));
        let signer = KeystoreBatchSigner::new(slot);

        let sig_bytes = signer.sign_batch(b"msg").unwrap();
        let sig = sr25519::Signature::from_raw(sig_bytes);
        assert!(
            !<sr25519::Pair as Pair>::verify(&sig, b"msg", &wrong_pub),
            "用 A 的私钥签的消息不能被 B 的公钥验过"
        );
    }

    #[test]
    fn hot_swap_key_takes_effect() {
        let key_a = mk_signing_key(10);
        let pub_a = key_a.pair.public();
        let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(Some(key_a)));
        let signer = KeystoreBatchSigner::new(slot.clone());

        let sig1 = signer.sign_batch(b"same-msg").unwrap();
        assert!(<sr25519::Pair as Pair>::verify(
            &sr25519::Signature::from_raw(sig1),
            b"same-msg",
            &pub_a
        ));

        // 热替换到新密钥
        let key_b = mk_signing_key(11);
        let pub_b = key_b.pair.public();
        *slot.write().unwrap() = Some(key_b);

        let sig2 = signer.sign_batch(b"same-msg").unwrap();
        assert!(<sr25519::Pair as Pair>::verify(
            &sr25519::Signature::from_raw(sig2),
            b"same-msg",
            &pub_b
        ));
    }
}
