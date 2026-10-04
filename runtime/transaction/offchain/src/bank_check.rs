//! 清算行合法性判定。
//!
//!
//! - 清算行(L2)= subject_property 为 S(私法人)或 F(非法人)的私权机构。
//! - 清算行在身份注册局注册时生成 cid_number,并登记主账户、费用账户等机构账户。
//! - 本模块判定:某个地址能否作为"可被 L3 绑定的清算行主账户"。
//!
//! **解耦设计**:bank_check 不直接依赖具体实体生命周期 pallet,而是通过
//! `CidAccountQuery` trait 抽象机构登记表。runtime 层实现该 trait(内部委托
//! 给 runtime 聚合查询),测试层可用 `()` 空实现或 mock,从
//! 而避免 pallet 之间形成强耦合。

use frame_support::ensure;
use sp_std::vec::Vec;

use crate::{Config, Error};
// 常量
/// 新版 CID 字符串 `R5-K3P1C1-N9-D4` 中 K1 的字节位置。
pub const CID_K1_INDEX: usize = 6;
/// 新版 CID 字符串第一段 R5 后的分隔符位置。
pub const CID_R5_SEPARATOR_INDEX: usize = 5;

/// 清算行"主账户"名称(字节形式,与身份注册局生成时逐字节一致)。
pub const ACCOUNT_NAME_MAIN: &[u8] = primitives::account_derive::RESERVED_NAME_MAIN;
/// 清算行"费用账户"名称。
pub const ACCOUNT_NAME_FEE: &[u8] = primitives::account_derive::RESERVED_NAME_FEE;
/// 清算行"清算账户"名称(承载 L2 存款准备金,充值/提现/结算/偿付的唯一资金池)。
pub const ACCOUNT_NAME_CLEARING: &[u8] = primitives::account_derive::RESERVED_NAME_CLEARING;
// 机构登记表查询抽象
/// 机构登记表查询抽象。
///
/// 运行时由 `InstitutionAccounts` 正向真源、`AccountRegisteredCid` 反向索引与
/// `ClearingBankNodes` 组合实现。测试可用 `()` 或 mock。
pub trait CidAccountQuery<AccountId> {
    /// 地址 → (cid_number 字节, account_name 字节)。未登记返回 None。
    fn account_info(addr: &AccountId) -> Option<(Vec<u8>, Vec<u8>)>;
    /// (cid_number, account_name) → 地址。未登记返回 None。
    fn find_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId>;
    /// 该地址是否存在于机构账户正反索引中。
    fn account_exists(addr: &AccountId) -> bool;
    /// CID、岗位码、签名账户是否同时拥有指定清算业务动作的发起权限。
    fn is_institution_role_authorized(
        cid_number: &[u8],
        role_code: &[u8],
        who: &AccountId,
        action_code: u32,
    ) -> bool;
    /// 清算行资格白名单判定。
    ///
    /// 资格唯二:`SFGF` 私法人股份公司本身,以及**父级机构码为 `SFGF` 的 `UNIN`
    /// 非法人组织**(股份公司的非法人分支机构)。其余机构一律无资格。
    ///
    /// 链上不保存机构类型和所属法人元数据,故本方法只确认地址属于已登记的 CID
    /// 机构账户;**真正的硬约束是「清算账户已派生」**(见 `ensure_can_be_bound` 第 7 条)
    /// —— 清算账户只对上述唯二资格机构派生,单源
    /// `primitives::institution_constraints::required_protocol_account_kinds`。
    fn is_clearing_bank_eligible(addr: &AccountId) -> bool;
    /// 节点是否已声明为清算行节点。
    ///
    /// 链上 `ClearingBankNodes` storage 由 cid_number 索引;此方法接受主账户
    /// 地址参数,内部由实现层反查 cid_number 后判定。
    fn is_registered_clearing_node(bank: &AccountId) -> bool;
}

