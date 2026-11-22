//! 组织当前机构工作台的类型和前端可渲染清单。

#[path = "kind.rs"]
mod kind;
#[path = "manifest.rs"]
pub mod manifest;
pub use crate::modules::model;

pub use manifest::build_institution_workspace;
pub(crate) use model::InstitutionWorkspace;
pub use model::WorkspaceModule;
