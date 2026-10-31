//! 已签名交易组装，复用所属签名材料。
use crate::{
    account_id_from_public, build_signing_material_from_call, sign_material_with_pair,
    SigningMaterial,
};
use codec::Encode;
use sp_core::{sr25519, Pair, H256};
use sp_runtime::MultiAddress;

/// 组装 signed extrinsic。
pub fn assemble_signed_extrinsic(
    material: SigningMaterial,
    public: sr25519::Public,
    signature: sr25519::Signature,
) -> runtime::UncheckedExtrinsic {
    sp_runtime::generic::UncheckedExtrinsic::new_signed(
        material.call,
        MultiAddress::Id(account_id_from_public(public)),
        runtime::Signature::Sr25519(signature),
        material.tx_ext,
    )
    .into()
}

/// signed extrinsic 的 0x-prefixed SCALE hex。
pub fn signed_extrinsic_hex(extrinsic: &runtime::UncheckedExtrinsic) -> String {
    format!("0x{}", hex::encode(extrinsic.encode()))
}

/// 使用指定 runtime 版本构建并签名 extrinsic。
pub fn build_signed_extrinsic_with_pair(
    call: runtime::RuntimeCall,
    genesis_hash: H256,
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
    sender: &sr25519::Pair,
) -> runtime::UncheckedExtrinsic {
    let material =
        build_signing_material_from_call(call, genesis_hash, nonce, spec_version, tx_version);
    let public = sender.public();
    let signature = sign_material_with_pair(&material, sender);
    assemble_signed_extrinsic(material, public, signature)
}

/// 使用本机 runtime 版本构建并签名 extrinsic，供 benchmark/本地结算沿用既有语义。
pub fn build_signed_extrinsic_local(
    call: runtime::RuntimeCall,
    genesis_hash: H256,
    nonce: u32,
    sender: &sr25519::Pair,
) -> runtime::UncheckedExtrinsic {
    build_signed_extrinsic_with_pair(
        call,
        genesis_hash,
        nonce,
        runtime::VERSION.spec_version,
        runtime::VERSION.transaction_version,
        sender,
    )
}
