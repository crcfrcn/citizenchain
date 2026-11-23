//! 阶段、身份绑定、管理员写入及查询接口。
use crate::*;

/// 运行期强制门控注入。链进入 Operation 期后 `required_admin_elements` 才生效。
///
/// 由 runtime 用 genesis 的 `Phase` 实现注入(仿 `DeveloperUpgradeCheck` 范式)。
/// 定义在 adminbase 而非 genesis:后者反向依赖各 admin/entity pallet,
/// 定义于此可让 publicadmin/privateentity 在 Config 约束引用而不成环。
pub trait ChainPhaseCheck {
    /// 链是否已进入运行期(Operation)。Genesis 返回 false。
    fn is_operation() -> bool;
}

/// 测试 no-op 默认:恒 Genesis(放行),供不关心相位的单测 mock 直接用 `()`。
/// 生产 runtime 必显式注入 `GenesisPallet`,不用本默认。
impl ChainPhaseCheck for () {
    fn is_operation() -> bool {
        false
    }
}

/// 个人多签管理员集合生命周期写入口。
///
/// 个人多签创建、注销等业务 pallet 只能通过此 trait 请求 personaladmin
/// 写入 Pending/Active/Closed，机构管理员不使用本生命周期模型。
pub trait AdminAccountLifecycle<AccountId, AdminItem = AccountId> {
    // 个人多签待激活记录必须逐字段绑定提案、主体和创建人，接口形状属于跨 pallet 契约。
    #[allow(clippy::too_many_arguments)]
    fn create_pending_admin_account_for_proposal(
        proposal_id: u128,
        module_tag: &[u8],
        personal_account_id: AccountId,
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<AdminItem>,
        creator_account_id: AccountId,
    ) -> DispatchResult;

