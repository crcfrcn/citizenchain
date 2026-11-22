//! 验证真实ML-DSA身份、跨消息验签、二维码传输和原生接口。

use codec::{Decode, Encode};
use mldsa::{account_id, public_key, sign, transaction_message, verify, Proof};

#[test]
fn same_seed_same_identity_and_cross_message_rejection() {
    let public = public_key(&[7; 32]);
    assert_eq!(public, public_key(&[7; 32]));
    assert_ne!(public, public_key(&[8; 32]));
    let account = account_id(&public);
    let message = transaction_message(&account, b"call+nonce+genesis+version");
    let signature = sign(&[7; 32], &message).unwrap();
    assert!(verify(&public, &message, &signature));
    assert!(!verify(&public, b"different transaction", &signature));
    assert!(!verify(&public_key(&[8; 32]), &message, &signature));
    assert_ne!(
        message,
        transaction_message(&[9; 32], b"call+nonce+genesis+version")
    );
    let proof = Proof::First {
        account,
        public,
        signature,
    };
    assert_eq!(Proof::decode(&mut &proof.encode()[..]).unwrap(), proof);
}

#[test]
fn qr_transport_reorders_deduplicates_and_rejects_conflicts() {
    let text = "😀PQ-signature:".repeat(900);
    let frames = mldsa::qr_frames(&text).unwrap();
    assert!(frames.len() > 1);
    assert!(frames.iter().all(|frame| frame.len() <= 2331));
    assert_eq!(mldsa::qr_join(&frames[0]).unwrap(), None);
    let mut reversed = frames.clone();
    reversed.reverse();
    reversed.push(frames[0].clone());
    assert_eq!(mldsa::qr_join(&reversed.join("\n")).unwrap(), Some(text));
    let other = mldsa::qr_frames(&"other message".repeat(1000)).unwrap();
    assert!(mldsa::qr_join(&format!("{}\n{}", frames[0], other[1])).is_err());
    let mut corrupted = frames;
    let last = corrupted[0].pop().unwrap();
    corrupted[0].push(if last == 'A' { 'B' } else { 'A' });
    assert!(mldsa::qr_join(&corrupted.join("\n")).is_err());
    assert!(mldsa::qr_frames(&"x".repeat(mldsa::QR_MAX_BYTES + 1)).is_err());
}

#[cfg(feature = "ffi")]
#[test]
fn native_abi_matches_shared_crypto_and_rejects_null_outputs() {
    let child = [7; 32];
    let mut public = [0u8; mldsa::PUBLIC_KEY_BYTES];
    let mut signature = [0u8; mldsa::SIGNATURE_BYTES];
    let message = b"native signer interoperability";
    unsafe {
        assert_eq!(
            mldsa::ffi::gmb_mldsa_public_key(child.as_ptr(), core::ptr::null_mut()),
            -1
        );
        assert_eq!(
            mldsa::ffi::gmb_mldsa_sign(
                child.as_ptr(),
                message.as_ptr(),
                message.len(),
                public.as_mut_ptr(),
                signature.as_mut_ptr()
            ),
            0
        );
        assert_eq!(mldsa::PublicKey(public), public_key(&child));
        assert!(verify(
            &mldsa::PublicKey(public),
            message,
            &mldsa::Signature(signature)
        ));
        signature[0] ^= 1;
        assert_eq!(
            mldsa::ffi::gmb_mldsa_verify(
                public.as_ptr(),
                signature.as_ptr(),
                message.as_ptr(),
                message.len()
            ),
            -1
        );
    }
}

#[test]
fn development_and_operation_derivation_rules_are_explicit() {
    use mldsa::AccountAlgorithm;
    assert_eq!(
        AccountAlgorithm::from_option(2).unwrap(),
        AccountAlgorithm::Mldsa65
    );
    if mldsa::legacy_derivation_allowed() {
        assert_eq!(AccountAlgorithm::default(), AccountAlgorithm::Sr25519);
        assert_eq!(
            AccountAlgorithm::from_option(1).unwrap(),
            AccountAlgorithm::Sr25519
        );
    } else {
        assert_eq!(AccountAlgorithm::default(), AccountAlgorithm::Mldsa65);
        assert!(AccountAlgorithm::from_option(1).is_err());
    }
}

#[test]
fn sr_and_ml_use_the_same_account_bound_v5_implication() {
    let review = b"call+nonce+genesis+runtime+business-checks";
    let message = mldsa::review_message(&[7; 32], review);
    assert_eq!(
        message,
        mldsa::transaction_message(&[7; 32], &[vec![1], review.to_vec()].concat())
    );
    assert_ne!(message, mldsa::review_message(&[8; 32], review));
    assert_ne!(message, mldsa::review_message(&[7; 32], b"changed-call"));
    let proof = Proof::Sr25519 {
        account: [7; 32],
        signature: [9; 64],
    };
    assert_eq!(Proof::decode(&mut &proof.encode()[..]).unwrap(), proof);
}
