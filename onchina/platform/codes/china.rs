//! 组织随包SQLite行政区划真源及行政范围查询。

#[path = "china_admin.rs"]
pub mod admin;
#[path = "china_model.rs"]
pub mod model;
#[path = "china_store.rs"]
mod store;

pub(crate) use store::area_display_names;
pub use store::{
    area_name_by_codes, china_sqlite_hash, city_code_by_name, province_code_by_name, provinces,
    town_code_by_name, town_exists, with_china_connection,
};
