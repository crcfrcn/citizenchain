//! 私权机构生命周期使用的实体共享 trait 出口。
//!
//! 具体 trait 唯一真源在 `entity-primitives`；本文件只为本 pallet 内部保持
//! `crate::traits::*` 的短路径，避免复制定义。

pub use entity_primitives::{
    AccountValidator, InstitutionCidQuery, InstitutionLegalRepresentativeQuery,
    InstitutionMultisigQuery, ProtectedSourceChecker, RegistryAuthority, ReservedAccountGuard,
};
