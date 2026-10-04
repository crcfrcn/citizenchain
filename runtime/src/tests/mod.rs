#![cfg(test)]
// runtime 测试夹具异常必须立即中止，断言式解包仅限测试模块。
#![allow(clippy::expect_used, clippy::unwrap_used)]

extern crate alloc;
use admin_primitives::InstitutionAdminQuery;
use alloc::vec::Vec;

use super::*;
use crate::configs::is_nrc_admin;
use crate::configs::*;
use crate::ResolutionDestroy;
use entity_primitives::InstitutionRoleAuthorizationQuery;
use frame_support::traits::{Contains, Currency, EnsureOrigin, FindAuthor};
use frame_support::{assert_noop, assert_ok};
use primitives::cid::china::china_cb::CHINA_CB;
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
use primitives::multisig::ReservedAccountGuard;
use sp_core::{sr25519, Pair};
use sp_runtime::{traits::Hash as HashT, traits::IdentifyAccount, BuildStorage, MultiSigner};
use votingengine::{CitizenIdentityReader, InternalAdminProvider, JointVoteResultCallback};

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = crate::RuntimeGenesisConfig::default()
        .build_storage()
        .expect("runtime test storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        // 链上时间 = 2026-07-02 00:00 UTC,使公民身份夹具护照(20260630-20360630)
        // 落在投票资格的护照有效期窗口内。
        pallet_timestamp::Pallet::<crate::Runtime>::set_timestamp(1_782_950_400_000);
        citizen_identity::PopulationReadyDate::<crate::Runtime>::put(20260702);
    });
    ext
}

fn test_area_code(bytes: &[u8]) -> citizen_identity::AreaCodeBound {
    bytes.to_vec().try_into().expect("area code fits")
}

fn test_cid_number(bytes: &[u8]) -> citizen_identity::CidNumberBound {
    bytes.to_vec().try_into().expect("cid number fits")
}

/// 按 tag 生成真实规则 CID 号(格式/校验和/机构码全合规)。
fn real_cid_number(tag: &str, institution: &str, p1: &str) -> Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            p1,
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution,
        },
    )
    .expect("cid should generate")
    .into_bytes()
}

fn build_voting_identity_payload(
    account_id: AccountId,
    cid_number: &[u8],
    province_code: &[u8],
    city_code: &[u8],
    town_code: &[u8],
) -> citizen_identity::VotingIdentityPayload<AccountId> {
    citizen_identity::VotingIdentityPayload {
        cid_number: test_cid_number(cid_number),
        account_id,
        passport_valid_from: 20260630,
        passport_valid_until: 20360630,
        citizen_status: citizen_identity::CitizenStatus::Normal,
        residence_province_code: test_area_code(province_code),
        residence_city_code: test_area_code(city_code),
        residence_town_code: test_area_code(town_code),
    }
}

/// 按链端授权结构构造身份写入授权并签名。
///
/// 返回 `(身份版本, 授权有效期, 签名)`，三者必须与 extrinsic 入参完全一致；
/// 版本读链上当前值、有效期按链上时间推算，与生产端取值方式相同。
fn sign_citizen_identity_authorization<P: codec::Encode + Clone>(
    signer_pair: &sr25519::Pair,
    cid_number: &citizen_identity::CidNumberBound,
    payload: &P,
) -> (u64, u64, citizen_identity::pallet::SignatureOf<Runtime>) {
    use frame_support::traits::UnixTime;

    let expected_identity_version =
        citizen_identity::VotingEligibilityVersionCount::<Runtime>::get(cid_number);
    let expires_at = <Timestamp as UnixTime>::now().as_secs().saturating_add(300);
    let authorization = citizen_identity::CitizenIdentityAuthorization {
        genesis_hash: frame_system::Pallet::<Runtime>::block_hash(0u32),
        payload: payload.clone(),
        expected_identity_version,
        expires_at,
    };
    let signature = sign_citizen_identity_payload(signer_pair, &authorization);
    (expected_identity_version, expires_at, signature)
}

fn sign_citizen_identity_payload(
    signer_pair: &sr25519::Pair,
    payload: &impl codec::Encode,
) -> citizen_identity::pallet::SignatureOf<Runtime> {
    let msg = primitives::sign::signing_message(
        primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
        &payload.encode(),
    );
    signer_pair
        .sign(&msg)
        .0
        .to_vec()
        .try_into()
        .expect("citizen identity signature fits")
}

fn sign_citizen_identity_message(
    signer_pair: &sr25519::Pair,
    op_tag: u8,
    payload: &[u8],
) -> citizen_identity::pallet::SignatureOf<Runtime> {
    signer_pair
        .sign(&primitives::sign::signing_message(op_tag, payload))
        .0
        .to_vec()
        .try_into()
        .expect("citizen identity signature fits")
}

