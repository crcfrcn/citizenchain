use super::*;

use crate::keystore::SigningKey;
use sp_core::Pair;
use std::sync::Arc;
use std::sync::RwLock;

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
    let signer = crate::signer::KeystoreBatchSigner::new(slot);

    // 任意字节消息即可验证签/验回环(此处不构造真实批次签名域,见 offchain::batch_item::batch_signing_hash)。
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
    let signer = crate::signer::KeystoreBatchSigner::new(slot);
    let err = signer
        .sign_batch(b"x")
        .expect_err("key is None so must err");
    assert!(err.contains("未加载"), "错误信息应包含'未加载',实际:{err}");
}

#[test]
fn different_messages_produce_different_signatures() {
    let key = mk_signing_key(7);
    let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(Some(key)));
    let signer = crate::signer::KeystoreBatchSigner::new(slot);

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
    let signer = crate::signer::KeystoreBatchSigner::new(slot);

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
    let signer = crate::signer::KeystoreBatchSigner::new(slot.clone());

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
