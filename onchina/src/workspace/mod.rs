//! OnChina 机构工作台框架入口。
//!
//! `workspace` 只描述当前登录机构的操作台形态,不保存管理员授权真源。
//! 管理员资格仍由链上 active admins 集合判定,本模块只把登录态整理成前端可渲染的工作台清单。

mod kind;
mod manifest;
mod model;

pub(crate) use manifest::build_institution_workspace;
pub(crate) use model::{InstitutionWorkspace, WorkspaceModule};
