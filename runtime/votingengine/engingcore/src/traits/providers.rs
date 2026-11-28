//! 公民身份、机构人员名册与岗位任职资格的只读提供者。

use crate::types::InstitutionCode;

/// 公民身份只读接口。投票引擎只能读链上公民身份模块的资格和人口数，
/// 不再接收注册局链下签发的人口快照或投票凭证。
pub trait CitizenIdentityReader<AccountId> {
    /// 从当前签名钱包读取有效、双向一致的人员 CID，包括公民与居民。
    fn person_cid(_who: &AccountId) -> Option<crate::types::CidNumber> {
        None
    }

    /// 查询有效人员 CID 的当前签名钱包。
    fn person_account(_person_cid: &crate::types::CidNumber) -> Option<AccountId> {
        None
    }

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

/// 内部投票只读接口，返回实体的治理阈值及个人多签成员。
pub trait InternalVoteProvider<AccountId> {
    /// 基准专用：写入目标机构的测试岗位任职。
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

    /// 获取机构法定代表人(立法与宪法协议 立法签署人)。
    /// 默认 None(个人账户/尚未任命);机构公开事实由 entity 的 `InstitutionInfo` 提供。
    fn legal_representative(_cid_number: &[u8]) -> Option<AccountId> {
        None
    }

    /// 获取 Pending 个人多签管理员列表。
    fn get_pending_personal_admins(
        _personal_account: AccountId,
    ) -> Option<sp_std::vec::Vec<AccountId>> {
        None
    }
}

impl<AccountId> InternalVoteProvider<AccountId> for () {}

/// 机构岗位任职快照提供器。
///
/// 本接口只暴露岗位任职事实，不解释业务权限。业务模块必须在调用投票引擎前通过
/// `InstitutionRoleAuthorizationQuery` 完成“CID + 岗位码 + 业务动作”的授权校验。
pub trait InstitutionRoleProvider<AccountId> {
    /// 当前签名钱包是否具备指定岗位的有效授权；仅用于建案和当前业务。
    fn is_active_assignment(cid_number: &[u8], who: &AccountId, role_code: &[u8]) -> bool;

    /// 建案时读取该岗位完整有效人员 CID。
    /// 不得预先排除缺钱包的有效任职，否则会降低原分母；不用于旧提案资格复查。
    fn active_persons_for_role(cid_number: &[u8], role_code: &[u8])
        -> sp_std::vec::Vec<sp_std::vec::Vec<u8>>;
}

impl<AccountId> InstitutionRoleProvider<AccountId> for () {
    fn is_active_assignment(_cid_number: &[u8], _who: &AccountId, _role_code: &[u8]) -> bool {
        false
    }

    fn active_persons_for_role(
        _cid_number: &[u8],
        _role_code: &[u8],
    ) -> sp_std::vec::Vec<sp_std::vec::Vec<u8>> {
        sp_std::vec::Vec::new()
    }
}