/// 注册局首次占号时，新账户签署绑定创世哈希、固定 revision=0 和过期时间的唯一载荷。
fn sign_cid_occupy(
    signer_pair: &sr25519::Pair,
    cid_number: &citizen_identity::CidNumberBound,
    account_id: &AccountId,
    expires_at: u64,
) -> citizen_identity::pallet::SignatureOf<Runtime> {
    let payload = codec::Encode::encode(&citizen_identity::CidOccupyAuthorization {
        genesis_hash: System::block_hash(0),
        cid_number: cid_number.clone(),
        account_id: account_id.clone(),
        expected_binding_revision: 0,
        expires_at,
    });
    let msg = primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_OCCUPY, &payload);
    signer_pair
        .sign(&msg)
        .0
        .to_vec()
        .try_into()
        .expect("cid occupy signature fits")
}

/// 注册局换绑时，新账户签署完整绑定快照；当前账户不参与本入口签名。
fn sign_cid_admin_rebind(
    signer_pair: &sr25519::Pair,
    cid_number: &citizen_identity::CidNumberBound,
    current_account_id: &AccountId,
    new_account_id: &AccountId,
    expected_binding_revision: u64,
    expires_at: u64,
) -> citizen_identity::pallet::SignatureOf<Runtime> {
    let payload = codec::Encode::encode(&citizen_identity::CidRebindAuthorization {
        genesis_hash: System::block_hash(0),
        cid_number: cid_number.clone(),
        current_account_id: current_account_id.clone(),
        new_account_id: new_account_id.clone(),
        expected_binding_revision,
        expires_at,
    });
    sign_citizen_identity_message(
        signer_pair,
        primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
        &payload,
    )
}

/// runtime 集成夹具按链上 Timestamp.Now 签发 300 秒首次绑定授权。
fn cid_authorization_expires_at() -> u64 {
    Timestamp::get().saturating_div(1_000).saturating_add(300)
}

