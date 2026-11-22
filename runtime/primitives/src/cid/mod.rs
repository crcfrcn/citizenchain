//! 组织CID静态码表、号码生成与校验及内置机构派生。

#[path = "../china/mod.rs"]
pub mod china;
pub mod code;
pub mod generator;
pub mod number;
pub mod official_derive;
pub mod official_template;
pub mod seed;
