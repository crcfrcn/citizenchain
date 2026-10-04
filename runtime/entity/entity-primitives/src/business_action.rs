//! 机构业务动作与受保护创世岗位固定权限目录。
//!
//! `BusinessActionId` 是跨业务模块、创世和原生节点守卫共同解释的稳定协议，不能把
//! pallet/call 索引临时转换成权限，也不能由客户端自造字符串。这里仅冻结动作身份和
//! 创世固定岗位权限；业务使用哪个投票引擎、如何构造 `VotePlan` 仍由对应业务模块决定。

extern crate alloc;

use alloc::{vec, vec::Vec};

use primitives::{
    cid::{
        china::citizenchain::{
            is_citizenchain_foundation_identity, ROLE_CODE_GENESIS_PRODUCT_MANAGER,
            ROLE_CODE_GENESIS_PROGRAMMER,
        },
        code::{InstitutionCode, FRG, FSC, NJD, NRC, PRB, PRC, PROVINCE_CODE_INFOS},
    },
    governance_skeleton::{
        fixed_institution_by_identity, fixed_role_seats_by_identity,
        province_commissioner_role_code, ROLE_CODE_CHIEF_JUSTICE, ROLE_CODE_COMMITTEE_MEMBER,
        ROLE_CODE_CONSTITUTION_GUARD, ROLE_CODE_DEPUTY_CHIEF_JUSTICE, ROLE_CODE_DIRECTOR,
        ROLE_CODE_JUSTICE,
    },
    institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
};

use crate::RolePermissionOperation;

pub const MODULE_PUBLIC_MANAGE: &[u8] = b"pub-mgmt";
pub const MODULE_PRIVATE_MANAGE: &[u8] = b"pri-mgmt";
pub const MODULE_RUNTIME_UPGRADE: &[u8] = b"rt-upg";
pub const MODULE_RESOLUTION_ISSUANCE: &[u8] = b"res-iss";
pub const MODULE_RESOLUTION_DESTROY: &[u8] = b"res-dst";
pub const MODULE_GRANDPA_KEY_CHANGE: &[u8] = b"gra-key";
pub const MODULE_MULTISIG: &[u8] = b"multisig";
pub const MODULE_ONCHAIN_ISSUANCE: &[u8] = b"onc-iss";
pub const MODULE_OFFCHAIN: &[u8] = b"offchain";
pub const MODULE_LEGISLATION_YUAN: &[u8] = b"leg-yuan";
pub const MODULE_SQUARE_SUBSCRIPTION: &[u8] = b"sqr-sub";
/// 第 7 步接入投票前先冻结公民身份业务标签，禁止复用为其它业务。
pub const MODULE_CITIZEN_IDENTITY: &[u8] = b"cit-id";
/// 第 7 步接入投票前先冻结地址登记业务标签，禁止复用为其它业务。
pub const MODULE_ADDRESS_REGISTRY: &[u8] = b"addr-reg";
/// 第 6 步原子机构登记模块的预留稳定标签。
pub const MODULE_INSTITUTION_REGISTRATION: &[u8] = b"ins-reg";

