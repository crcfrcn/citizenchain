//! Rust host链交易签名材料唯一真源；原公开名称与SCALE字节保持。
//! QR/op_tag业务签名域仍由runtime/primitives拥有。

mod material;
pub use material::{
    build_signing_material, build_signing_material_from_call, build_signing_payloads,
    build_tx_extension, decode_runtime_call, SigningMaterial,
};

mod signature;
pub use signature::{
    account_id_from_public, parse_sr25519_public_key, parse_sr25519_signature_hex, sha256_hex,
    sign_material_with_pair, verify_signature,
};

mod extrinsic;
pub use extrinsic::{
    assemble_signed_extrinsic, build_signed_extrinsic_local, build_signed_extrinsic_with_pair,
    signed_extrinsic_hex,
};

mod preflight;
pub use preflight::{classify_invalid_tx, dry_run_reject_message, preflight_reject_message};
