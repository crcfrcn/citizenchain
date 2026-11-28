//! 组织QR_V1码型、动作、中文字段及签名判定的宿主协议真源。

#[cfg(feature = "registry")]
pub mod codec;
#[cfg(feature = "registry")]
pub mod decision;
#[cfg(feature = "registry")]
pub mod export;
#[cfg(feature = "registry")]
pub mod registry;
#[cfg(feature = "transport")]
pub mod transport;
#[cfg(feature = "transport")]
pub mod wasm;

#[cfg(feature = "registry")]
pub use codec::{
    b64_to_prefixed_hex, bytes_to_b64, decode_node_deploy_intent, encode_node_deploy_intent,
    node_deploy_signing_preimage, public_key_b64, validate_qr_value, CodecError,
    NodeDeployIntent, NodeDeployKind, NODE_DEPLOY_SIGNING_DOMAIN, PUBLIC_KEY_BYTES, QR_V1,
    SIGNATURE_BYTES,
};
#[cfg(feature = "registry")]
pub use decision::{SignDecision, SignNormal, SignReject};
#[cfg(feature = "registry")]
pub use registry::{
    action_by_code, action_by_key, field_label_zh, kind_by_code, kinds, reject_reason_zh,
    ActionEntry, ActionKind, FieldEntry, QrFieldConstraint, QrFieldEntry, QrKindEntry,
    RegistryError, RejectReasonEntry, SigningCategory,
};
