//! 声明供节点升级守卫查询管理员强制策略的Runtime API。

sp_api::decl_runtime_apis! {
    pub trait AdminPolicyApi {
        /// 个人多签管理员是否被强制要求提供公民 CID。**死规则:永为 `false`。**
        fn personal_multisig_cid_mandated() -> bool;
    }
}
