//! 机构生命周期类型统一出口。
//!
//! 定义已上提 `entity-primitives` 单源(公权/私权 pallet 逐字段一致),本模块仅 re-export,
//! 保持 `crate::institution::types::*` 与对外 `private_manage::{...}` API 不变。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use entity_primitives::{
    CloseInstitutionAction, CreateInstitutionAccount, InstitutionAccountInfo, InstitutionInfo,
    InstitutionInitialAccount, RegisteredInstitution,
};

/// 新增机构自定义命名账户提案的业务数据(公权/私权镜像,存入投票引擎 ProposalData)。
///
/// 与 `CloseInstitutionAction` 完全对称:`do_propose_add_institution_account` 在发起时
/// 完成派生与逐项校验,把已派生好的 `(账户名, 账户地址)` 冻结进本载荷;投票通过后由
/// `execute_institution_add_account_with_finalizer` 重校验并落库。
/// 载荷携带 `Vec`,只作 ProposalData 明细,不进任何 storage 值,故不派生 `MaxEncodedLen`。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct AddInstitutionAccountAction<AccountId, CidNumber, AccountName> {
    /// 目标机构 CID:授权主体与账户归属都是本机构自身(actor == target)。
    pub actor_cid_number: CidNumber,
    /// 发起时已派生并逐项校验通过的 (账户名, 账户地址) 列表。
    pub derived: Vec<(AccountName, AccountId)>,
    /// 提案发起人账户(与 `CloseInstitutionAction.proposer_account_id` 对称,供落库事件署名)。
    pub proposer_account_id: AccountId,
}