    fn activate_admin_account_for_proposal(
        proposal_id: u128,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;

    fn remove_pending_admin_account_for_proposal(
        proposal_id: u128,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;

    fn close_admin_account_for_proposal(
        proposal_id: u128,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;
}

/// 机构管理员集合写入口。
///
/// 机构的来源、岗位和任职全部由 entity 表达，因此该接口不接收 `creator_account_id`，也不承担
/// 个人多签的创建语义。公权与私权 entity 只通过显式增删改操作原子写入签名钱包集合。
pub trait InstitutionAdminLifecycle<AccountId, AdminItem = Admin<AccountId>> {
    /// 登记钱包操作显式区分，新增/删除不写岗位人员，更换保留原岗位授权引用。
    fn update_institution_wallets(
        cid: Vec<u8>,
        code: InstitutionCode,
        added: Vec<AdminItem>,
        removed: Vec<(AccountId, u64)>,
        replaced: Vec<crate::InstitutionAdminWalletChange<AccountId>>,
    ) -> DispatchResult;

    /// 仅基准externalities构造夹具；正式构建不存在全量替换接口。
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_institution_admins(
        module_tag: &[u8],
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<AdminItem>,
    ) -> DispatchResult;
}

/// 机构管理员集合统一查询口。
///
/// 机构身份只使用 CID；账户地址不能作为本 trait 的查询 key。
pub trait InstitutionAdminQuery<AccountId> {
    fn institution_admins_exist(institution_code: InstitutionCode, cid_number: &[u8]) -> bool;

    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId,
    ) -> bool;

    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<AccountId>>;

    fn institution_admins_len(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32>;

    /// 验证当前真实钱包及其在用版本；返回值始终等于caller。
    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &AccountId,
    ) -> Option<AccountId>;

    /// 已批准岗位签名授权所引用的原记录。不是管理员身份，也不修改人员任职。
    fn resolve_role_authorization(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &AccountId,
    ) -> Option<AccountId>;

    /// 是否仍有当前有效钱包代表该已批准岗位授权引用。
    fn has_role_authorization(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        reference: &AccountId,
    ) -> bool;
}

impl<AccountId> InstitutionAdminQuery<AccountId> for () {
    fn institution_admins_exist(_institution_code: InstitutionCode, _cid_number: &[u8]) -> bool {
        false
    }

    fn is_institution_admin(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn institution_admins(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn institution_admins_len(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<u32> {
        None
    }

    fn resolve_admin_account(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _caller: &AccountId,
    ) -> Option<AccountId> {
        None
    }
    fn resolve_role_authorization(
        _code: InstitutionCode,
        _cid: &[u8],
        _caller: &AccountId,
    ) -> Option<AccountId> {
        None
    }
    fn has_role_authorization(_code: InstitutionCode, _cid: &[u8], _reference: &AccountId) -> bool {
        false
    }
}

/// 管理员资料的个人身份真源查询；不作为机构管理员钱包解析器。
///
/// 实现只能读取 `identity` 真源；publicadmin 不得自行生成或修正公民 CID。
pub trait CitizenIdentityBindingQuery<AccountId> {
    fn matches_citizen_account(cid_number: &[u8], account: &AccountId) -> bool;
    /// 读取身份资料的现有校验接口；不得用于推导机构签名钱包。
    fn current_account(_cid_number: &[u8]) -> Option<AccountId> {
        None
    }
}

impl<AccountId> CitizenIdentityBindingQuery<AccountId> for () {
    fn matches_citizen_account(_cid_number: &[u8], _account: &AccountId) -> bool {
        false
    }
}

/// 个人多签管理员集合查询口。
///
/// 机构管理员已经使用 CID 专用查询；本 trait 只服务个人多签账户。
pub trait AdminAccountQuery<AccountId> {
    fn active_admin_account_exists(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> bool;

    fn is_active_account_admin(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
        who: &AccountId,
    ) -> bool;

    fn active_account_admins(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<AccountId>>;

    /// 返回个人多签完整管理员人员记录；授权调用方仍只能比较账户。
    fn active_account_admin_records(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<Admin<AccountId>>>;

    fn active_account_admins_len(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<u32>;

    fn pending_account_exists_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_pending_account_admin_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn pending_account_admins_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn pending_account_admin_records_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn pending_account_admins_len_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }
}

impl<AccountId> AdminAccountQuery<AccountId> for () {
    fn active_admin_account_exists(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_active_account_admin(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn active_account_admins(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn active_account_admin_records(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn active_account_admins_len(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }

    fn pending_account_exists_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_pending_account_admin_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn pending_account_admins_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn pending_account_admin_records_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn pending_account_admins_len_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }
}

/// 按实际钱包查询机构授权状态；个人CID与岗位人员记录不参与账户解析。
pub trait InstitutionAdminWalletQuery<AccountId> {
    /// 读取已持久化的状态（含退役版本）；未初始化明确返回 None，不猜测旧锚点或个人绑定。
    fn institution_wallet_state(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &AccountId,
    ) -> Option<InstitutionAdminWalletState<AccountId>>;

    /// 返回仍在本机构名册且具有有效版本的实际钱包；缺记录明确拒绝。
    fn current_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &AccountId,
    ) -> Option<AccountId>;
}

/// 受控机构入口共用的实际钱包变更内核。登记随名册写齐，不提供补建或初始化操作。
pub trait InstitutionAdminWalletLifecycle<AccountId> {
    fn change_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        change: InstitutionAdminWalletChange<AccountId>,
    ) -> DispatchResult;

    /// 在管理员已移出名册后退役，保留版本；可用于整个名册已关闭后的内部清理。
    fn retire_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: AccountId,
        expected_revision: u64,
    ) -> DispatchResult;
}

/// 机构新钱包接收证明的账户验签注入；不复用个人CID换绑入口。
pub trait InstitutionWalletSignatureVerifier<AccountId> {
    fn verify(account: &AccountId, payload: &[u8], proof: &[u8]) -> bool;
}
