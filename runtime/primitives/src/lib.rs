//! 全链统一 primitives 常量与轻量类型。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod account_derive; // 账户地址派生
pub mod admin_policy; // 管理员要素强制策略 Runtime API(节点升级守卫用)
#[path = "../cid/mod.rs"]
pub mod cid; // CID 常量与号码协议
pub mod citizen_const; // 公民发行常量
pub mod constitution; // 宪法修改「章→档位」分类(第十九条)
pub mod core_const; // 核心常量
pub mod count_const; // 投票治理常量
pub mod fee_policy; // 费率规则常量
pub mod genesis; // 创世常量
pub mod governance_skeleton; // 固定治理骨架冻结规格(档 A)
pub mod institution_asset; // 机构账户资金操作白名单 trait 与动作枚举
pub mod institution_constraints; // 国家级单例身份与法定成员组成约束
pub mod multisig; // 多签共用 trait 与类型
pub mod pow_const; // 全节点铸块与发行常量
pub mod sign; // QR_V1 签名消息原语
