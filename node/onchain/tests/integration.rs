use super::*;

#[test]
fn transfer_amounts_fees_and_cold_call_match_runtime() {
    use codec::Encode;

    let destination = [0xAA; 32];
    for (yuan, fen, fee) in [
        (1.0, 100u128, 10u128),
        (104.99, 10_499, 10),
        (105.0, 10_500, 11),
        (1000.0, 100_000, 100),
    ] {
        assert_eq!(amount_yuan_to_fen(yuan).unwrap(), fen);
        assert_eq!(calculate_transfer_fee(fen), fee);
        let bytes = build_transfer_with_remark_call(&destination, fen, "转账").unwrap();
        let call = runtime::RuntimeCall::OnchainTransaction(
            ::onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: destination.into(),
                amount: fen,
                remark: "转账".as_bytes().to_vec().try_into().unwrap(),
            },
        );
        // 冷签二维码中的调用必须是当前 Runtime 的真实调用编码，不只比较自制字节偏移。
        assert_eq!(bytes, call.encode());
        let material = chain_signing::build_signing_material(&bytes, &[0; 32], 7, 1, 0).unwrap();
        assert_eq!(material.call.encode(), bytes);
        let request = nodeclient::signing::QrSignRequest {
            proto: nodeclient::signing::QR_V1.into(),
            kind: nodeclient::signing::QR_KIND_SIGN_REQUEST,
            id: format!("transfer-test-{fen}"),
            expires_at: nodeclient::signing::now_secs().unwrap()
                + nodeclient::signing::DEFAULT_TTL_SECS,
            body: nodeclient::signing::SignRequestBody {
                action: nodeclient::signing::chain_action_code(&bytes).unwrap(),
                sig_alg: 1,
                signer_public_key: nodeclient::signing::public_key_b64(&[0x11; 32]).unwrap(),
                payload: nodeclient::signing::payload_b64(&material.payload),
            },
        };
        let envelope = serde_json::to_value(request).unwrap();
        assert!(qr_protocol::validate_qr_value(&envelope).is_ok());
    }
}
