//! 机构治理 SCALE call-data 编码器。
//!
//! OnChina 只构造裸 call data，不提交 extrinsic。旧机构直接创建 call 5 已关闭；
//! 本模块只保留现存机构治理和注册局管理员登记调用。

use codec::{Compact, Encode};

/// PublicManage pallet 索引。
pub const PUBLIC_MANAGE_PALLET_INDEX: u8 = 30;
/// PrivateManage pallet 索引。
pub const PRIVATE_MANAGE_PALLET_INDEX: u8 = 31;
/// 机构内部治理提案 call index。
pub const PROPOSE_INSTITUTION_GOVERNANCE_CALL_INDEX: u8 = 8;
/// 注册局登记机构管理员集合 call index。
pub const REGISTER_INSTITUTION_ADMINS_CALL_INDEX: u8 = 9;
/// 发起「新增机构自定义账户」提案 call index(公私权同为 7)。
pub const PROPOSE_ADD_INSTITUTION_ACCOUNT_CALL_INDEX: u8 = 7;
/// 发起「关闭机构自定义账户」提案 call index(公私权同为 1)。
pub const PROPOSE_CLOSE_INSTITUTION_CALL_INDEX: u8 = 1;

/// 按机构码派生机构管理目标 pallet。
pub fn institution_manage_pallet_index(institution_code: &[u8; 4]) -> u8 {
    if primitives::cid::code::is_private_legal_code(institution_code) {
        PRIVATE_MANAGE_PALLET_INDEX
    } else {
        PUBLIC_MANAGE_PALLET_INDEX
    }
}

/// 公私权管理员现统一 `Admin` 结构；仍按目标机构类型分流到公权/私权 pallet。
#[derive(Debug, Clone)]
pub enum InstitutionAdminsPayload {
    Public(Vec<adminbase::Admin<[u8; 32]>>),
    Private(Vec<adminbase::Admin<[u8; 32]>>),
}

/// `propose_institution_governance` 完整参数。
///
/// 机构操作 = 发起管理员使用签名钱包直接冷签一笔普通 extrinsic;call 不嵌独立凭证
/// 签名/公钥/nonce/作用域,授权由 runtime 在 origin 处以 `is_institution_admin` + 岗位码校验。
#[derive(Debug, Clone)]
pub struct ProposeInstitutionGovernanceArgs {
    pub cid_number: Vec<u8>,
    /// `entitybase::InstitutionGovernanceAction` 的 SCALE 字节。
    pub governance_action: Vec<u8>,
    pub institution_code: [u8; 4],
    pub actor_cid_number: Vec<u8>,
    /// 发起人当前任职的机构岗位码；runtime 据此校验业务提案权限。
    pub proposer_role_code: Vec<u8>,
}

/// `register_institution_admins` 完整参数。
///
/// 授权由 runtime 在 origin 处以 `can_register_institution_origin` 校验(签名者是注册局
/// 在册管理员 + 对目标机构有登记权),call 不嵌独立凭证签名/公钥/nonce/作用域。
#[derive(Debug, Clone)]
pub struct RegisterInstitutionAdminsArgs {
    pub cid_number: Vec<u8>,
    pub admins: InstitutionAdminsPayload,
    pub institution_code: [u8; 4],
    pub actor_cid_number: Vec<u8>,
    /// 当前注册局操作人的任职岗位；由现有链端权限规则最终核验。
    pub actor_role_code: Vec<u8>,
}

/// `propose_add_institution_account` 完整参数(runtime call index 7)。
///
/// 机构本机构任职人发起「新增自定义命名账户」内部投票提案。授权由 runtime 在 origin 处以
/// `is_institution_admin`(本机构管理员)+ `proposer_role_code` 岗位码校验;call 不嵌独立
/// 凭证/签名/公钥/nonce。`institution_code` 只用于本端选 pallet(公 30 / 私 31),不进 call。
#[derive(Debug, Clone)]
pub struct ProposeAddInstitutionAccountArgs {
    pub cid_number: Vec<u8>,
    /// 待新增账户名列表;字节与链端 `BoundedVec<AccountNameOf, MaxInstitutionAccounts>` 对齐。
    pub account_names: Vec<Vec<u8>>,
    pub institution_code: [u8; 4],
    pub proposer_role_code: Vec<u8>,
}

/// `propose_close_*_institution` 完整参数(runtime call index 1)。
///
/// 机构本机构任职人发起「关闭自定义命名账户」内部投票提案。关闭执行时余额扫入
/// `beneficiary_account_id`(本端固定填本机构主账户),费用从费用账户扣。授权同上由 runtime
/// 在 origin 处以 `is_institution_admin` + `proposer_role_code` 岗位码校验。
#[derive(Debug, Clone)]
pub struct ProposeCloseInstitutionArgs {
    pub actor_cid_number: Vec<u8>,
    pub proposer_role_code: Vec<u8>,
    /// 待关闭账户的链上多签地址(32 字节 AccountId)。
    pub institution_account_id: [u8; 32],
    /// 关闭后余额受益账户(32 字节 AccountId,本端固定为本机构主账户)。
    pub beneficiary_account_id: [u8; 32],
    pub institution_code: [u8; 4],
}

