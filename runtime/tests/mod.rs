//! 装配主Runtime测试外部执行环境及真实CID辅助数据。

#![cfg(test)]
// runtime 测试夹具异常必须立即中止，断言式解包仅限测试模块。
#![allow(clippy::expect_used, clippy::unwrap_used)]

extern crate alloc;
use alloc::vec::Vec;

use super::*;
use crate::configs::is_nrc_member;
use crate::configs::*;
use crate::Destroy;
use engingcore::{CitizenIdentityReader, JointVoteResultCallback};
use entitybase::InstitutionRoleAuthorizationQuery;
use frame_support::traits::{Contains, Currency, FindAuthor};
use frame_support::{assert_noop, assert_ok};
use primitives::cid::china::china_cb::CHINA_CB;
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
use primitives::multisig::ReservedAccountGuard;
use sp_core::{sr25519, Pair};
#[cfg(not(feature = "runtime-benchmarks"))]
use sp_runtime::traits::Hash as HashT;
use sp_runtime::{traits::IdentifyAccount, BuildStorage, MultiSigner};

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
        identity::PopulationReadyDate::<crate::Runtime>::put(20260702);
    });
    ext
}

fn test_area_code(bytes: &[u8]) -> identity::AreaCodeBound {
    bytes.to_vec().try_into().expect("area code fits")
}

