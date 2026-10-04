//! 管理员要素强制策略 Runtime API。
//!
//! 供节点升级守卫(`node_guard::check_candidate_runtime`)查询候选 runtime 的
//! 分层强制策略。当前只暴露「个人多签是否被强制要求提供公民 CID」——**死规则永为
//! `false`**;任何返回 `true`(强制)或移除本 API 的候选 runtime 都会被节点守卫判为
//! `KnownBad`,从节点二进制层锁死「个人多签禁强制 CID」,防 runtime 升级篡改。

sp_api::decl_runtime_apis! {
    pub trait AdminPolicyApi {
        /// 个人多签管理员是否被强制要求提供公民 CID。**死规则:永为 `false`。**
        fn personal_multisig_cid_mandated() -> bool;
    }
}
