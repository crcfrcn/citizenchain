//! 业务域:公权(gov)、私权(private)、公民(citizens)、机构资料库(docs)聚合父模块。
//!
//! 面向不同主体类别的业务 handler 同属「业务域」,聚合于此;各子模块保留原职责,
//! 机构作用域校验统一走 institution::subjects::http。

#[path = "address/mod.rs"]
pub(crate) mod address;
#[path = "citizens/mod.rs"]
pub(crate) mod citizens;
#[path = "docs/mod.rs"]
pub(crate) mod docs;
#[path = "domains_genesis_projection.rs"]
pub(crate) mod genesis_projection;
#[path = "government/mod.rs"]
pub(crate) mod gov;
/// 立法与表决域（业务提案 / 代表机构表决 / 大屏只读）。
#[path = "legislation/mod.rs"]
pub(crate) mod legislation;
/// 平台会员价格治理域；只构造统一投票提案交易，不实现投票流程。
#[path = "membership/mod.rs"]
pub(crate) mod membership;
#[path = "private/mod.rs"]
pub(crate) mod private;
#[path = "domains_projection.rs"]
pub(crate) mod projection;
