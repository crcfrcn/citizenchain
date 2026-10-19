//! 注册局机构登记与办理。私权机构登记和私权机构自身治理分别归属不同模块。
#[path = "government.rs"]
pub(crate) mod government;
#[path = "private.rs"]
pub(crate) mod private;

#[path = "registration.rs"]
pub(crate) mod registration;