/// 普通机构关闭；受保护创世机构仍由业务模块固定拒绝。
pub const ACTION_INSTITUTION_CLOSE: u32 = 2;
pub const ACTION_INSTITUTION_GOVERNANCE: u32 = 3;
pub const ACTION_RUNTIME_UPGRADE: u32 = 0;
pub const ACTION_RESOLUTION_ISSUANCE: u32 = 0;
pub const ACTION_RESOLUTION_DESTROY: u32 = 0;
/// GRANDPA 验证密钥丢失后的机构内部投票恢复。
pub const ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY: u32 = 0;
/// GRANDPA 验证密钥仍可用时，由本机构单个委员直接发起的双密钥签名更换。
pub const ACTION_GRANDPA_KEY_ROTATION: u32 = 1;
pub const ACTION_MULTISIG_TRANSFER: u32 = 0;
pub const ACTION_SAFETY_FUND_TRANSFER: u32 = 1;
pub const ACTION_FEE_SWEEP_TO_MAIN: u32 = 2;
pub const ACTION_ONCHAIN_ASSET_ISSUE: u32 = 0;
pub const ACTION_ONCHAIN_ASSET_MINT: u32 = 1;
pub const ACTION_ONCHAIN_ASSET_BURN: u32 = 2;
pub const ACTION_ONCHAIN_ASSET_CLOSE: u32 = 3;
pub const ACTION_ONCHAIN_ASSET_TRANSFER: u32 = 4;
pub const ACTION_MONITOR_FREEZE: u32 = 10;
pub const ACTION_MONITOR_UNFREEZE: u32 = 11;
pub const ACTION_MONITOR_CONFISCATE: u32 = 12;
pub const ACTION_MONITOR_FORCE_TRANSFER: u32 = 13;
pub const ACTION_MONITOR_FORCE_CLOSE: u32 = 14;
pub const ACTION_OFFCHAIN_SUBMIT_BATCH: u32 = 34;
pub const ACTION_OFFCHAIN_PROPOSE_FEE_RATE: u32 = 40;
pub const ACTION_OFFCHAIN_REGISTER_BANK: u32 = 50;
pub const ACTION_OFFCHAIN_UPDATE_BANK_ENDPOINT: u32 = 51;
pub const ACTION_OFFCHAIN_UNREGISTER_BANK: u32 = 52;
pub const ACTION_ENACT_LAW: u32 = 0;
pub const ACTION_AMEND_LAW: u32 = 1;
pub const ACTION_REPEAL_LAW: u32 = 2;
pub const ACTION_PLATFORM_PRICE: u32 = 5;
pub const ACTION_REGISTER_VOTING_IDENTITY: u32 = 0;
pub const ACTION_UPGRADE_CANDIDATE_IDENTITY: u32 = 1;
pub const ACTION_UPDATE_VOTING_IDENTITY: u32 = 2;
pub const ACTION_UPDATE_CANDIDATE_IDENTITY: u32 = 3;
pub const ACTION_REVOKE_IDENTITY: u32 = 4;
pub const ACTION_OCCUPY_CID: u32 = 6;
pub const ACTION_ADMIN_REBIND_CID_ACCOUNT_ID: u32 = 7;
pub const ACTION_REVOKE_CID: u32 = 8;
pub const ACTION_SET_ADDRESS_CATALOG: u32 = 0;
pub const ACTION_SET_ADDRESS_NAME: u32 = 1;
pub const ACTION_REMOVE_ADDRESS_NAME: u32 = 2;
pub const ACTION_SET_ADDRESS: u32 = 3;
pub const ACTION_REMOVE_ADDRESS: u32 = 4;
pub const ACTION_REGISTER_INSTITUTION: u32 = 0;

/// 不带机构主体的固定权限模板；创世写入时由 entity 补齐准确 CID 和岗位码。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedRolePermissionSpec {
    pub module_tag: &'static [u8],
    pub action_code: u32,
    pub operation: RolePermissionOperation,
}

fn push_permission(
    out: &mut Vec<FixedRolePermissionSpec>,
    module_tag: &'static [u8],
    action_code: u32,
    operation: RolePermissionOperation,
) {
    out.push(FixedRolePermissionSpec {
        module_tag,
        action_code,
        operation,
    });
}

fn push_both(out: &mut Vec<FixedRolePermissionSpec>, module_tag: &'static [u8], action_code: u32) {
    push_permission(
        out,
        module_tag,
        action_code,
        RolePermissionOperation::Propose,
    );
    push_permission(out, module_tag, action_code, RolePermissionOperation::Vote);
}

fn push_actions_both(
    out: &mut Vec<FixedRolePermissionSpec>,
    module_tag: &'static [u8],
    action_codes: &[u32],
) {
    for action_code in action_codes {
        push_both(out, module_tag, *action_code);
    }
}

