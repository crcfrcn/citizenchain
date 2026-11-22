//! 导出公权机构实体、账户及登记权限的共用接口。

pub use entitybase::{
    AccountValidator, InstitutionCidQuery, InstitutionLegalRepresentativeQuery,
    InstitutionMultisigQuery, ProtectedSourceChecker, RegistryAuthority, ReservedAccountGuard,
};
