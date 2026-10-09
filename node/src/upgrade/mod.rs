//! 协议升级的 node 端后端实现。
//!
//! 本目录只承载 RuntimeUpgrade 业务:开发期直升、运行期提案升级、
//! 专用 call_data 编码和 Tauri 命令。签名响应校验仍复用治理公共签名底座。

pub(crate) mod call_data;
pub mod commands;
pub(crate) mod signing;
pub mod types;