/// 测试用 no-op 默认实现:一律返回未登记 / 未激活 / 无岗位权限 / 不合资格 / 未声明节点。
impl<AccountId> CidAccountQuery<AccountId> for () {
    fn account_info(_addr: &AccountId) -> Option<(Vec<u8>, Vec<u8>)> {
        None
    }
    fn find_account(_cid_number: &[u8], _account_name: &[u8]) -> Option<AccountId> {
        None
    }
    fn account_exists(_addr: &AccountId) -> bool {
        false
    }
    fn is_institution_role_authorized(
        _cid_number: &[u8],
        _role_code: &[u8],
        _who: &AccountId,
        _action_code: u32,
    ) -> bool {
        false
    }
    fn is_clearing_bank_eligible(_addr: &AccountId) -> bool {
        false
    }
    fn is_registered_clearing_node(_bank: &AccountId) -> bool {
        false
    }
}
// 内部辅助
/// 判定 CID 编码字符串的 K1 主体属性属于"私权机构"(S 或 F)。
///
/// 直接对目标态 `R5-K3P1C1-N9-D4` 做字节判定,不依赖公民身份模块或 CID 后端。
fn subject_property_is_private_institution(cid_bytes: &[u8]) -> bool {
    if cid_bytes.len() <= CID_K1_INDEX || cid_bytes.get(CID_R5_SEPARATOR_INDEX) != Some(&b'-') {
        return false;
    }
    matches!(cid_bytes[CID_K1_INDEX], b'S' | b'F')
}
// 对外 API
/// 严格校验:某地址可作为"清算行主账户"被 L3 绑定。
///
/// 7 重校验,任一失败即拒绝:
/// 1. 在链上 `AccountRegisteredCid` 有机构登记
/// 2. `account_name` 段等于 "主账户"
/// 3. K1 ∈ {S, F}(字节级主体属性判定)
/// 4. 对应 `InstitutionAccounts.status == Active`
/// 5. **资格白名单**:由身份注册局在候选/注册信息接口筛选;链上通过
///    `CidAccountQuery::is_clearing_bank_eligible` 只确认该 CID 机构账户已 Active
/// 6. **节点已声明**:`cid_number ∈ ClearingBankNodes`,确保该机构已加入清算网络
///    (用户不能绑定到"机构合法但未声明清算行节点"的机构)
/// 7. **清算账户已派生**(S2-②):L2 资金落点必须存在。这是资格的**硬约束点** ——
///    清算账户只对 `SFGF` 股份公司、以及父级为 `SFGF` 的 `UNIN` 非法人分支机构派生
///    (单源 `primitives::institution_constraints::required_protocol_account_kinds`)
pub fn ensure_can_be_bound<T: Config>(cid_number: &[u8]) -> Result<(), Error<T>> {
    // 1. K1 主体属性:私法人/非法人(S/F),直接对 CID 字节判定。
    ensure!(
        subject_property_is_private_institution(cid_number),
        Error::<T>::NotPrivateInstitution
    );

    // 2. 由 CID 解析主账户,复用既有(按地址)的资格/节点校验。
    let main = T::CidAccountQuery::find_account(cid_number, ACCOUNT_NAME_MAIN)
        .ok_or(Error::<T>::NotRegisteredClearingBank)?;

    ensure!(
        T::CidAccountQuery::account_exists(&main),
        Error::<T>::ClearingBankAccountNotFound
    );

    // 3. 资格白名单(SFGF 股份公司 / 父级为 SFGF 的 UNIN 非法人分支);
    //    链上只确认机构账户已登记,资格的硬约束在第 5 步「清算账户已派生」。
    ensure!(
        T::CidAccountQuery::is_clearing_bank_eligible(&main),
        Error::<T>::NotEligibleForClearingBank
    );

    // 4. 必须已声明清算行节点。
    ensure!(
        T::CidAccountQuery::is_registered_clearing_node(&main),
        Error::<T>::ClearingBankNotRegisteredAsNode
    );

    // 5. (S2-②)必须已派生清算账户 —— L2 资金落点。这是资格在资金层的硬约束:
    //    清算账户只对 SFGF 股份公司、以及父级为 SFGF 的 UNIN 非法人分支机构派生,
    //    在此拦下可避免用户绑定成功却在首次充值时才失败。
    let _clearing = clearing_account_of::<T>(cid_number)?;

    Ok(())
}

