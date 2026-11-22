//! 组织CID运行态生成、行政区查询及管理端元信息接口。

#[path = "admin.rs"]
pub mod admin;
#[path = "category.rs"]
pub mod category;
#[path = "china.rs"]
pub mod china;
#[path = "generator.rs"]
pub mod generator;
#[path = "model.rs"]
pub mod model;

pub use category::InstitutionCategory;
pub use generator::{generate_cid_number, GenerateCidInput};
pub use primitives::cid::code;
pub use primitives::cid::code::AdminLevel;
pub use primitives::cid::number::{parse_cid_number_parts, validate_cid_number_format};
