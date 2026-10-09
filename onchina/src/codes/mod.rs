//! onchina CID 运行态入口。
//!
//!
//! runtime primitives 负责 CID 字节协议与常量;onchina 负责 SQLite 行政区、
//! 当前年份、动态 UUID、数据库查重和管理端 API。业务模块只允许从 `crate::cid`
//! 引用 CID 能力,不得恢复顶层 `china` 或 `number` 模块。

pub(crate) mod admin;
pub mod category;
pub mod china;
pub mod generator;
pub(crate) mod model;

pub use category::InstitutionCategory;
pub use generator::{generate_cid_number, GenerateCidInput};
pub use primitives::cid::code;
pub use primitives::cid::code::AdminLevel;
pub use primitives::cid::number::{parse_cid_number_parts, validate_cid_number_format};
