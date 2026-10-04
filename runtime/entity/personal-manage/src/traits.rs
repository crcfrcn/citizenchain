//! 个人多签账户生命周期查询 trait,供 multisig-transfer / runtime config 等下游调用。
//!
//! `PersonalMultisigQuery` 与 public-manage/private-manage 的 `InstitutionMultisigQuery`
//! 是平行 trait,multisig-transfer 在转账治理时按"先问 personal、再问 institution"
//! union 查询,任意机构账户(主/费用/自创)经 public-manage/private-manage 路径反查到
//! admin 配置;个人多签账户经 personal-manage 判断账户状态,再经 personal-admins 读取管理员。

use primitives::multisig::MultisigConfigSnapshot;

/// 个人多签账户管理员配置查询。
pub trait PersonalMultisigQuery<AccountId> {
    /// 输入个人多签账户,返回 admin 配置快照(管理员列表 + threshold + admins_len)。
    /// 地址不属于个人多签时返回 None;multisig-transfer 据此 union 跳到机构查询。
    fn lookup_admin_config(addr: &AccountId) -> Option<MultisigConfigSnapshot<AccountId>>;

    /// 判断地址是否为已激活(Active)的个人多签账户。
    fn is_active(addr: &AccountId) -> bool;
}

/// 默认空实现(测试 mock 与 public-manage/private-manage 配置零依赖时使用)。
impl<AccountId> PersonalMultisigQuery<AccountId> for () {
    fn lookup_admin_config(_addr: &AccountId) -> Option<MultisigConfigSnapshot<AccountId>> {
        None
    }
    fn is_active(_addr: &AccountId) -> bool {
        false
    }
}