fn encode_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend(Compact(value.len() as u32).encode());
    out.extend_from_slice(value);
}

/// 构造与 runtime 公权 `BoundedVec<Admin>` 或私权
/// `BoundedVec<Admin>` 完全一致的签名与 call 载荷。
pub fn encode_admins_payload(admins: &InstitutionAdminsPayload) -> Vec<u8> {
    match admins {
        InstitutionAdminsPayload::Public(admins) => admins.encode(),
        InstitutionAdminsPayload::Private(admins) => admins.encode(),
    }
}

/// QR 链动作码：`(pallet_index << 8) | call_index`。
pub const fn chain_action_code(pallet_index: u8, call_index: u8) -> u16 {
    ((pallet_index as u16) << 8) | call_index as u16
}

/// 一条待冷签链调用。
pub struct ChainCall {
    pub action: u16,
    pub call_data: Vec<u8>,
}

/// 编码机构内部治理提案调用。字段顺序与 runtime call index 8 完全一致。
pub fn encode_propose_institution_governance(args: &ProposeInstitutionGovernanceArgs) -> ChainCall {
    let pallet_index = institution_manage_pallet_index(&args.institution_code);
    let mut out = vec![pallet_index, PROPOSE_INSTITUTION_GOVERNANCE_CALL_INDEX];

    encode_bytes(&mut out, &args.cid_number);
    out.extend_from_slice(&args.governance_action);
    encode_bytes(&mut out, &args.actor_cid_number);
    encode_bytes(&mut out, &args.proposer_role_code);

    ChainCall {
        action: chain_action_code(pallet_index, PROPOSE_INSTITUTION_GOVERNANCE_CALL_INDEX),
        call_data: out,
    }
}

/// 编码注册局登记机构管理员集合调用。字段顺序与 runtime call index 9 完全一致。
pub fn encode_register_institution_admins(args: &RegisterInstitutionAdminsArgs) -> ChainCall {
    let pallet_index = institution_manage_pallet_index(&args.institution_code);
    let mut out = vec![pallet_index, REGISTER_INSTITUTION_ADMINS_CALL_INDEX];

    encode_bytes(&mut out, &args.cid_number);
    out.extend(encode_admins_payload(&args.admins));
    encode_bytes(&mut out, &args.actor_cid_number);
    encode_bytes(&mut out, &args.actor_role_code);

    ChainCall {
        action: chain_action_code(pallet_index, REGISTER_INSTITUTION_ADMINS_CALL_INDEX),
        call_data: out,
    }
}

/// 编码「新增机构自定义账户」提案调用。字段顺序与 runtime call index 7 完全一致:
/// `cid_number` → `account_names: Vec<Vec<u8>>` → `proposer_role_code`。
pub fn encode_propose_add_institution_account(
    args: &ProposeAddInstitutionAccountArgs,
) -> ChainCall {
    let pallet_index = institution_manage_pallet_index(&args.institution_code);
    let mut out = vec![pallet_index, PROPOSE_ADD_INSTITUTION_ACCOUNT_CALL_INDEX];

    encode_bytes(&mut out, &args.cid_number);
    // BoundedVec<AccountNameOf, _> 与 Vec<Vec<u8>> 的 SCALE 一致:Compact(个数) ++ 逐个 (Compact(长度) ++ 字节)。
    out.extend(Compact(args.account_names.len() as u32).encode());
    for name in &args.account_names {
        encode_bytes(&mut out, name);
    }
    encode_bytes(&mut out, &args.proposer_role_code);

    ChainCall {
        action: chain_action_code(pallet_index, PROPOSE_ADD_INSTITUTION_ACCOUNT_CALL_INDEX),
        call_data: out,
    }
}

/// 编码「关闭机构自定义账户」提案调用。字段顺序与 runtime call index 1 完全一致:
/// `actor_cid_number` → `proposer_role_code` → `institution_account_id` → `beneficiary_account_id`。
/// 两个 AccountId 均为 32 字节定长数组(SCALE 无长度前缀)。
pub fn encode_propose_close_institution(args: &ProposeCloseInstitutionArgs) -> ChainCall {
    let pallet_index = institution_manage_pallet_index(&args.institution_code);
    let mut out = vec![pallet_index, PROPOSE_CLOSE_INSTITUTION_CALL_INDEX];

    encode_bytes(&mut out, &args.actor_cid_number);
    encode_bytes(&mut out, &args.proposer_role_code);
    out.extend_from_slice(&args.institution_account_id);
    out.extend_from_slice(&args.beneficiary_account_id);

    ChainCall {
        action: chain_action_code(pallet_index, PROPOSE_CLOSE_INSTITUTION_CALL_INDEX),
        call_data: out,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/institutions_chain_call.rs"]
mod tests;