/// 返回准确创世机构固定岗位的永久权限。
///
/// 返回空数组有两种合法含义：该岗位固定为空权限，或输入不是受保护创世固定岗位。
/// 调用方必须先用共享固定岗位规格确认岗位身份，不能把空数组当作一般岗位判断依据。
pub fn fixed_role_permission_specs(
    institution_code: InstitutionCode,
    cid_number: &[u8],
    role_code: &[u8],
) -> Vec<FixedRolePermissionSpec> {
    // 联邦安全局(FSC):唯一持有「联邦公民安全基金」的机构。给该局的法定代表人与
    // 局长两岗授「多签转账」Propose+Vote,使其经内部投票从该基金转出(选民=两岗、
    // 票据 2,严格过半阈值 2)。机构码严格限定为 FSC,不外溢到其它机构的同名岗位。
    if institution_code == FSC
        && (role_code == ROLE_CODE_LEGAL_REPRESENTATIVE || role_code == ROLE_CODE_DIRECTOR)
    {
        let mut out = Vec::new();
        push_both(&mut out, MODULE_MULTISIG, ACTION_MULTISIG_TRANSFER);
        return out;
    }

    // 法律行政签署和三人会签都属于原法律动作的 Vote 权限。LR 是所有机构
    // 唯一固定岗位；这里只给承担法定签署职责的机构码配置权限，不把权限扩大到
    // 其它机构的 LR，也不预建尚未依法组成的议员、参议员或委员岗位。
    if role_code == ROLE_CODE_LEGAL_REPRESENTATIVE
        && legislation_legal_representative_code(institution_code)
    {
        let mut out = Vec::new();
        for action_code in [ACTION_ENACT_LAW, ACTION_AMEND_LAW, ACTION_REPEAL_LAW] {
            push_permission(
                &mut out,
                MODULE_LEGISLATION_YUAN,
                action_code,
                RolePermissionOperation::Vote,
            );
        }
        return out;
    }

    let is_public_fixed = fixed_institution_by_identity(institution_code, cid_number).is_some();
    let is_technology = is_citizenchain_foundation_identity(institution_code, cid_number);
    if !is_public_fixed && !is_technology {
        return Vec::new();
    }

    // 普通受保护公权机构的 LR 永久为空权限；基金会 LR 是准确 CID 的独立规格。
    if role_code == ROLE_CODE_LEGAL_REPRESENTATIVE && !is_technology {
        return Vec::new();
    }

    let mut out = Vec::new();
    match institution_code {
        NRC if role_code == ROLE_CODE_COMMITTEE_MEMBER => {
            push_both(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            push_both(&mut out, MODULE_RUNTIME_UPGRADE, ACTION_RUNTIME_UPGRADE);
            push_both(
                &mut out,
                MODULE_RESOLUTION_ISSUANCE,
                ACTION_RESOLUTION_ISSUANCE,
            );
            push_both(
                &mut out,
                MODULE_RESOLUTION_DESTROY,
                ACTION_RESOLUTION_DESTROY,
            );
            push_both(
                &mut out,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY,
            );
            push_permission(
                &mut out,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_ROTATION,
                RolePermissionOperation::Propose,
            );
            push_actions_both(
                &mut out,
                MODULE_MULTISIG,
                &[
                    ACTION_MULTISIG_TRANSFER,
                    ACTION_SAFETY_FUND_TRANSFER,
                    ACTION_FEE_SWEEP_TO_MAIN,
                ],
            );
            push_actions_both(
                &mut out,
                MODULE_ONCHAIN_ISSUANCE,
                &[
                    ACTION_MONITOR_FREEZE,
                    ACTION_MONITOR_UNFREEZE,
                    ACTION_MONITOR_CONFISCATE,
                    ACTION_MONITOR_FORCE_TRANSFER,
                    ACTION_MONITOR_FORCE_CLOSE,
                ],
            );
        }
        PRC if role_code == ROLE_CODE_COMMITTEE_MEMBER => {
            push_both(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            // 协议升级与决议发行相同，由 NRC + 43 个 PRC 委员岗位共同拥有。
            push_both(&mut out, MODULE_RUNTIME_UPGRADE, ACTION_RUNTIME_UPGRADE);
            push_both(
                &mut out,
                MODULE_RESOLUTION_ISSUANCE,
                ACTION_RESOLUTION_ISSUANCE,
            );
            push_both(
                &mut out,
                MODULE_RESOLUTION_DESTROY,
                ACTION_RESOLUTION_DESTROY,
            );
            push_both(
                &mut out,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY,
            );
            push_permission(
                &mut out,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_ROTATION,
                RolePermissionOperation::Propose,
            );
            push_both(&mut out, MODULE_MULTISIG, ACTION_MULTISIG_TRANSFER);
        }
        PRB if role_code == ROLE_CODE_DIRECTOR => {
            push_both(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            // 协议升级与决议发行使用同一联合投票主体：PRB 董事仅参与投票，不能发起。
            push_permission(
                &mut out,
                MODULE_RUNTIME_UPGRADE,
                ACTION_RUNTIME_UPGRADE,
                RolePermissionOperation::Vote,
            );
            push_permission(
                &mut out,
                MODULE_RESOLUTION_ISSUANCE,
                ACTION_RESOLUTION_ISSUANCE,
                RolePermissionOperation::Vote,
            );
            push_both(
                &mut out,
                MODULE_RESOLUTION_DESTROY,
                ACTION_RESOLUTION_DESTROY,
            );
            push_actions_both(
                &mut out,
                MODULE_MULTISIG,
                &[ACTION_MULTISIG_TRANSFER, ACTION_FEE_SWEEP_TO_MAIN],
            );
        }
        NJD if role_code == ROLE_CODE_CHIEF_JUSTICE => {
            push_both(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
        }
        NJD if role_code == ROLE_CODE_DEPUTY_CHIEF_JUSTICE || role_code == ROLE_CODE_JUSTICE => {
            push_permission(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
                RolePermissionOperation::Vote,
            );
        }
        NJD if role_code == ROLE_CODE_CONSTITUTION_GUARD => {
            push_permission(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
                RolePermissionOperation::Vote,
            );
            push_permission(
                &mut out,
                MODULE_LEGISLATION_YUAN,
                ACTION_AMEND_LAW,
                RolePermissionOperation::Vote,
            );
        }
        FRG if fixed_role_seats_by_identity(institution_code, cid_number, role_code).is_some() => {
            push_both(
                &mut out,
                MODULE_PUBLIC_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            push_both(
                &mut out,
                MODULE_INSTITUTION_REGISTRATION,
                ACTION_REGISTER_INSTITUTION,
            );
            push_actions_both(
                &mut out,
                MODULE_CITIZEN_IDENTITY,
                &[
                    ACTION_REGISTER_VOTING_IDENTITY,
                    ACTION_UPGRADE_CANDIDATE_IDENTITY,
                    ACTION_UPDATE_VOTING_IDENTITY,
                    ACTION_UPDATE_CANDIDATE_IDENTITY,
                    ACTION_REVOKE_IDENTITY,
                    ACTION_OCCUPY_CID,
                    ACTION_ADMIN_REBIND_CID_ACCOUNT_ID,
                    ACTION_REVOKE_CID,
                ],
            );
            push_actions_both(
                &mut out,
                MODULE_ADDRESS_REGISTRY,
                &[
                    ACTION_SET_ADDRESS_CATALOG,
                    ACTION_SET_ADDRESS_NAME,
                    ACTION_REMOVE_ADDRESS_NAME,
                    ACTION_SET_ADDRESS,
                    ACTION_REMOVE_ADDRESS,
                ],
            );
        }
        _ if is_technology && role_code == ROLE_CODE_LEGAL_REPRESENTATIVE => {
            push_both(
                &mut out,
                MODULE_PRIVATE_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            push_permission(
                &mut out,
                MODULE_SQUARE_SUBSCRIPTION,
                ACTION_PLATFORM_PRICE,
                RolePermissionOperation::Vote,
            );
        }
        _ if is_technology && role_code == ROLE_CODE_GENESIS_PRODUCT_MANAGER => {
            push_both(
                &mut out,
                MODULE_PRIVATE_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            push_both(&mut out, MODULE_SQUARE_SUBSCRIPTION, ACTION_PLATFORM_PRICE);
        }
        _ if is_technology && role_code == ROLE_CODE_GENESIS_PROGRAMMER => {
            push_both(
                &mut out,
                MODULE_PRIVATE_MANAGE,
                ACTION_INSTITUTION_GOVERNANCE,
            );
            push_permission(
                &mut out,
                MODULE_SQUARE_SUBSCRIPTION,
                ACTION_PLATFORM_PRICE,
                RolePermissionOperation::Vote,
            );
        }
        _ => {}
    }
    out
}

/// 需要承担法律行政签署或国家/省级三人会签的机构码。
fn legislation_legal_representative_code(institution_code: InstitutionCode) -> bool {
    matches!(
        institution_code,
        code if code == *b"PRS\0"
            || code == *b"NLG\0"
            || code == *b"NRP\0"
            || code == *b"NSN\0"
            || code == *b"NED\0"
            || code == *b"PGV\0"
            || code == *b"PLG\0"
            || code == *b"PRP\0"
            || code == *b"PSN\0"
            || code == *b"CGOV"
    )
}

/// 立法机构 CID 顶层能力白名单。
///
/// 岗位码仍由各机构依法创建并写入权限；这里只回答该机构类型能否拥有权限，不能
/// 代替 `InstitutionRolePermissions`，也不能让普通 admins 自动取得任何能力。
pub fn legislation_institution_capability_allows(
    institution_code: InstitutionCode,
    module_tag: &[u8],
    action_code: u32,
    operation: RolePermissionOperation,
) -> bool {
    if module_tag != MODULE_LEGISLATION_YUAN
        || !matches!(
            action_code,
            ACTION_ENACT_LAW | ACTION_AMEND_LAW | ACTION_REPEAL_LAW
        )
    {
        return false;
    }
    match operation {
        RolePermissionOperation::Propose => matches!(
            institution_code,
            code if code == *b"NRP\0"
                || code == *b"NED\0"
                || code == *b"PRP\0"
                || code == *b"CSLF"
                || code == *b"CEDU"
                || code == *b"CLEG"
        ),
        RolePermissionOperation::Vote => {
            legislation_legal_representative_code(institution_code)
                || matches!(
                    institution_code,
                    code if code == *b"CLEG"
                        || (code == NJD && action_code == ACTION_AMEND_LAW)
                )
        }
    }
}

/// 固定创世机构 CID 顶层能力白名单。
///
/// 固定岗位权限必须是本白名单的子集。动态岗位后续需要新增业务时，应先由对应业务步骤
/// 明确扩展 CID 顶层能力，再创建新岗位；不得借此函数给固定岗位追加权限。
pub fn fixed_institution_capability_allows(
    institution_code: InstitutionCode,
    cid_number: &[u8],
    module_tag: &[u8],
    action_code: u32,
    operation: RolePermissionOperation,
) -> bool {
    let role_codes: Vec<Vec<u8>> = match institution_code {
        NRC | PRC => vec![ROLE_CODE_COMMITTEE_MEMBER.to_vec()],
        PRB => vec![ROLE_CODE_DIRECTOR.to_vec()],
        NJD => vec![
            ROLE_CODE_CONSTITUTION_GUARD.to_vec(),
            ROLE_CODE_CHIEF_JUSTICE.to_vec(),
            ROLE_CODE_DEPUTY_CHIEF_JUSTICE.to_vec(),
            ROLE_CODE_JUSTICE.to_vec(),
        ],
        FRG => PROVINCE_CODE_INFOS
            .iter()
            .map(|province| province_commissioner_role_code(province.province_code))
            .collect(),
        _ if legislation_legal_representative_code(institution_code) => {
            vec![ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec()]
        }
        _ if is_citizenchain_foundation_identity(institution_code, cid_number) => vec![
            ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec(),
            ROLE_CODE_GENESIS_PRODUCT_MANAGER.to_vec(),
            ROLE_CODE_GENESIS_PROGRAMMER.to_vec(),
        ],
        // 联邦安全局:LR + 局长两岗持有多签转账能力,用于经内部投票从联邦公民安全基金转出。
        FSC => vec![
            ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec(),
            ROLE_CODE_DIRECTOR.to_vec(),
        ],
        _ => Vec::new(),
    };
    role_codes.into_iter().any(|role_code| {
        fixed_role_permission_specs(institution_code, cid_number, &role_code)
            .into_iter()
            .any(|permission| {
                permission.module_tag == module_tag
                    && permission.action_code == action_code
                    && permission.operation == operation
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitives::cid::china::{china_cb::CHINA_CB, china_ch::CHINA_CH, china_sf::CHINA_SF};

    fn has(
        permissions: &[FixedRolePermissionSpec],
        module_tag: &[u8],
        action_code: u32,
        operation: RolePermissionOperation,
    ) -> bool {
        permissions.iter().any(|permission| {
            permission.module_tag == module_tag
                && permission.action_code == action_code
                && permission.operation == operation
        })
    }

    #[test]
    fn runtime_upgrade_is_shared_by_nrc_and_prc_committees() {
        for entry in CHINA_CB {
            let permissions = fixed_role_permission_specs(
                primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                    .expect("CHINA_CB CID encodes institution code"),
                entry.cid_number.as_bytes(),
                ROLE_CODE_COMMITTEE_MEMBER,
            );
            assert!(has(
                &permissions,
                MODULE_RUNTIME_UPGRADE,
                ACTION_RUNTIME_UPGRADE,
                RolePermissionOperation::Propose,
            ));
            assert!(has(
                &permissions,
                MODULE_RUNTIME_UPGRADE,
                ACTION_RUNTIME_UPGRADE,
                RolePermissionOperation::Vote,
            ));
        }
    }

    #[test]
    fn grandpa_rotation_is_direct_but_emergency_recovery_is_internal_vote() {
        for entry in CHINA_CB {
            let permissions = fixed_role_permission_specs(
                primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                    .expect("CHINA_CB CID encodes institution code"),
                entry.cid_number.as_bytes(),
                ROLE_CODE_COMMITTEE_MEMBER,
            );
            assert!(has(
                &permissions,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY,
                RolePermissionOperation::Propose,
            ));
            assert!(has(
                &permissions,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY,
                RolePermissionOperation::Vote,
            ));
            assert!(has(
                &permissions,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_ROTATION,
                RolePermissionOperation::Propose,
            ));
            assert!(!has(
                &permissions,
                MODULE_GRANDPA_KEY_CHANGE,
                ACTION_GRANDPA_KEY_ROTATION,
                RolePermissionOperation::Vote,
            ));
        }
    }

    #[test]
    fn fsc_lr_and_director_get_multisig_transfer_others_do_not() {
        use primitives::cid::china::china_zf::CHINA_ZF;
        use primitives::cid::code::institution_code_from_cid_number;

        let fsc_cid = CHINA_ZF
            .iter()
            .find(|n| institution_code_from_cid_number(n.cid_number) == Some(FSC))
            .expect("CHINA_ZF must include FSC")
            .cid_number;

        // FSC 的法定代表人与局长两岗都拿到多签转账 Propose+Vote。
        for role_code in [ROLE_CODE_LEGAL_REPRESENTATIVE, ROLE_CODE_DIRECTOR] {
            let perms = fixed_role_permission_specs(FSC, fsc_cid.as_bytes(), role_code);
            assert!(has(
                &perms,
                MODULE_MULTISIG,
                ACTION_MULTISIG_TRANSFER,
                RolePermissionOperation::Propose,
            ));
            assert!(has(
                &perms,
                MODULE_MULTISIG,
                ACTION_MULTISIG_TRANSFER,
                RolePermissionOperation::Vote,
            ));
        }

        // 不外溢:其它机构(国家储委会)的法定代表人不得拿到多签转账权限。
        let nrc = &CHINA_CB[0];
        let nrc_lr = fixed_role_permission_specs(
            institution_code_from_cid_number(nrc.cid_number).expect("NRC code"),
            nrc.cid_number.as_bytes(),
            ROLE_CODE_LEGAL_REPRESENTATIVE,
        );
        assert!(!has(
            &nrc_lr,
            MODULE_MULTISIG,
            ACTION_MULTISIG_TRANSFER,
            RolePermissionOperation::Propose,
        ));
        assert!(!has(
            &nrc_lr,
            MODULE_MULTISIG,
            ACTION_MULTISIG_TRANSFER,
            RolePermissionOperation::Vote,
        ));
    }

    #[test]
    fn protocol_upgrade_and_resolution_issuance_give_prb_vote_only() {
        let entry = &CHINA_CH[0];
        let permissions =
            fixed_role_permission_specs(PRB, entry.cid_number.as_bytes(), ROLE_CODE_DIRECTOR);
        for (module_tag, action_code) in [
            (MODULE_RUNTIME_UPGRADE, ACTION_RUNTIME_UPGRADE),
            (MODULE_RESOLUTION_ISSUANCE, ACTION_RESOLUTION_ISSUANCE),
        ] {
            assert!(has(
                &permissions,
                module_tag,
                action_code,
                RolePermissionOperation::Vote,
            ));
            assert!(!has(
                &permissions,
                module_tag,
                action_code,
                RolePermissionOperation::Propose,
            ));
        }
    }

    #[test]
    fn judicial_and_technology_roles_are_least_privilege() {
        let njd = &CHINA_SF[0];
        let chief =
            fixed_role_permission_specs(NJD, njd.cid_number.as_bytes(), ROLE_CODE_CHIEF_JUSTICE);
        let justice =
            fixed_role_permission_specs(NJD, njd.cid_number.as_bytes(), ROLE_CODE_JUSTICE);
        assert!(has(
            &chief,
            MODULE_PUBLIC_MANAGE,
            ACTION_INSTITUTION_GOVERNANCE,
            RolePermissionOperation::Propose,
        ));
        assert!(!has(
            &justice,
            MODULE_PUBLIC_MANAGE,
            ACTION_INSTITUTION_GOVERNANCE,
            RolePermissionOperation::Propose,
        ));

        let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
        let product = fixed_role_permission_specs(
            *b"SFGY",
            foundation.cid_number.as_bytes(),
            ROLE_CODE_GENESIS_PRODUCT_MANAGER,
        );
        let programmer = fixed_role_permission_specs(
            *b"SFGY",
            foundation.cid_number.as_bytes(),
            ROLE_CODE_GENESIS_PROGRAMMER,
        );
        assert!(has(
            &product,
            MODULE_SQUARE_SUBSCRIPTION,
            ACTION_PLATFORM_PRICE,
            RolePermissionOperation::Propose,
        ));
        assert!(!has(
            &programmer,
            MODULE_SQUARE_SUBSCRIPTION,
            ACTION_PLATFORM_PRICE,
            RolePermissionOperation::Propose,
        ));
    }

    #[test]
    fn legislation_lr_and_institution_capabilities_are_separate() {
        let lr = fixed_role_permission_specs(
            *b"NRP\0",
            b"LN001-NRP0G-000000001-2026",
            ROLE_CODE_LEGAL_REPRESENTATIVE,
        );
        for action_code in [ACTION_ENACT_LAW, ACTION_AMEND_LAW, ACTION_REPEAL_LAW] {
            assert!(has(
                &lr,
                MODULE_LEGISLATION_YUAN,
                action_code,
                RolePermissionOperation::Vote,
            ));
            assert!(!has(
                &lr,
                MODULE_LEGISLATION_YUAN,
                action_code,
                RolePermissionOperation::Propose,
            ));
            assert!(legislation_institution_capability_allows(
                *b"CLEG",
                MODULE_LEGISLATION_YUAN,
                action_code,
                RolePermissionOperation::Propose,
            ));
        }

        assert!(!legislation_institution_capability_allows(
            *b"PRS\0",
            MODULE_LEGISLATION_YUAN,
            ACTION_ENACT_LAW,
            RolePermissionOperation::Propose,
        ));
        assert!(legislation_institution_capability_allows(
            *b"PRS\0",
            MODULE_LEGISLATION_YUAN,
            ACTION_ENACT_LAW,
            RolePermissionOperation::Vote,
        ));
        assert!(legislation_institution_capability_allows(
            NJD,
            MODULE_LEGISLATION_YUAN,
            ACTION_AMEND_LAW,
            RolePermissionOperation::Vote,
        ));
        assert!(!legislation_institution_capability_allows(
            NJD,
            MODULE_LEGISLATION_YUAN,
            ACTION_ENACT_LAW,
            RolePermissionOperation::Vote,
        ));
    }
}
