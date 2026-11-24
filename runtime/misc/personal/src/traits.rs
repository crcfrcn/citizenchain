//! 提供个人多签账户查询及节点升级守卫使用的公开Runtime API。

use primitives::multisig::MultisigConfigSnapshot;

/// 个人多签账户管理员配置查询。
pub trait PersonalMultisigQuery<AccountId> {
    /// 输入个人多签账户,返回 admin 配置快照(管理员列表 + threshold + admins_len)。
    /// 地址不属于个人多签时返回 None;multisig-transfer 据此 union 跳到机构查询。
    fn lookup_admin_config(addr: &AccountId) -> Option<MultisigConfigSnapshot<AccountId>>;

    /// 判断地址是否为已激活(Active)的个人多签账户。
    fn is_active(addr: &AccountId) -> bool;
}

/// 默认空实现(测试 mock 与 publicentity/privateentity 配置零依赖时使用)。
impl<AccountId> PersonalMultisigQuery<AccountId> for () {
    fn lookup_admin_config(_addr: &AccountId) -> Option<MultisigConfigSnapshot<AccountId>> {
        None
    }
    fn is_active(_addr: &AccountId) -> bool {
        false
    }
}

// 节点守卫的API名称及方法名保持原合同，定义归个人模块。
sp_api::decl_runtime_apis! {
    pub trait AdminPolicyApi {
        /// 个人多签成员不使用CID；Runtime必须返回false，节点据此拒绝违规升级。
        fn personal_multisig_cid_mandated() -> bool;
    }
}