/// 严格校验机构账户交易中的 `(actor_cid_number, institution_account_id)` 绑定关系。
///
/// 授权主体只能是 CID；账户只是该 CID 下被本次交易操作的具体账户。这里不接受
/// “由账户反推 CID 后继续执行”的回落路径，调用方传入的 CID、账户正向登记和
/// 账户反向登记必须完全一致。
pub fn ensure_institution_account<T: Config>(
    actor_cid_number: &[u8],
    institution_account_id: &T::AccountId,
    required_account_name: &[u8],
) -> Result<(), Error<T>> {
    let (institution_cid_number, institution_account_name) =
        T::CidAccountQuery::account_info(institution_account_id)
            .ok_or(Error::<T>::NotRegisteredClearingBank)?;
    ensure!(
        institution_cid_number.as_slice() == actor_cid_number,
        Error::<T>::InstitutionMismatch
    );
    ensure!(
        institution_account_name.as_slice() == required_account_name,
        Error::<T>::NotMainAccount
    );
    ensure!(
        T::CidAccountQuery::account_exists(institution_account_id),
        Error::<T>::ClearingBankAccountNotFound
    );
    Ok(())
}

/// 由清算行 CID 反查其"费用账户"地址(由 `settlement.rs` 使用)。
///
/// 用 `(cid_number, "费用账户")` 直接查询;清算行注册时未同步创建费用账户则返回
/// `FeeAccountNotFound`。
pub fn fee_account_of<T: Config>(cid_number: &[u8]) -> Result<T::AccountId, Error<T>> {
    T::CidAccountQuery::find_account(cid_number, ACCOUNT_NAME_FEE)
        .ok_or(Error::<T>::FeeAccountNotFound)
}

/// 由清算行 CID 反查其**清算账户**地址 —— L2 充值/提现/结算/偿付的唯一资金落点。
///
/// 主账户(`ACCOUNT_NAME_MAIN`)是机构身份锚,只在 `ensure_can_be_bound` /
/// `ensure_institution_account` 内按名解析,不再作为资金落点,故无独立取址原语。
///
/// 清算账户仅私法人股份公司(SFGF)注册时派生(约束表 `CORPORATION_PROTOCOL_ACCOUNT_KINDS`);
/// 未派生返回 `ClearingAccountNotFound`。
pub fn clearing_account_of<T: Config>(cid_number: &[u8]) -> Result<T::AccountId, Error<T>> {
    T::CidAccountQuery::find_account(cid_number, ACCOUNT_NAME_CLEARING)
        .ok_or(Error::<T>::ClearingAccountNotFound)
}

/// 判定某地址是"清算行的任一已登记账户"。
///
/// 供 `institution-asset` 的 `can_spend` / `is_protected` 实现时使用。
pub fn is_clearing_bank_account<T: Config>(addr: &T::AccountId) -> bool {
    match T::CidAccountQuery::account_info(addr) {
        Some((cid, _)) => {
            subject_property_is_private_institution(cid.as_slice())
                && T::CidAccountQuery::account_exists(addr)
        }
        None => false,
    }
}
// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_property_s_accepted() {
        assert!(subject_property_is_private_institution(
            b"AH001-SCB0V-123456789-2026"
        ));
    }

    #[test]
    fn subject_property_f_accepted() {
        assert!(subject_property_is_private_institution(
            b"AH001-FCB0P-123456789-2026"
        ));
    }

    #[test]
    fn subject_property_g_rejected() {
        assert!(!subject_property_is_private_institution(
            b"AH001-GCB0V-123456789-2026"
        ));
    }

    #[test]
    fn subject_property_too_short_rejected() {
        assert!(!subject_property_is_private_institution(b"AH001"));
    }

    #[test]
    fn noop_impl_returns_none_and_inactive() {
        let addr: [u8; 32] = [0u8; 32];
        assert!(<() as CidAccountQuery<[u8; 32]>>::account_info(&addr).is_none());
        assert!(<() as CidAccountQuery<[u8; 32]>>::find_account(
            b"AH001-SCB0V-123456789-2026",
            b"main"
        )
        .is_none());
        assert!(!<() as CidAccountQuery<[u8; 32]>>::account_exists(&addr));
    }
}
