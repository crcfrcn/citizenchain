//! 身份主体公共边界。
//!
//! `subjects` 只承接所有身份 ID 共有的索引、详情入口与统一查询边界。
//! 公权机构业务放 `gov`,私权机构业务放 `private`,公民仍放 `citizens`。

pub(crate) mod admin;
pub(crate) mod chain_multisig_info;
pub(crate) mod http;
pub(crate) mod model;
pub(crate) mod photos;
pub(crate) mod registration;
pub(crate) mod schema;
pub(crate) mod service;
pub(crate) mod unincorporated_org;

pub use model::{
    CreateInstitutionInput, Institution, InstitutionAccount, InstitutionListFilter,
    InstitutionListRow, EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE,
    EDUCATION_TYPE_NATIONAL_CITIZEN_EDU_COMMITTEE,
};
