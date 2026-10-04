//! GRANDPA 验证密钥更换持钥证明。
//!
//! 正常更换由旧、新两把 GRANDPA ed25519 私钥签同一摘要；紧急恢复仅要求新私钥
//! 签名，因为旧私钥已经丢失或无法使用。摘要绑定创世哈希、机构、岗位、发起账户、
//! 当前 authority set、nonce 与有效期，不能跨链、跨机构、跨账户或跨轮次重放。

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_core::ed25519;

use crate::{pallet, CidNumber, RoleCode};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
    MaxEncodedLen,
)]
/// GRANDPA 验证密钥变更路径。
pub enum GrandpaKeyChangeKind {
    /// 旧私钥仍可签名，由单个委员直接正常更换。
    RoutineRotation,
    /// 旧私钥不可用，由目标机构委员内部投票恢复。
    EmergencyRecovery,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
/// 旧、新 GRANDPA 私钥共同解释的唯一签名载荷。
pub struct GrandpaKeyProofPayload<AccountId, BlockNumber, Hash> {
    pub genesis_hash: Hash,
    pub actor_cid_number: CidNumber,
    pub actor_role_code: RoleCode,
    pub initiator_account_id: AccountId,
    pub old_public_key: [u8; 32],
    pub new_public_key: [u8; 32],
    pub current_set_id: u64,
    pub proof_nonce: u64,
    pub proof_expires_at: BlockNumber,
    pub change_kind: GrandpaKeyChangeKind,
}

/// 字段数量与链上持钥证明协议完全一致，不能为了缩短参数表隐藏或漏绑签名字段。
#[allow(clippy::too_many_arguments)]
pub(crate) fn payload<T: pallet::Config>(
    actor_cid_number: CidNumber,
    actor_role_code: RoleCode,
    initiator_account_id: T::AccountId,
    old_public_key: [u8; 32],
    new_public_key: [u8; 32],
    proof_nonce: u64,
    proof_expires_at: BlockNumberFor<T>,
    change_kind: GrandpaKeyChangeKind,
) -> GrandpaKeyProofPayload<T::AccountId, BlockNumberFor<T>, T::Hash> {
    GrandpaKeyProofPayload {
        genesis_hash: frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::default()),
        actor_cid_number,
        actor_role_code,
        initiator_account_id,
        old_public_key,
        new_public_key,
        current_set_id: pallet_grandpa::Pallet::<T>::current_set_id(),
        proof_nonce,
        proof_expires_at,
        change_kind,
    }
}

/// 生成 GRANDPA 更换证明摘要；客户端必须使用同一 SCALE 载荷与统一签名域。
pub fn signing_digest<AccountId: Encode, BlockNumber: Encode, Hash: Encode>(
    payload: &GrandpaKeyProofPayload<AccountId, BlockNumber, Hash>,
) -> [u8; 32] {
    primitives::sign::signing_message(
        primitives::sign::OP_SIGN_GRANDPA_KEY_CHANGE,
        &payload.encode(),
    )
}

pub(crate) fn verify_signature<AccountId: Encode, BlockNumber: Encode, Hash: Encode>(
    public_key: [u8; 32],
    signature: [u8; 64],
    payload: &GrandpaKeyProofPayload<AccountId, BlockNumber, Hash>,
) -> bool {
    sp_io::crypto::ed25519_verify(
        &ed25519::Signature::from_raw(signature),
        &signing_digest(payload),
        &ed25519::Public::from_raw(public_key),
    )
}
