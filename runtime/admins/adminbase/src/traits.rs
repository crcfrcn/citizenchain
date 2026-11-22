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
/// 个人多签的创建语义。公权与私权 entity 只通过本接口原子写入管理员人员集合。
pub trait InstitutionAdminLifecycle<AccountId, AdminItem = Admin<AccountId>> {
    /// 注册局或机构治理写入有效管理员人员集合；机构阈值由 entity 独立保存。
    fn set_institution_admins(
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

    /// 把调用者钱包解析为名册中的**规范账户**(canonical `account_id`，即快照键)。
    /// - 运行期(Operation)对**有 CID** 的管理员：按 identity 绑定解析
    ///   (`matches_citizen_account(cid, caller)`)，换绑钱包后新钱包解析到同一 canonical、
    ///   旧钱包不再匹配 →「换绑不掉权」；
    /// - 创世期，或**无 CID** 的管理员(私权非 LR / 个人多签)：按 `account_id` 直配；
    /// - 返回 `None` 表示 caller 不是该机构管理员。
    ///
    /// 授权链路统一先经本方法解析，再用返回的 canonical 匹配名册与任职，`account_id`
    /// 由此降为签名快照、CID 成授权锚点(相位门控与 Phase 2 字段强制同一开关)。
    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &AccountId,
    ) -> Option<AccountId>;
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
}

/// 公权管理员非空公民 CID 与账户绑定查询。
///
/// 实现只能读取 `identity` 真源；publicadmin 不得自行生成或修正公民 CID。
pub trait CitizenIdentityBindingQuery<AccountId> {
    fn matches_citizen_account(cid_number: &[u8], account: &AccountId) -> bool;
    /// 只供机构旧授权兼容初始化读取，不能改写个人CID绑定。
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

/// 机构钱包基础查询。状态记录不是独立的管理员资格来源，任职仍由名册和 entity 决定。
/// 机构激活后 `resolve_admin_account` 通过独立钱包状态解析，个人CID仍独立。
pub trait InstitutionAdminWalletQuery<AccountId> {
    fn institution_wallets_initialized(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> bool;
    /// 读取已持久化的状态（含退役版本）；未初始化明确返回 None，不猜测旧锚点或个人绑定。
    fn institution_wallet_state(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &AccountId,
    ) -> Option<InstitutionAdminWalletState<AccountId>>;

    /// 只返回仍在本机构名册中且已初始化的基础钱包；尚不参与现有登录/业务授权。
    fn current_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &AccountId,
    ) -> Option<AccountId>;
}

/// 仅运行时内部的钱包状态写入原语，没有对外 extrinsic。
///
/// 初始化的钱包必须由后续兼容接入层读取旧授权事实提供，不能接收客户端猜测值。
/// 本接口只校验状态；本人双签、机构投票及名册移除调用点在第2步接入，不能直接开放。
pub trait InstitutionAdminWalletLifecycle<AccountId> {
    /// 同一机构内原子采样旧授权并初始化；完成后不再回退个人钱包。
    fn ensure_institution_wallets(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
    ) -> DispatchResult;
    /// 首次初始化传 None；退役后重新录入必须传入保留的版本，再递增一次。
    fn initialize_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: AccountId,
        wallet: AccountId,
        expected_revision: Option<u64>,
    ) -> DispatchResult;

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
