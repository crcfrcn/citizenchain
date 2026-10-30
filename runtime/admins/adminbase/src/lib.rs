#![cfg_attr(not(feature = "std"), no_std)]
//! 管理员共用原语。
//!
//! 本 crate 只放管理员共用类型、trait 与分类策略，不放业务 storage，
//! 也不直接创建任何 pallet。`publicadmin`、`privateadmin` 和
//! `personaladmin` 必须在各自模块内维护自己的管理员状态。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, traits::ConstU32, BoundedVec};
use primitives::cid::code::{
    is_fixed_governance_code, is_private_legal_code, is_public_legal_code, is_unincorporated_code,
    InstitutionCode, PMUL,
};
use primitives::core_const::CID_NUMBER_MAX_BYTES;
use scale_info::TypeInfo;
use sp_runtime::Debug;

/// 固定治理公权机构码,唯一真源在 `primitives::cid::code`。
pub use primitives::cid::code::{FRG, NJD};

// 模块仅在本包内装配，原公开接口由包根唯一导出。
mod policy;
mod traits;
mod types;
pub use policy::*;
pub use traits::*;
pub use types::*;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
