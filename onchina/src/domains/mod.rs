//! 业务域:公权(gov)、私权(private)、公民(citizens)、机构资料库(docs)聚合父模块。
//!
//! 面向不同主体类别的业务 handler 同属「业务域」,聚合于此;各子模块保留原职责,
//! 机构作用域校验统一走 institution::subjects::http。

pub(crate) mod address;
pub(crate) mod citizens;
pub(crate) mod docs;
pub(crate) mod genesis_projection;
pub(crate) mod gov;
/// 立法与表决域（业务提案 / 代表机构表决 / 大屏只读）。
pub(crate) mod legislation;
/// 平台会员价格治理域；只构造统一投票提案交易，不实现投票流程。
pub(crate) mod membership;
pub(crate) mod private;
pub(crate) mod projection;
