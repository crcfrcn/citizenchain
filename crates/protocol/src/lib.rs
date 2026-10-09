//! QR_V1 扫码签名协议真源包。
//!
//! 本 crate 是 host 端协议真源，不进入 runtime wasm。它统一保存 action registry、
//! 中文字段、拒绝原因和两态签名判定 schema，后续由这里生成或校验 Rust/Dart/TS 端产物。

pub mod codec;
pub mod decision;
pub mod export;
pub mod registry;

pub use codec::{
    b64_to_prefixed_hex, bytes_to_b64, public_key_b64, validate_qr_value, CodecError,
    PUBLIC_KEY_BYTES, QR_V1, SIGNATURE_BYTES,
};
pub use decision::{SignDecision, SignNormal, SignReject};
pub use registry::{
    action_by_code, action_by_key, field_label_zh, kind_by_code, kinds, reject_reason_zh,
    ActionEntry, ActionKind, FieldEntry, QrFieldConstraint, QrFieldEntry, QrKindEntry,
    RegistryError, RejectReasonEntry, SigningCategory,
};
