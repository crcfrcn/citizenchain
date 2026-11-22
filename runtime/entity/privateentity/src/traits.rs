//! 导出私权机构使用的实体共用生命周期和权限接口。

pub use entitybase::{
    AccountValidator, InstitutionCidQuery, InstitutionLegalRepresentativeQuery,
    InstitutionMultisigQuery, ProtectedSourceChecker, RegistryAuthority, ReservedAccountGuard,
};