fn test_cid_number(bytes: &[u8]) -> identity::CidNumberBound {
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
            year: 2026,
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
) -> identity::VotingIdentityPayload<AccountId> {
    identity::VotingIdentityPayload {
        cid_number: test_cid_number(cid_number),
        account_id,
        passport_valid_from: 20260630,
        passport_valid_until: 20360630,
        citizen_status: identity::CitizenStatus::Normal,
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
    cid_number: &identity::CidNumberBound,
    payload: &P,
) -> (u64, u64, identity::pallet::SignatureOf<Runtime>) {
    use frame_support::traits::UnixTime;

    let expected_identity_version =
        identity::VotingEligibilityVersionCount::<Runtime>::get(cid_number);
    let expires_at = <Timestamp as UnixTime>::now().as_secs().saturating_add(300);
    let authorization = identity::CitizenIdentityAuthorization {
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
) -> identity::pallet::SignatureOf<Runtime> {
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
) -> identity::pallet::SignatureOf<Runtime> {
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
    cid_number: &identity::CidNumberBound,
    account_id: &AccountId,
    expires_at: u64,
) -> identity::pallet::SignatureOf<Runtime> {
    let payload = codec::Encode::encode(&identity::CidOccupyAuthorization {
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
fn sign_cid_registrar_rebind(
    signer_pair: &sr25519::Pair,
    cid_number: &identity::CidNumberBound,
    current_account_id: &AccountId,
    new_account_id: &AccountId,
    expected_binding_revision: u64,
    expires_at: u64,
) -> identity::pallet::SignatureOf<Runtime> {
    let payload = codec::Encode::encode(&identity::CidRebindAuthorization {
        genesis_hash: System::block_hash(0),
        cid_number: cid_number.clone(),
        current_account_id: current_account_id.clone(),
        new_account_id: new_account_id.clone(),
        expected_binding_revision,
        expires_at,
    });
    sign_citizen_identity_message(
        signer_pair,
        primitives::sign::OP_SIGN_CID_REGISTRAR_REBIND,
        &payload,
    )
}

/// runtime 集成夹具按链上 Timestamp.Now 签发 300 秒首次绑定授权。
fn cid_authorization_expires_at() -> u64 {
    Timestamp::get().saturating_div(1_000).saturating_add(300)
}

/// 仅在测试外部执行环境为钱包准备符合协议的人员身份。
fn fixture_person(wallet: &AccountId, kind: &str) -> identity::CidNumberBound {
    if let Some(person_cid) = identity::Pallet::<Runtime>::person_cid(wallet) {
        return person_cid;
    }
    let raw = real_cid_number(&alloc::format!("fixture-person-{wallet:?}"), kind, "1");
    let cid_number = test_cid_number(&raw);
    identity::AccountIdByCid::<Runtime>::insert(&cid_number, wallet);
    identity::CidByAccountId::<Runtime>::insert(wallet, &cid_number);
    identity::BindingRevisionByCid::<Runtime>::insert(&cid_number, 1);
    identity::CidRegistry::<Runtime>::insert(&cid_number, identity::CidRecord {
        registrar_cid_number: Default::default(),
        commitment: [0u8; 32],
        residence_province_code: Default::default(),
        residence_city_code: Default::default(),
        status: identity::CidRecordStatus::Active,
        registered_at: 1,
        revoked_at: None,
    });
    cid_number
}

fn fixture_public_role_people(cid: &publicentity::CidNumberOf<Runtime>, role: &[u8], wallets: &[AccountId]) {
    let role = publicentity::RoleCodeOf::try_from(role.to_vec()).unwrap();
    let rows: publicentity::institution::role::RoleAssignmentsOf<Runtime> = wallets.iter().map(|wallet| {
        entitybase::InstitutionAssignment {
            cid_number: cid.clone(),
            person_cid: fixture_person(wallet, "CTZN"),
            role_code: role.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        }
    }).collect::<Vec<_>>().try_into().unwrap();
    publicentity::InstitutionRoleAssignments::<Runtime>::insert(cid, role, rows);
}

/// 用固定机构账户夹具建立指定岗位的人员身份与任职。
// 合成钱包按机构 CID 与席位编号生成，岗位权限仍由测试中的真实绑定与任职装配校验。
fn fixture_seat_wallets(cid: &str) -> Vec<[u8; 32]> {
    let seats = primitives::governance_skeleton::fixed_institution_by_cid(cid.as_bytes())
        .expect("测试机构存在固定席位").expected_len;
    (0..seats)
        .map(|index| sp_io::hashing::blake2_256(&codec::Encode::encode(&(b"test-seat", cid, index))))
        .collect()
}

fn foundation_fixture_wallet() -> [u8; 32] {
    sp_io::hashing::blake2_256(b"foundation-person-fixture")
}

fn fixture_fixed_role_wallets(cid: &str, role: &[u8], wallets: &[[u8; 32]]) {
    let cid = cid.as_bytes().to_vec().try_into().expect("institution CID fits");
    let wallets: Vec<_> = wallets.iter().copied().map(AccountId::new).collect();
    fixture_public_role_people(&cid, role, &wallets);
}

fn setup_frg_citizen_identity_admin(
    province_code: &[u8],
) -> (
    sr25519::Pair,
    AccountId,
    publicentity::pallet::CidNumberOf<Runtime>,
    publicentity::RoleCodeOf,
) {
    let registrar_pair =
        sr25519::Pair::from_string("//frg-citizen-identity-admin", None).expect("registrar pair");
    let registrar = AccountId::new(registrar_pair.public().0);
    let federal_registry = primitives::governance_skeleton::federal_registry_institution();
    let main_account = AccountId::new(federal_registry.main_account);
    // 注册局省级授权由 entity 的“省专员岗位 + 有效任职”表达，不再建立虚拟管理员组。
    let cid_number: publicentity::pallet::CidNumberOf<Runtime> = federal_registry
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("FRG CID fits");
    let account_name: publicentity::pallet::AccountNameOf<Runtime> =
        primitives::account_derive::RESERVED_NAME_MAIN
            .to_vec()
            .try_into()
            .expect("account name fits");
    let person_cid = fixture_person(&registrar, "CTZN");
    publicentity::AccountRegisteredCid::<Runtime>::insert(
        main_account.clone(),
        entitybase::RegisteredInstitution {
            cid_number: cid_number.clone(),
            account_name: account_name.clone(),
        },
    );
    publicentity::InstitutionAccounts::<Runtime>::insert(
        &cid_number,
        account_name,
        entitybase::InstitutionAccountInfo {
            account_id: main_account,
            initial_balance: 0,
            created_at: 0,
        },
    );
    publicentity::Institutions::<Runtime>::insert(
        &cid_number,
        entitybase::InstitutionInfo {
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
            institution_code: primitives::cid::code::FRG,
            created_at: 0,
        },
    );
    let province_code: primitives::cid::code::ProvinceCode =
        province_code.try_into().expect("province code fits");
    let role_code: publicentity::RoleCodeOf =
        primitives::governance_skeleton::province_commissioner_role_code(province_code)
            .try_into()
            .expect("role code fits");
    publicentity::InstitutionRoles::<Runtime>::insert(
        &cid_number,
        &role_code,
        entitybase::InstitutionRole {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
            role_name: "测试省专员"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("role name fits"),
            term_required: true,
            role_status: entitybase::InstitutionRoleStatus::Active,
            seats: primitives::count_const::FRG_COMMISSIONER_SEATS,
        },
    );
    let assignments: publicentity::institution::role::RoleAssignmentsOf<Runtime> =
        vec![entitybase::InstitutionAssignment {
            cid_number: cid_number.clone(),
            person_cid,
            role_code: role_code.clone(),
            // 测试链时间已设置为 2026-07-02；固定任职从第 1 天起永久有效。
            term_start: 1,
            term_end: u32::MAX,
            assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        }]
        .try_into()
        .expect("assignment fits");
    publicentity::InstitutionRoleAssignments::<Runtime>::insert(
        cid_number.clone(),
        role_code.clone(),
        assignments,
    );
    let permission_specs = entitybase::fixed_role_permission_specs(
        primitives::cid::code::FRG,
        cid_number.as_slice(),
        role_code.as_slice(),
    );
    let permissions: publicentity::RolePermissionsOf<Runtime> = permission_specs
        .into_iter()
        .map(|spec| entitybase::RoleBusinessPermission {
            role_subject: entitybase::RoleSubject {
                cid_number: cid_number.clone(),
                role_code: role_code.clone(),
            },
            business_action_id: entitybase::BusinessActionId {
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
    publicentity::InstitutionRolePermissions::<Runtime>::insert(
        cid_number.clone(),
        role_code.clone(),
        permissions,
    );
    assert!(
        <publicentity::Pallet<Runtime> as entitybase::InstitutionRoleQuery<AccountId>>::is_active_assignment(
            cid_number.as_slice(),
            &registrar,
            role_code.as_slice(),
        ),
        "FRG 省专员测试任职必须在测试链日期有效"
    );
    let role_subject = entitybase::RoleSubject {
        cid_number: cid_number.to_vec(),
        role_code: role_code.to_vec(),
    };
    let business_action = entitybase::BusinessActionId {
        module_tag: entitybase::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
        action_code: entitybase::business_action::ACTION_OCCUPY_CID,
    };
    let stored_permissions =
        publicentity::InstitutionRolePermissions::<Runtime>::get(&cid_number, &role_code);
    assert!(
        stored_permissions.iter().any(|permission| {
            permission.role_subject.cid_number.as_slice() == cid_number.as_slice()
                && permission.role_subject.role_code.as_slice() == role_code.as_slice()
                && permission.business_action_id.module_tag.as_slice()
                    == entitybase::business_action::MODULE_CITIZEN_IDENTITY
                && permission.business_action_id.action_code
                    == entitybase::business_action::ACTION_OCCUPY_CID
                && permission.operation == entitybase::RolePermissionOperation::Propose
        }),
        "FRG 省专员测试权限存储必须包含公民 CID 占号提案权限"
    );
    assert!(
        <RuntimeInstitutionCapabilityPolicy as entitybase::InstitutionCapabilityPolicy>::allows(
            cid_number.as_slice(),
            &business_action,
            entitybase::RolePermissionOperation::Propose,
        ),
        "FRG 必须拥有公民 CID 占号顶层能力"
    );
    assert!(
        <publicentity::Pallet<Runtime> as entitybase::InstitutionRoleAuthorizationQuery<
            AccountId,
        >>::role_has_permission(
            &role_subject,
            &business_action,
            entitybase::RolePermissionOperation::Propose,
        ),
        "FRG 省专员岗位必须登记公民 CID 占号权限"
    );
    assert!(
        RuntimeInstitutionRoleAuthorization::is_authorized(
            &registrar,
            &role_subject,
            &business_action,
            entitybase::RolePermissionOperation::Propose,
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

/// 在 `privateentity` 反向索引把 `clearing_account()` 登记为某 SFGF 的「清算账户」。
/// 读写 storage,必须在 `execute_with` 内调用。
fn register_clearing_account() {
    let cid_number: privateentity::CidNumberOf<Runtime> = b"GD001-SCB05-000000002-2026"
        .to_vec()
        .try_into()
        .expect("测试 CID 长度合法");
    let account_name: privateentity::AccountNameOf<Runtime> = "清算账户"
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("清算账户名长度合法");
    privateentity::AccountRegisteredCid::<Runtime>::insert(
        clearing_account(),
        entitybase::RegisteredInstitution {
            cid_number,
            account_name,
        },
    );
}

/// 四个公民身份签名域在普通 runtime 与 `runtime-benchmarks` 下必须保持同一验证结果。
/// 非空且长度正确的伪造签名不能因为 feature 改变而通过。
#[test]
fn citizen_identity_signature_verifiers_reject_nonempty_forgery_in_every_feature() {
    use identity::CitizenIdentityAuthority as _;

    new_test_ext().execute_with(|| {
        let signer_pair = sr25519::Pair::from_seed(&[0x31; 32]);
        let account_id = AccountId::new(signer_pair.public().0);
        let other_account_id = AccountId::new(sr25519::Pair::from_seed(&[0x32; 32]).public().0);
        let payload = b"citizen-identity-verifier-regression";
        let forged_signature: identity::pallet::SignatureOf<Runtime> =
            [0xA5u8; 64].to_vec().try_into().expect("signature fits");
        let short_signature: identity::pallet::SignatureOf<Runtime> =
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
        let registrar_rebind_signature = sign_citizen_identity_message(
            &signer_pair,
            primitives::sign::OP_SIGN_CID_REGISTRAR_REBIND,
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
            RuntimeCitizenIdentityAuthority::verify_registrar_rebind_signature(
                &account_id,
                payload,
                &registrar_rebind_signature,
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
            !RuntimeCitizenIdentityAuthority::verify_registrar_rebind_signature(
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

mod security;

mod migrations;

mod performance;

mod unit;

// 原独立金标从cases迁为共享测试常量，数值保持，不按生产实现推导。
const EXPECTED_PUBLIC_INSTITUTIONS: usize = 49_593;

/// 当前Runtime公开metadata直接导出给SDK测试；不手工改写call9布局。
#[test]
fn export_self_rebind_runtime_metadata() {
    let metadata = Runtime::metadata_at_version(15).expect("Runtime supports metadata v15");
    assert_eq!(&metadata[..5], b"meta\x0f");
    if let Some(path) = std::env::var_os("CITIZENSDK_METADATA_OUTPUT") {
        std::fs::write(path, format!("0x{}\n", hex::encode(metadata.as_slice())))
            .expect("write current SDK metadata fixture");
    }
}

/// 复用交易扩展实际使用的nonce校验/准备实现，旧nonce不得再次接纳或改写状态。
#[test]
fn unified_transaction_nonce_rejects_repeated_execution() {
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionValidityError};
    new_test_ext().execute_with(|| {
        let who = AccountId::new([0x57; 32]);
        System::inc_providers(&who);
        assert!(frame_system::CheckNonce::<Runtime>::validate_nonce_for_account(&who, 0).is_ok());
        assert_ok!(frame_system::CheckNonce::<Runtime>::prepare_nonce_for_account(&who, 0));
        assert_eq!(System::account_nonce(&who), 1);
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(matches!(
            frame_system::CheckNonce::<Runtime>::validate_nonce_for_account(&who, 0),
            Err(TransactionValidityError::Invalid(InvalidTransaction::Stale))
        ));
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

/// 真实Runtime和sr25519校验无期限授权，A→B→A后仍以绑定版本拒绝旧证明。
#[test]
fn unified_self_rebind_real_signatures_survive_clock_gap_but_not_binding_revision() {
    new_test_ext().execute_with(|| {
        let first = sr25519::Pair::from_seed(&[0x61; 32]);
        let second = sr25519::Pair::from_seed(&[0x62; 32]);
        let a = AccountId::new(first.public().0);
        let b = AccountId::new(second.public().0);
        let cid = test_cid_number(&real_cid_number("unified-clock", "CTZN", "CN"));
        assert_ok!(Identity::self_occupy_cid(
            RuntimeOrigin::signed(a.clone()),
            cid.clone()
        ));
        let authorization =
            |old: AccountId, target: AccountId, revision| identity::SelfRebindAuthorization {
                genesis_hash: System::block_hash(0),
                cid_number: cid.clone(),
                current_account_id: old,
                new_account_id: target,
                expected_binding_revision: revision,
            };
        let old = sign_citizen_identity_message(
            &first,
            primitives::sign::OP_SIGN_CID_REBIND,
            &codec::Encode::encode(&authorization(a.clone(), b.clone(), 1)),
        );
        Timestamp::set_timestamp(Timestamp::get() + 3_600_000);
        assert_ok!(Identity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(b.clone()),
            cid.clone(),
            1,
            old.clone()
        ));
        let back = sign_citizen_identity_message(
            &second,
            primitives::sign::OP_SIGN_CID_REBIND,
            &codec::Encode::encode(&authorization(b.clone(), a.clone(), 2)),
        );
        assert_ok!(Identity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(a.clone()),
            cid.clone(),
            2,
            back
        ));
        assert_eq!(
            identity::BindingRevisionByCid::<Runtime>::get(&cid),
            Some(3)
        );
        assert_noop!(
            Identity::self_rebind_cid_account_id(RuntimeOrigin::signed(b), cid, 1, old),
            identity::Error::<Runtime>::BindingRevisionMismatch
        );
    });
}
