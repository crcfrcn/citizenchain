use grandpakey::GrandpaKeyChangeKind;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalChangeKind {
    RoutineRotation,
    EmergencyRecovery,
}

impl From<LocalChangeKind> for GrandpaKeyChangeKind {
    fn from(value: LocalChangeKind) -> Self {
        match value {
            crate::types::LocalChangeKind::RoutineRotation => Self::RoutineRotation,
            crate::types::LocalChangeKind::EmergencyRecovery => Self::EmergencyRecovery,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingKeyChange {
    pub actor_cid_number: String,
    pub cid_short_name: String,
    pub old_public_key: String,
    pub new_public_key: String,
    pub change_kind: LocalChangeKind,
    pub proof_nonce: u64,
    pub proof_expires_at: u32,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandpaKeyChangeRequest {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub sign_nonce: u32,
    pub sign_block_number: u64,
    pub old_public_key: String,
    pub new_public_key: String,
    pub proof_nonce: u64,
    pub proof_expires_at: u32,
    pub change_kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandpaKeyChangeStatus {
    pub pending: bool,
    pub actor_cid_number: Option<String>,
    pub old_public_key: Option<String>,
    pub new_public_key: Option<String>,
    pub change_kind: Option<String>,
    pub tx_hash: Option<String>,
}
