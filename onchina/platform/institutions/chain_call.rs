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
    Public(Vec<admin_primitives::Admin<[u8; 32]>>),
    Private(Vec<admin_primitives::Admin<[u8; 32]>>),
}

/// `propose_institution_governance` 完整参数。
///
/// 机构操作 = 发起管理员使用签名钱包直接冷签一笔普通 extrinsic;call 不嵌独立凭证
/// 签名/公钥/nonce/作用域,授权由 runtime 在 origin 处以 `is_institution_admin` + 岗位码校验。
#[derive(Debug, Clone)]
pub struct ProposeInstitutionGovernanceArgs {
    pub cid_number: Vec<u8>,
    /// `entity_primitives::InstitutionGovernanceAction` 的 SCALE 字节。
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
// 机构调用编码测试要求固定夹具可解码，断言式解包仅限本模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn private_admin(
        account_id: [u8; 32],
        family_name: &str,
        given_name: &str,
    ) -> admin_primitives::Admin<[u8; 32]> {
        admin_primitives::Admin {
            account_id,
            // Phase 1: 私权/个人多签管理员公民 CID 预留空。
            cid_number: Default::default(),
            family_name: family_name
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("family name fits"),
            given_name: given_name
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("given name fits"),
        }
    }

    fn public_admin(
        account_id: [u8; 32],
        cid_number: &str,
        family_name: &str,
        given_name: &str,
    ) -> admin_primitives::Admin<[u8; 32]> {
        admin_primitives::Admin {
            account_id,
            cid_number: cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("citizen cid fits"),
            family_name: family_name
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("family name fits"),
            given_name: given_name
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("given name fits"),
        }
    }

    #[test]
    fn admin_payload_encodes_account_cid_family_name_and_given_name() {
        let admins = vec![
            private_admin([1; 32], "张", "三"),
            private_admin([2; 32], "管理", "员"),
        ];
        let expected = admins
            .iter()
            .map(|admin| {
                (
                    admin.account_id,
                    admin.cid_number.clone(),
                    admin.family_name.clone(),
                    admin.given_name.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            encode_admins_payload(&InstitutionAdminsPayload::Private(admins)),
            expected.encode()
        );
    }

    #[test]
    fn public_admin_payload_encodes_account_cid_family_name_and_given_name() {
        let admins = vec![public_admin([3; 32], "CN220-CTZN2-198805200-2026", "", "")];
        let expected = admins
            .iter()
            .map(|admin| {
                (
                    admin.account_id,
                    admin.cid_number.clone(),
                    admin.family_name.clone(),
                    admin.given_name.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            encode_admins_payload(&InstitutionAdminsPayload::Public(admins)),
            expected.encode()
        );
    }

    #[test]
    fn governance_payload_matches_runtime_call_field_order() {
        let args = ProposeInstitutionGovernanceArgs {
            cid_number: b"LN001-SFAS-0001".to_vec(),
            governance_action: vec![0, 8, b'A'],
            institution_code: *b"SFAS",
            actor_cid_number: b"LN001-SFAS-0001".to_vec(),
            proposer_role_code: b"RFINANCE".to_vec(),
        };
        let encoded = encode_propose_institution_governance(&args);
        assert_eq!(&encoded.call_data[..2], &[31, 8]);
        assert_eq!(encoded.action, 0x1f08);
    }

    #[test]
    fn register_admins_payload_matches_runtime_call_field_order() {
        let args = RegisterInstitutionAdminsArgs {
            cid_number: b"LN001-SFAS-0001".to_vec(),
            admins: InstitutionAdminsPayload::Private(vec![
                private_admin([1; 32], "张", "三"),
                private_admin([2; 32], "李", "四"),
            ]),
            institution_code: *b"SFAS",
            actor_cid_number: b"LN001-FRG0-0001".to_vec(),
        };
        let encoded = encode_register_institution_admins(&args);
        assert_eq!(&encoded.call_data[..2], &[31, 9]);
        assert_eq!(encoded.action, 0x1f09);
    }

    #[test]
    fn add_account_payload_matches_runtime_call_field_order() {
        // CFIN(市财政)= 公权码 → PublicManage(30)。
        let args = ProposeAddInstitutionAccountArgs {
            cid_number: b"LN001-CFIN-0001".to_vec(),
            account_names: vec![
                "专项账户".as_bytes().to_vec(),
                "结算账户".as_bytes().to_vec(),
            ],
            institution_code: *b"CFIN",
            proposer_role_code: b"RFINANCE".to_vec(),
        };
        let encoded = encode_propose_add_institution_account(&args);
        assert_eq!(&encoded.call_data[..2], &[30, 7]);
        assert_eq!(encoded.action, 0x1e07);
        // 逐字节金标:pallet/call ++ SCALE(cid) ++ SCALE(Vec<Vec<u8>>) ++ SCALE(role)。
        let mut expected = vec![30u8, 7u8];
        expected.extend(args.cid_number.encode());
        expected.extend(args.account_names.encode());
        expected.extend(args.proposer_role_code.encode());
        assert_eq!(encoded.call_data, expected);
    }

    #[test]
    fn close_institution_payload_matches_runtime_call_field_order() {
        // CFIN(市财政)= 公权码 → PublicManage(30)。
        let args = ProposeCloseInstitutionArgs {
            actor_cid_number: b"LN001-CFIN-0001".to_vec(),
            proposer_role_code: b"RFINANCE".to_vec(),
            institution_account_id: [7u8; 32],
            beneficiary_account_id: [9u8; 32],
            institution_code: *b"CFIN",
        };
        let encoded = encode_propose_close_institution(&args);
        assert_eq!(&encoded.call_data[..2], &[30, 1]);
        assert_eq!(encoded.action, 0x1e01);
        // 逐字节金标:pallet/call ++ SCALE(actor_cid) ++ SCALE(role) ++ 32B ++ 32B(AccountId 无长度前缀)。
        let mut expected = vec![30u8, 1u8];
        expected.extend(args.actor_cid_number.encode());
        expected.extend(args.proposer_role_code.encode());
        expected.extend_from_slice(&args.institution_account_id);
        expected.extend_from_slice(&args.beneficiary_account_id);
        assert_eq!(encoded.call_data, expected);
    }

    #[test]
    fn private_institution_routes_add_and_close_to_pallet_31() {
        // SFAS = 私权码(见 primitives::cid::code::is_private_legal_code)→ PrivateManage(31)。
        let add = encode_propose_add_institution_account(&ProposeAddInstitutionAccountArgs {
            cid_number: b"LN001-SFAS-0001".to_vec(),
            account_names: vec![b"payroll".to_vec()],
            institution_code: *b"SFAS",
            proposer_role_code: b"ROWNER".to_vec(),
        });
        assert_eq!(&add.call_data[..2], &[31, 7]);
        assert_eq!(add.action, 0x1f07);
        let close = encode_propose_close_institution(&ProposeCloseInstitutionArgs {
            actor_cid_number: b"LN001-SFAS-0001".to_vec(),
            proposer_role_code: b"ROWNER".to_vec(),
            institution_account_id: [1u8; 32],
            beneficiary_account_id: [2u8; 32],
            institution_code: *b"SFAS",
        });
        assert_eq!(&close.call_data[..2], &[31, 1]);
        assert_eq!(close.action, 0x1f01);
    }
}
