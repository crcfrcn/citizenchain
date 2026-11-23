//! 公民身份、机构人员名册与岗位任职资格的只读提供者。

use crate::types::InstitutionCode;

/// 公民身份只读接口。投票引擎只能读链上公民身份模块的资格和人口数，
/// 不再接收注册局链下签发的人口快照或投票凭证。
pub trait CitizenIdentityReader<AccountId> {
    /// 读取 CID↔账户双向绑定、身份状态和 CID 状态全部有效的完整公民主体。
    fn citizen_subject(_who: &AccountId) -> Option<identity::CitizenSubject<AccountId>> {
        None
    }

    fn voting_subject(
        who: &AccountId,
        scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>>;
    fn candidate_subject(
        who: &AccountId,
        scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>>;

    /// 读取投票引擎生成快照所需的四级人口数据；日期未完整推进时必须返回 `None`。
    /// 身份模块不得创建、保存或绑定任何投票快照。
    fn population_data(
        scope: &identity::PopulationScope) -> Option<identity::PopulationData> {
        let _ = scope;
        None
    }

    /// 按投票引擎冻结的人口数据验证账户在建案时是否具备投票资格。
    fn voting_subject_at(
        _who: &AccountId,
        _population_data: &identity::PopulationData,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        None
    }

    /// FRAME benchmark 专用：写入一个同时具备投票和参选资格的账户，
    /// 并同步人口分母。生产调用路径不会调用此函数。
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_identity(_who: &AccountId, _scope: &identity::PopulationScope) {}
}

impl<AccountId> CitizenIdentityReader<AccountId> for () {
    fn voting_subject(
        _who: &AccountId,
        _scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        None
    }

    fn candidate_subject(
        _who: &AccountId,
        _scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        None
    }
}

/// 内部管理员动态提供器。
///
/// 机构管理员查询只用于当前业务入口确认真实签名钱包；机构投票资格只能来自
/// 提案冻结的钱包和版本。岗位提供器仅用于建案。个人多签继续使用独立管理员快照。
pub trait InternalAdminProvider<AccountId> {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId,
    ) -> bool;

    /// 仅在建案时将已批准岗位人员引用转换为同区块的实际管理员钱包及其版本。
    /// 缺失/重复数据必须返回None；旧提案投票不调用当前名单或此接口。
    fn institution_wallet_snapshot(
        _cid_number: &[u8],
        _references: &[AccountId],
    ) -> Option<sp_std::vec::Vec<(AccountId, u64)>> {
        None
    }

    /// FRAME benchmark 专用：把指定账户写成目标机构可解析的当前管理员。
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_institution_voter(_cid_number: &[u8], _voter: &AccountId) {}

    /// 读取机构治理阈值唯一真源。
    ///
    /// 阈值属于机构而不是管理员集合或投票引擎；生产实现必须路由到对应 entity
    /// 模块，投票引擎只在建案时读取并冻结快照。
    fn institution_threshold(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<u32> {
        None
    }

    /// 查询个人多签管理员权限。
    fn is_personal_admin(_personal_account: AccountId, _who: &AccountId) -> bool {
        false
    }

    /// 获取个人多签当前管理员列表。
    fn get_personal_admins(_personal_account: AccountId) -> Option<sp_std::vec::Vec<AccountId>> {
        None
    }

    /// 查询 Pending 个人多签管理员权限。仅供创建个人多签提案使用。
    fn is_pending_personal_admin(_personal_account: AccountId, _who: &AccountId) -> bool {
        false
    }

    /// 获取机构法定代表人(ADR-027 立法签署人)。
    /// 默认 None(个人账户/尚未任命);机构公开事实由 entity 的 `InstitutionInfo` 提供。
    fn legal_representative(_cid_number: &[u8]) -> Option<AccountId> {
        None
    }

    /// 获取护宪大法官成员集(ADR-027 修订:修宪最终否决,宪法第21条)。
    /// 护宪大法官归口国家司法院，生产读取 NJD `CONSTITUTION_GUARD` 岗位的
    /// 当前有效任职账户。立法投票模块要求成员数恰好 7 人，并按 4 名及以上
    /// 赞成判定修宪终审通过。
    fn constitution_guard_members() -> sp_std::vec::Vec<AccountId> {
        sp_std::vec::Vec::new()
    }

    /// 获取 Pending 个人多签管理员列表。
    fn get_pending_personal_admins(
        _personal_account: AccountId,
    ) -> Option<sp_std::vec::Vec<AccountId>> {
        None
    }
}

impl<AccountId> InternalAdminProvider<AccountId> for () {
    fn is_institution_admin(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _who: &AccountId,
    ) -> bool {
        false
    }
}

/// 机构岗位任职快照提供器。
///
/// 本接口只暴露岗位任职事实，不解释业务权限。业务模块必须在调用投票引擎前通过
/// `InstitutionRoleAuthorizationQuery` 完成“CID + 岗位码 + 业务动作”的授权校验。
pub trait InstitutionRoleProvider<AccountId> {
    /// 当前签名钱包是否具备指定岗位的有效授权；仅用于建案和当前业务。
    fn is_active_assignment(cid_number: &[u8], who: &AccountId, role_code: &[u8]) -> bool;

    /// 建案时读取该岗位的完整有效人员/授权引用，随后映射实际钱包及版本。
    /// 不得预先排除缺钱包的有效任职，否则会降低原分母；不用于旧提案资格复查。
    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8])
        -> sp_std::vec::Vec<AccountId>;
}

impl<AccountId> InstitutionRoleProvider<AccountId> for () {
    fn is_active_assignment(_cid_number: &[u8], _who: &AccountId, _role_code: &[u8]) -> bool {
        false
    }

    fn active_accounts_for_role(
        _cid_number: &[u8],
        _role_code: &[u8],
    ) -> sp_std::vec::Vec<AccountId> {
        sp_std::vec::Vec::new()
    }
}
