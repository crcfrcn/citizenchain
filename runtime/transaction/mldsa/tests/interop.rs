//! 从真实 FRAME 运行时导出公开测试数据，供 SDK 独立回归使用。
use chain_signing::*;
use codec::{Decode, Encode};
use sp_core::{sr25519, Pair, H256};

fn main() {
    let child = [7u8; 32];
    let public = mldsa::public_key(&child);
    let account = mldsa::account_id(&public);
    let call = runtime::RuntimeCall::OnchainTransaction(onchain_call());
    let genesis = H256::from(primitives::genesis::GENESIS_HASH);
    let material = build_signing_material_from_call(call.clone(), genesis, 7, 0, 0);
    let sr = sr25519::Pair::from_seed(&child);
    let sr_account = sr.public().0;
    let sr_signature = sign_material_with_pair(&material, &sr);
    let sr_extrinsic = assemble_signed_extrinsic(material, sr.public(), sr_signature);
    let material = build_signing_material_from_call(call.clone(), genesis, 7, 0, 0);
    let metadata = runtime::Runtime::metadata_at_version(16).expect("V16 metadata");
    let metadata_bytes =
        Vec::<u8>::decode(&mut &metadata.encode()[..]).expect("opaque metadata bytes");
    let transfer_call = runtime::RuntimeCall::OnchainTransaction(onchain::pallet::Call::<runtime::Runtime>::transfer_with_remark {
        beneficiary_account_id: runtime::AccountId::new([0x22;32]), amount: 123456,
        remark: b"CitizenSDK".to_vec().try_into().unwrap(),
    });
    let transfer_material = build_signing_material_from_call(transfer_call, genesis, 7, 0, 0);
    let transfer_signature = sign_material_with_pair(&transfer_material, &sr);
    let transfer_extrinsic = assemble_signed_extrinsic(build_signing_material_from_call(transfer_material.call.clone(), genesis, 7, 0, 0), sr.public(), transfer_signature);
    println!(
        "{}",
        serde_json::json!({
            "metadata": hex::encode(metadata_bytes),
            "transfer": {"call": hex::encode(transfer_material.call.encode()), "review": hex::encode(&transfer_material.payload),
                "message":hex::encode(transfer_material.signing_message(&sr_account)), "signature":hex::encode(transfer_signature.0), "extrinsic":hex::encode(transfer_extrinsic.encode())},
            "genesis": hex::encode(genesis), "account": hex::encode(account), "child": hex::encode(child),
            "call": hex::encode(call.encode()), "review": hex::encode(&material.payload),
            "extensions": hex::encode(material.tx_ext.encode()),
            "message": hex::encode(mldsa::review_message(&account, &material.payload)),
            "first": hex::encode(build_pq_signed_extrinsic(call.clone(), genesis, 7, 0, 0, &child, true).encode()),
            "stored": hex::encode(build_pq_signed_extrinsic(call, genesis, 7, 0, 0, &child, false).encode()),
            "sr_account": hex::encode(sr_account), "sr_signature": hex::encode(sr_signature.0),
            "sr_extrinsic": hex::encode(sr_extrinsic.encode()),
        })
    );
}

fn onchain_call() -> onchain::pallet::Call<runtime::Runtime> {
    onchain::pallet::Call::<runtime::Runtime>::transfer_with_remark {
        beneficiary_account_id: runtime::AccountId::new([0x69; 32]),
        amount: 111,
        remark: Vec::new().try_into().expect("empty bounded remark"),
    }
}
