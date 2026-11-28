//! 验证转账金额、费用和冷签调用与真实Runtime一致。

use super::*;

#[test]
fn transfer_amounts_fees_and_cold_call_match_runtime() {
    use codec::Encode;

    let destination = [0xAA; 32];
    for (fen, fee) in [
        (100u128, 10u128),
        (10_499, 10),
        (10_500, 11),
        (100_000, 100),
    ] {
        assert_eq!(parse_amount_fen(&fen.to_string()).unwrap(), fen);
        assert_eq!(calculate_transfer_fee(fen, ""), fee);
        assert_eq!(
            calculate_transfer_fee(fen, "转账"),
            fee + primitives::fee_policy::FLAT_FEE
        );
        let address=nodeclient::signing::account_id_to_ss58(&destination).unwrap();
        let bytes=build_transfer_call(crate::types::TransferMode::Transfer,&[crate::types::TransferItem{to_address:address,amount_fen:fen.to_string(),remark:"转账".into()}]).unwrap().encode();
        let call = runtime::RuntimeCall::Onchain(::onchain::pallet::Call::transfer_with_remark {
            beneficiary_account_id: destination.into(),
            amount: fen,
            remark: "转账".as_bytes().to_vec().try_into().unwrap(),
        });
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

#[test]
fn official_batch_encoding_and_large_review_bind_every_item() {
    use codec::{Encode,Decode};
    use crate::types::{TransferMode,TransferItem};
    let address=nodeclient::signing::account_id_to_ss58(&[0xAA;32]).unwrap();
    let items=vec![TransferItem{to_address:address,amount_fen:"10500".into(),remark:"转账".into()};8];
    for (mode,call_index) in [(TransferMode::Batch,0),(TransferMode::BatchAll,2),(TransferMode::ForceBatch,4)] {
        let call=build_transfer_call(mode,&items).unwrap(); let bytes=call.encode();
        assert_eq!(&bytes[..2],&[27,call_index]);
        println!("UTILITY_GOLDEN {}",serde_json::json!({"name":match mode {TransferMode::Batch=>"batch",TransferMode::BatchAll=>"batch_all",_=>"force_batch"},"pallet_index":27,"call_index":call_index,"expected_call_data_hex":format!("0x{}",hex::encode(&bytes)),"fields":{"transfer_count":8,"amount_fen":"84000","fee_fen":"888"}}));
        assert_eq!(runtime::RuntimeCall::decode(&mut bytes.as_slice()).unwrap(),call);
        let material=chain_signing::build_signing_material(&bytes,&[9;32],0,9,8).unwrap();
        assert!(material.payload.len()>256);
        let mut changed=items.clone();changed[7].amount_fen="10501".into();
        let other=chain_signing::build_signing_material(&build_transfer_call(mode,&changed).unwrap().encode(),&[9;32],0,9,8).unwrap();
        assert_ne!(material.signing_message(&[1;32]),other.signing_message(&[1;32]));
        assert!(nodeclient::signing::chain_action_code(&bytes).is_ok());
    }
}

#[test]
fn transaction_history_wire_keeps_exact_amounts_and_strict_cursor() {
    use crate::types::{TransactionCursor,TransactionHistory,TransactionRecord};
    let value=TransactionHistory{records:vec![TransactionRecord{block_number:1,timestamp:1782950400000,tx_hash:Some("0xfixture".into()),event_index:0,account_id:format!("0x{}","11".repeat(32)),direction:"sent".into(),mode:"transfer".into(),transfer_count:1,amount_fen:u128::MAX.to_string(),fee_fen:"10".into(),state:"success".into()}],cursor:Some(TransactionCursor{block_number:1,event_index:Some(0)})};
    let encoded=serde_json::to_value(value).unwrap();
    assert_eq!(encoded["records"][0]["amountFen"],u128::MAX.to_string());
    assert_eq!(encoded["cursor"],serde_json::json!({"blockNumber":1,"eventIndex":0}));
    let decoded:TransactionHistory=serde_json::from_value(encoded.clone()).unwrap();assert_eq!(decoded.records[0].amount_fen,u128::MAX.to_string());
    let mut rounded=encoded;rounded["records"][0]["amountFen"]=serde_json::json!(9007199254740992u64);assert!(serde_json::from_value::<TransactionHistory>(rounded).is_err());
    assert!(serde_json::from_value::<TransactionCursor>(serde_json::json!({"blockNumber":-1,"eventIndex":null})).is_err());
    assert!(serde_json::from_value::<TransactionCursor>(serde_json::json!({"blockNumber":1,"eventIndex":0,"extra":1})).is_err());
}
