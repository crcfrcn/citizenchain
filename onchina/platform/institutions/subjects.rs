//! 身份主体公共边界。
//!
//! `subjects` 只承接所有身份 ID 共有的索引、详情入口与统一查询边界。
//! 注册局登记归 registry/institutions，私权机构自身业务归 private，公民办理归 registry/citizens。

#[path = "subjects_admin.rs"]
pub mod admin;
#[path = "subjects_chain_multisig_info.rs"]
pub(crate) mod chain_multisig_info;
#[path = "subjects_http.rs"]
pub mod http;
#[path = "subjects_model.rs"]
pub mod model;
#[path = "subjects_photos.rs"]
pub mod photos;
#[path = "subjects_schema.rs"]
pub mod schema;
#[path = "subjects_service.rs"]
pub mod service;
#[path = "subjects_unincorporated_org.rs"]
pub mod unincorporated_org;

pub use model::{
    CreateInstitutionInput, Institution, InstitutionAccount, InstitutionListFilter,
    InstitutionListRow, EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE,
    EDUCATION_TYPE_NATIONAL_CITIZEN_EDU_COMMITTEE,
};
