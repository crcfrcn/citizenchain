//! 机构域:机构主体(subjects)与机构账户(accounts)统一父模块。
//!
//! 机构主体登记/详情/资料库与机构账户读写同属「机构」业务域,聚合于此;
//! 两子模块各自保留原职责,跨子模块共享辅助仍走 subjects::http。

#[path = "accounts/mod.rs"]
pub(crate) mod accounts;
#[path = "admins/mod.rs"]
pub(crate) mod admins;
#[path = "subjects/mod.rs"]
pub(crate) mod subjects;
