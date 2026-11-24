//! 业务模块创建内部或联合投票提案的统一引擎入口。

use sp_runtime::DispatchError;

use crate::types::{InstitutionCode, VotePlanOf};

pub trait JointVoteEngine<AccountId> {
    fn create_joint_proposal_with_data(
        who: AccountId,
        actor_cid_number: sp_std::vec::Vec<u8>,
        vote_plan: VotePlanOf<AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError>;

    fn create_joint_proposal_with_data_and_object(
        who: AccountId,
        actor_cid_number: sp_std::vec::Vec<u8>,
        vote_plan: VotePlanOf<AccountId>,
        data: sp_std::vec::Vec<u8>,
        object_kind: u8,
        object_data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError>;
}

impl<AccountId> JointVoteEngine<AccountId> for () {
    fn create_joint_proposal_with_data(
        _who: AccountId,
        _actor_cid_number: sp_std::vec::Vec<u8>,
        _vote_plan: VotePlanOf<AccountId>,
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other("JointVoteEngineNotConfigured"))
    }

    fn create_joint_proposal_with_data_and_object(
        _who: AccountId,
        _actor_cid_number: sp_std::vec::Vec<u8>,
        _vote_plan: VotePlanOf<AccountId>,
        _data: sp_std::vec::Vec<u8>,
        _object_kind: u8,
        _object_data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other("JointVoteEngineNotConfigured"))
    }
}

/// 事项模块接入内部投票时,统一由投票引擎创建提案并返回真实提案 ID。
///
/// 内部投票是所有机构共用的投票程序，不代表所有机构都能发起每一种业务。
/// 投票引擎负责内部投票模式准入，转账、销毁、密钥变更等具体权限由对应业务模块
/// 校验；只有模式准入与业务权限同时通过，提案才可创建并执行。
///
/// 业务模块只能选择“提案语义”，不能传入“本次投票通过阈值”。
/// 阈值读取、快照、计票、自动赞成票与通过/否决判定全部归属投票引擎。
pub trait InternalVoteEngine<AccountId> {
    /// 个人普通阈值的唯一引擎校验；未配置引擎时拒绝，不猜测默认规则。
    fn validate_personal_threshold(_admins_len: u32, _threshold: u32) -> Result<(), DispatchError> {
        Err(DispatchError::Other("PersonalVoteEngineNotConfigured"))
    }

    /// 创建机构内部提案。机构唯一主体是 CID；具体资产账户仅作为执行上下文。
    fn create_institution_proposal_with_data(
        who: AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        execution_account_id: Option<AccountId>,
        subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        vote_plan: VotePlanOf<AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError>;

    /// 创建个人多签普通内部提案。个人多签没有机构 CID。
    fn create_personal_proposal_with_data(
        _who: AccountId,
        _personal_account: AccountId,
        _module_tag: &[u8],
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other("PersonalVoteEngineNotConfigured"))
    }

    /// 创建个人多签注销提案，按当前管理员快照要求全员通过。
    fn create_personal_lifecycle_proposal_with_data(
        _who: AccountId,
        _personal_account: AccountId,
        _module_tag: &[u8],
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other(
            "PersonalLifecycleVoteEngineNotConfigured",
        ))
    }

    /// 创建注册个人多签的特别内部投票提案。
    ///
    /// `dynamic_threshold` 是注册后普通业务使用的动态阈值配置，不是本次注册投票阈值。
    /// 本次注册投票阈值由投票引擎按 `admins.len()` 写全员通过快照。
    fn create_personal_account_create_proposal_with_data(
        _who: AccountId,
        _personal_account: AccountId,
        _admins: sp_std::vec::Vec<AccountId>,
        _dynamic_threshold: u32,
        _module_tag: &[u8],
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other(
            "RegisteredAccountCreateVoteEngineNotConfigured",
        ))
    }

    /// 创建个人多签管理员集合变更内部提案，仅供个人多签模块接入。
    ///
    /// 本次投票仍使用当前 active 阈值；`new_threshold` 只表示变更执行成功后
    /// 写入投票引擎的下一阶段动态阈值。
    fn create_personal_admin_change_proposal_with_data(
        _who: AccountId,
        _personal_account: AccountId,
        _new_admins_len: u32,
        _new_threshold: u32,
        _module_tag: &[u8],
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other(
            "AdminSetMutationVoteEngineNotConfigured",
        ))
    }

    /// 读取机构治理阈值。实现只能从 entity 真源读取，不得在投票引擎另建阈值状态。
    fn active_institution_threshold(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<u32> {
        None
    }

    /// 个人多签已激活动态阈值。
    fn active_personal_threshold(_personal_account: AccountId) -> Option<u32> {
        None
    }

    /// 读取指定个人多签提案的 pending 阈值；不存在时读取个人账户 active 阈值。
    fn configured_personal_threshold(
        _proposal_id: u128,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }
}

impl<AccountId> InternalVoteEngine<AccountId> for () {
    fn create_institution_proposal_with_data(
        _who: AccountId,
        _institution_code: InstitutionCode,
        _actor_cid_number: sp_std::vec::Vec<u8>,
        _execution_account: Option<AccountId>,
        _subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        _vote_plan: VotePlanOf<AccountId>,
        _data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Err(DispatchError::Other("InternalVoteEngineNotConfigured"))
    }
}
