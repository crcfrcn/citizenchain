//! 组织六类私权机构的注册规则和登记接口。

#[path = "private_association.rs"]
pub(crate) mod association;
#[path = "private_common.rs"]
pub(crate) mod common;
#[path = "private_company.rs"]
pub(crate) mod company;
#[path = "private_corporation.rs"]
pub(crate) mod corporation;
#[path = "private_participants.rs"]
pub(crate) mod participants;
#[path = "private_partnership.rs"]
pub(crate) mod partnership;
#[path = "private_sole.rs"]
pub(crate) mod sole;
#[path = "private_welfare.rs"]
pub(crate) mod welfare;