fn setup_frg_citizen_identity_admin(
    province_code: &[u8],
) -> (
    sr25519::Pair,
    AccountId,
    public_manage::pallet::CidNumberOf<Runtime>,
    public_manage::RoleCodeOf,
) {
    let registrar_pair =
        sr25519::Pair::from_string("//frg-citizen-identity-admin", None).expect("registrar pair");
    let registrar = AccountId::new(registrar_pair.public().0);
    let federal_registry = primitives::governance_skeleton::federal_registry_institution();
    let main_account = AccountId::new(federal_registry.main_account);
    // 注册局省级授权由 entity 的“省专员岗位 + 有效任职”表达，不再建立虚拟管理员组。
    let cid_number: public_manage::pallet::CidNumberOf<Runtime> = federal_registry
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("FRG CID fits");
    let account_name: public_manage::pallet::AccountNameOf<Runtime> =
        primitives::account_derive::RESERVED_NAME_MAIN
            .to_vec()
            .try_into()
            .expect("account name fits");
    public_admins::pallet::AdminAccounts::<Runtime>::insert(
        cid_number.clone(),
        admin_primitives::InstitutionAdmins {
            institution_code: admin_primitives::FRG,
            admins: vec![admin_primitives::Admin {
                account_id: registrar.clone(),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            }]
            .try_into()
            .expect("single registrar admin fits"),
        },
    );
    public_manage::AccountRegisteredCid::<Runtime>::insert(
        main_account.clone(),
        entity_primitives::RegisteredInstitution {
            cid_number: cid_number.clone(),
            account_name: account_name.clone(),
        },
    );
    public_manage::InstitutionAccounts::<Runtime>::insert(
        &cid_number,
        account_name,
        entity_primitives::InstitutionAccountInfo {
            account_id: main_account,
            initial_balance: 0,
            created_at: 0,
        },
    );
    public_manage::Institutions::<Runtime>::insert(
        &cid_number,
        entity_primitives::InstitutionInfo {
            cid_full_name: "联邦注册局测试机构"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("full name fits"),
            cid_short_name: "注册局测试"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("short name fits"),
            town_code: Default::default(),
            legal_representative: None,
            institution_code: admin_primitives::FRG,
            created_at: 0,
        },
    );
    let province_code: primitives::cid::code::ProvinceCode =
        province_code.try_into().expect("province code fits");
    let role_code: public_manage::RoleCodeOf =
        primitives::governance_skeleton::province_commissioner_role_code(province_code)
            .try_into()
            .expect("role code fits");
    public_manage::InstitutionRoles::<Runtime>::insert(
        &cid_number,
        &role_code,
        entity_primitives::InstitutionRole {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
            role_name: "测试省专员"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("role name fits"),
            term_required: true,
            role_status: entity_primitives::InstitutionRoleStatus::Active,
        },
    );
    let assignments: public_manage::institution::role::RoleAssignmentsOf<Runtime> =
        vec![entity_primitives::InstitutionAdminAssignment {
            cid_number: cid_number.clone(),
            account_id: registrar.clone(),
            role_code: role_code.clone(),
            // 测试链时间已设置为 2026-07-02；固定任职从第 1 天起永久有效。
            term_start: 1,
            term_end: u32::MAX,
            assignment_source: entity_primitives::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
        }]
        .try_into()
        .expect("assignment fits");
    public_manage::InstitutionRoleAssignments::<Runtime>::insert(
        cid_number.clone(),
        role_code.clone(),
        assignments,
    );
    let permission_specs = entity_primitives::fixed_role_permission_specs(
        admin_primitives::FRG,
        cid_number.as_slice(),
        role_code.as_slice(),
    );
    let permissions: public_manage::RolePermissionsOf<Runtime> = permission_specs
        .into_iter()
        .map(|spec| entity_primitives::RoleBusinessPermission {
            role_subject: entity_primitives::RoleSubject {
                cid_number: cid_number.clone(),
                role_code: role_code.clone(),
            },
            business_action_id: entity_primitives::BusinessActionId {
                module_tag: spec
                    .module_tag
                    .to_vec()
                    .try_into()
                    .expect("module tag fits"),
                action_code: spec.action_code,
            },
            operation: spec.operation,
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("fixed permissions fit");
    public_manage::InstitutionRolePermissions::<Runtime>::insert(
        cid_number.clone(),
        role_code.clone(),
        permissions,
    );
    assert!(
        RuntimeInstitutionAdminQuery::is_institution_admin(
            admin_primitives::FRG,
            cid_number.as_slice(),
            &registrar,
        ),
        "FRG 测试管理员必须存在于目标机构 admins"
    );
    assert!(
        <public_manage::Pallet<Runtime> as entity_primitives::InstitutionRoleQuery<AccountId>>::is_active_assignment(
            cid_number.as_slice(),
            &registrar,
            role_code.as_slice(),
        ),
        "FRG 省专员测试任职必须在测试链日期有效"
    );
    let role_subject = entity_primitives::RoleSubject {
        cid_number: cid_number.to_vec(),
        role_code: role_code.to_vec(),
    };
    let business_action = entity_primitives::BusinessActionId {
        module_tag: entity_primitives::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
        action_code: entity_primitives::business_action::ACTION_OCCUPY_CID,
    };
    let stored_permissions =
        public_manage::InstitutionRolePermissions::<Runtime>::get(&cid_number, &role_code);
    assert!(
        stored_permissions.iter().any(|permission| {
            permission.role_subject.cid_number.as_slice() == cid_number.as_slice()
                && permission.role_subject.role_code.as_slice() == role_code.as_slice()
                && permission.business_action_id.module_tag.as_slice()
                    == entity_primitives::business_action::MODULE_CITIZEN_IDENTITY
                && permission.business_action_id.action_code
                    == entity_primitives::business_action::ACTION_OCCUPY_CID
                && permission.operation == entity_primitives::RolePermissionOperation::Propose
        }),
        "FRG 省专员测试权限存储必须包含公民 CID 占号提案权限"
    );
    assert!(
        <RuntimeInstitutionCapabilityPolicy as entity_primitives::InstitutionCapabilityPolicy>::allows(
            cid_number.as_slice(),
            &business_action,
            entity_primitives::RolePermissionOperation::Propose,
        ),
        "FRG 必须拥有公民 CID 占号顶层能力"
    );
    assert!(
        <public_manage::Pallet<Runtime> as entity_primitives::InstitutionRoleAuthorizationQuery<
            AccountId,
        >>::role_has_permission(
            &role_subject,
            &business_action,
            entity_primitives::RolePermissionOperation::Propose,
        ),
        "FRG 省专员岗位必须登记公民 CID 占号权限"
    );
    assert!(
        RuntimeInstitutionRoleAuthorization::is_authorized(
            &registrar,
            &role_subject,
            &business_action,
            entity_primitives::RolePermissionOperation::Propose,
        ),
        "FRG 省专员测试主体必须拥有公民 CID 占号权限"
    );
    (registrar_pair, registrar, cid_number, role_code)
}

fn stake_account() -> AccountId {
    AccountId::new(primitives::cid::china::china_ch::CHINA_CH[0].stake_account)
}

fn reserved_main_account() -> AccountId {
    AccountId::new(primitives::cid::china::china_cb::CHINA_CB[1].main_account)
}

fn reserved_fee_account() -> AccountId {
    AccountId::new(primitives::cid::china::china_ch::CHINA_CH[0].fee_account)
}

fn ordinary_account() -> AccountId {
    AccountId::new([99u8; 32])
}

/// L2 清算账户 fixture 地址(资金白名单 / 防占号用例)。
fn clearing_account() -> AccountId {
    AccountId::new([0xC1u8; 32])
}

/// 在 `private_manage` 反向索引把 `clearing_account()` 登记为某 SFGF 的「清算账户」。
/// 读写 storage,必须在 `execute_with` 内调用。
fn register_clearing_account() {
    let cid_number: private_manage::CidNumberOf<Runtime> = b"GD001-SCB05-000000002-2026"
        .to_vec()
        .try_into()
        .expect("测试 CID 长度合法");
    let account_name: private_manage::AccountNameOf<Runtime> = "清算账户"
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("清算账户名长度合法");
    private_manage::AccountRegisteredCid::<Runtime>::insert(
        clearing_account(),
        entity_primitives::RegisteredInstitution {
            cid_number,
            account_name,
        },
    );
}

/// 四个公民身份签名域在普通 runtime 与 `runtime-benchmarks` 下必须保持同一验证结果。
/// 非空且长度正确的伪造签名不能因为 feature 改变而通过。
#[test]
fn citizen_identity_signature_verifiers_reject_nonempty_forgery_in_every_feature() {
    use citizen_identity::CitizenIdentityAuthority as _;

    new_test_ext().execute_with(|| {
        let signer_pair = sr25519::Pair::from_seed(&[0x31; 32]);
        let account_id = AccountId::new(signer_pair.public().0);
        let other_account_id = AccountId::new(sr25519::Pair::from_seed(&[0x32; 32]).public().0);
        let payload = b"citizen-identity-verifier-regression";
        let forged_signature: citizen_identity::pallet::SignatureOf<Runtime> =
            [0xA5u8; 64].to_vec().try_into().expect("signature fits");
        let short_signature: citizen_identity::pallet::SignatureOf<Runtime> =
            [0xA5u8; 63].to_vec().try_into().expect("signature fits");

        let citizen_signature = sign_citizen_identity_message(
            &signer_pair,
            primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
            payload,
        );
        let rebind_signature = sign_citizen_identity_message(
            &signer_pair,
            primitives::sign::OP_SIGN_CID_REBIND,
            payload,
        );
        let occupy_signature = sign_citizen_identity_message(
            &signer_pair,
            primitives::sign::OP_SIGN_CID_OCCUPY,
            payload,
        );
        let admin_rebind_signature = sign_citizen_identity_message(
            &signer_pair,
            primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
            payload,
        );

        assert!(RuntimeCitizenIdentityAuthority::verify_citizen_signature(
            &account_id,
            payload,
            &citizen_signature,
        ));
        assert!(RuntimeCitizenIdentityAuthority::verify_rebind_signature(
            &account_id,
            payload,
            &rebind_signature,
        ));
        assert!(RuntimeCitizenIdentityAuthority::verify_occupy_signature(
            &account_id,
            payload,
            &occupy_signature,
        ));
        assert!(
            RuntimeCitizenIdentityAuthority::verify_admin_rebind_signature(
                &account_id,
                payload,
                &admin_rebind_signature,
            )
        );

        assert!(!RuntimeCitizenIdentityAuthority::verify_citizen_signature(
            &account_id,
            payload,
            &forged_signature,
        ));
        assert!(!RuntimeCitizenIdentityAuthority::verify_rebind_signature(
            &account_id,
            payload,
            &forged_signature,
        ));
        assert!(!RuntimeCitizenIdentityAuthority::verify_occupy_signature(
            &account_id,
            payload,
            &forged_signature,
        ));
        assert!(
            !RuntimeCitizenIdentityAuthority::verify_admin_rebind_signature(
                &account_id,
                payload,
                &forged_signature,
            )
        );

        // 相同密钥、相同载荷也不能跨操作码复用；账户或长度错误同样拒绝。
        assert!(!RuntimeCitizenIdentityAuthority::verify_rebind_signature(
            &account_id,
            payload,
            &citizen_signature,
        ));
        assert!(!RuntimeCitizenIdentityAuthority::verify_citizen_signature(
            &other_account_id,
            payload,
            &citizen_signature,
        ));
        assert!(!RuntimeCitizenIdentityAuthority::verify_citizen_signature(
            &account_id,
            payload,
            &short_signature,
        ));
    });
}

mod cases;
