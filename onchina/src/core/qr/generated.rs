// 本文件由 citizenchain/crates/qr-protocol/registry/kinds.yaml 生成，禁止手改。

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrKind {
    SignRequest = 1,
    SignResponse = 2,
    UserContact = 3,
    UserTransfer = 4,
    AccountIdCode = 5,
    AccountDataKeyResponse = 6,
}

impl QrKind {
    pub fn code(self) -> u8 {
        self as u8
    }
}

pub(super) fn validate(
    value: &serde_json::Value,
) -> Result<qr_protocol::QrKindEntry, super::QrParseError> {
    qr_protocol::validate_qr_value(value)
        .map_err(|error| super::QrParseError::BadField(error.to_string()))
}
