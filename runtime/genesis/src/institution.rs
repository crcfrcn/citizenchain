//! 初始化机构、协议账户和固定岗位；有有效绑定的既定任职按人员 CID 写入。

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::Decode;
use frame_support::{pallet_prelude::BoundedVec, traits::Currency};
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
    account_derive::{
        institution_kind_by_name, institution_protocol_account_name, AccountKind,
        InstitutionProtocolAccountKind, RESERVED_NAME_FEE, RESERVED_NAME_MAIN,
    },
    cid::{
        china::{
            china_cb::CHINA_CB,
            china_ch::CHINA_CH,
            china_jc::CHINA_JC,
            china_jy::CHINA_JY,
            china_lf::CHINA_LF,
            china_sf::CHINA_SF,
            china_zf::{CHINA_ZF, FSC_FIXED_ROLES, FSC_GOVERNANCE_THRESHOLD},
            citizenchain::{
                CITIZENCHAIN_FIXED_ROLES, CITIZENCHAIN_FOUNDATION, CITIZENCHAIN_GOVERNANCE_THRESHOLD,
                LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER, LEGAL_REPRESENTATIVE_FAMILY_NAME,
                LEGAL_REPRESENTATIVE_GIVEN_NAME,
            },
        },
        code::{institution_code_from_cid_number, InstitutionCode, FRG, FSC, NJD},
    },
};
use sp_runtime::traits::Zero;

use engingcore::{types::CidNumber, CitizenIdentityReader as _};
use publicentity::{
    InstitutionAccountInfo, InstitutionAssignment, InstitutionAssignmentSource,
    InstitutionAssignmentStatus, InstitutionInfo, InstitutionRole, InstitutionRoleStatus,
    LegalRepresentative, RegisteredInstitution, RESERVED_NAME_FEE as PUBLIC_RESERVED_NAME_FEE,
    RESERVED_NAME_MAIN as PUBLIC_RESERVED_NAME_MAIN,
};

use crate::roles;

type PublicBalanceOf<T> = <<T as publicentity::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;
type PublicCidNumberOf<T> = BoundedVec<u8, <T as publicentity::Config>::MaxCidNumberLength>;
type PublicAccountNameOf<T> = BoundedVec<u8, <T as publicentity::Config>::MaxAccountNameLength>;
type PublicInstitutionInfoOf<T> = InstitutionInfo<
    BlockNumberFor<T>,
    PublicAccountNameOf<T>,
    PublicCidNumberOf<T>,
>;
type PublicInstitutionAccountInfoOf<T> = InstitutionAccountInfo<
    <T as frame_system::Config>::AccountId,
    PublicBalanceOf<T>,
    BlockNumberFor<T>,
>;
type PublicRegisteredInstitutionOf<T> =
    RegisteredInstitution<PublicCidNumberOf<T>, PublicAccountNameOf<T>>;
type PrivateBalanceOf<T> = <<T as privateentity::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;
type PrivateCidNumberOf<T> = BoundedVec<u8, <T as privateentity::Config>::MaxCidNumberLength>;
type PrivateAccountNameOf<T> = BoundedVec<u8, <T as privateentity::Config>::MaxAccountNameLength>;
type PrivateInstitutionInfoOf<T> = privateentity::InstitutionInfo<
    BlockNumberFor<T>,
    PrivateAccountNameOf<T>,
    PrivateCidNumberOf<T>,
>;
type PrivateInstitutionAccountInfoOf<T> = privateentity::InstitutionAccountInfo<
    <T as frame_system::Config>::AccountId,
    PrivateBalanceOf<T>,
    BlockNumberFor<T>,
>;
type PrivateRegisteredInstitutionOf<T> =
    privateentity::RegisteredInstitution<PrivateCidNumberOf<T>, PrivateAccountNameOf<T>>;

fn decode_account<T: frame_system::Config>(raw: &[u8; 32], label: &str) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..])
        .unwrap_or_else(|_| panic!("genesis institution: {} 账户 decode 失败", label))
}

fn bounded_cid<T: publicentity::Config>(cid_number: &'static str) -> PublicCidNumberOf<T> {
    cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .unwrap_or_else(|_| {
            panic!(
                "genesis institution: cid_number {} 超过 MaxCidNumberLength",
                cid_number
            )
        })
}

fn bounded_account_name<T: publicentity::Config>(
    bytes: &[u8],
    label: &str,
    cid_number: &'static str,
) -> PublicAccountNameOf<T> {
    bytes.to_vec().try_into().unwrap_or_else(|_| {
        panic!(
            "genesis institution: cid_number {} {} 超过 MaxAccountNameLength",
            cid_number, label
        )
    })
}

fn bounded_static_name<T: publicentity::Config>(
    value: &'static str,
    label: &str,
    cid_number: &'static str,
) -> PublicAccountNameOf<T> {
    bounded_account_name::<T>(value.as_bytes(), label, cid_number)
}

fn insert_public_account<T: publicentity::Config>(
    cid: &PublicCidNumberOf<T>,
    account_name: PublicAccountNameOf<T>,
    account_id: T::AccountId,
) {
    publicentity::InstitutionAccounts::<T>::insert(
        cid,
        &account_name,
        PublicInstitutionAccountInfoOf::<T> {
            account_id: account_id.clone(),
            initial_balance: PublicBalanceOf::<T>::zero(),
            created_at: BlockNumberFor::<T>::default(),
        },
    );
    publicentity::AccountRegisteredCid::<T>::insert(
        &account_id,
        PublicRegisteredInstitutionOf::<T> {
            cid_number: cid.clone(),
            account_name,
        },
    );
    <<T as publicentity::Config>::OnAccountRegistered as frame_support::traits::OnNewAccount<T::AccountId,
    >>::on_new_account(&account_id);
}

/// 模板派生机构落地：账户由 CID 号确定性派生，名称为运行态 String。
/// 构建期断言 CID 格式合法、公权家族且协议账户集合完整。
fn insert_derived_public_institution<T: publicentity::Config>(
    cid_number: &str,
    cid_full_name: &str,
    cid_short_name: &str,
) {
    let parts = primitives::cid::number::parse_cid_number_parts(cid_number)
        .unwrap_or_else(|e| panic!("genesis derived cid {cid_number} 非法: {e}"));
    assert!(
        primitives::cid::code::is_public_legal_code(&parts.institution),
        "genesis derived cid {cid_number} 非公权家族"
    );
    let cid: PublicCidNumberOf<T> = cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .unwrap_or_else(|_| panic!("genesis derived cid {cid_number} 超过 MaxCidNumberLength"));
    let bounded_name =
        |value: &str| -> PublicAccountNameOf<T> {
            value.as_bytes().to_vec().try_into().unwrap_or_else(|_| {
                panic!("genesis derived name {value} 超过 MaxAccountNameLength")
            })
        };
    publicentity::Institutions::<T>::insert(
        &cid,
        PublicInstitutionInfoOf::<T> {
            cid_full_name: bounded_name(cid_full_name),
            cid_short_name: bounded_name(cid_short_name),
            town_code: BoundedVec::new(),
            // 法定代表人不是创世必填项：创世字段为空，后续依法任命时原子写入。
            // 人员无有效绑定时，法定代表人席位保持空缺。
            legal_representative: None,
            institution_code: parts.institution,
            created_at: BlockNumberFor::<T>::default(),
        },
    );
    publicentity::Pallet::<T>::store_default_legal_representative_role(&cid)
        .unwrap_or_else(|_| panic!("genesis derived cid {cid_number} 默认法定代表人岗位写入失败"));
    let bounded_reserved = |value: &[u8]| -> PublicAccountNameOf<T> {
        value
            .to_vec()
            .try_into()
            .expect("reserved account_id name fits")
    };
    let cid_bytes = cid_number.as_bytes();
    let main = AccountKind::InstitutionMain {
        cid_number: cid_bytes,
    }
    .derive(primitives::core_const::SS58_FORMAT);
    let fee = AccountKind::InstitutionFee {
        cid_number: cid_bytes,
    }
    .derive(primitives::core_const::SS58_FORMAT);
    let institution_code = parts.institution;
    // 创世不铸非法人组织(UNIN)，无父级可传。
    let required = primitives::institution_constraints::required_protocol_account_kinds(
        institution_code,
        cid_bytes,
        None,
    )
    .expect("创世派生机构 CID 与机构码必须一致");
    for kind in required {
        let name = institution_protocol_account_name(*kind);
        let account_kind =
            institution_kind_by_name(cid_bytes, name).expect("协议账户名必须映射到唯一派生类型");
        let account_id = match kind {
            InstitutionProtocolAccountKind::Main => main,
            InstitutionProtocolAccountKind::Fee => fee,
            _ => account_kind.derive(primitives::core_const::SS58_FORMAT),
        };
        insert_derived_account::<T>(
            &cid,
            bounded_reserved(name),
            decode_account::<T>(&account_id, "派生协议账户"),
        );
    }
}

/// 派生机构账户落地到 CID 正向账户真源和地址反向索引。
fn insert_derived_account<T: publicentity::Config>(
    cid: &PublicCidNumberOf<T>,
    account_name: PublicAccountNameOf<T>,
    account_id: T::AccountId,
) {
    publicentity::InstitutionAccounts::<T>::insert(
        cid,
        &account_name,
        PublicInstitutionAccountInfoOf::<T> {
            account_id: account_id.clone(),
            initial_balance: PublicBalanceOf::<T>::zero(),
            created_at: BlockNumberFor::<T>::default(),
        },
    );
    publicentity::AccountRegisteredCid::<T>::insert(
        &account_id,
        PublicRegisteredInstitutionOf::<T> {
            cid_number: cid.clone(),
            account_name,
        },
    );
    <<T as publicentity::Config>::OnAccountRegistered as frame_support::traits::OnNewAccount<T::AccountId,
    >>::on_new_account(&account_id);
}

fn insert_public_institution<T: publicentity::Config>(
    cid_number: &'static str,
    cid_full_name: &'static str,
    cid_short_name: &'static str,
    main_account: [u8; 32],
    fee_account: [u8; 32],
) {
    let cid = bounded_cid::<T>(cid_number);
    let institution_code = institution_code_from_cid_number(cid_number).unwrap_or_else(|| {
        panic!(
            "genesis institution: cid_number {} 机构码解析失败",
            cid_number
        )
    });
    publicentity::Institutions::<T>::insert(
        &cid,
        PublicInstitutionInfoOf::<T> {
            cid_full_name: bounded_static_name::<T>(cid_full_name, "cid_full_name", cid_number),
            cid_short_name: bounded_static_name::<T>(cid_short_name, "cid_short_name", cid_number),
            town_code: BoundedVec::new(),
            // 固定创世机构同样允许尚未任命法定代表人；字段必须保持为空。
            legal_representative: None,
            institution_code,
            created_at: BlockNumberFor::<T>::default(),
        },
    );
    publicentity::Pallet::<T>::store_default_legal_representative_role(&cid).unwrap_or_else(
        |_| {
            panic!(
                "genesis institution: {} 默认法定代表人岗位写入失败",
                cid_number
            )
        });
    // 联邦安全局创世岗位 = 自带的 LR + 局长(创世期不设任期,任期由运行期业务模块规范)。
    // 两岗在此建为空缺,其多签转账 Propose+Vote 权限由固定权限目录随建岗写入;
    // 程伟的实际任职由 insert_fsc_genesis_governance 补齐。
    if institution_code == FSC {
        publicentity::Pallet::<T>::store_genesis_director_role(&cid)
            .unwrap_or_else(|_| panic!("genesis institution: {} 局长岗位写入失败", cid_number));
    }
    insert_public_account::<T>(
        &cid,
        bounded_account_name::<T>(RESERVED_NAME_MAIN, "主账户名", cid_number),
        decode_account::<T>(&main_account, "主账户"),
    );
    insert_public_account::<T>(
        &cid,
        bounded_account_name::<T>(RESERVED_NAME_FEE, "费用账户名", cid_number),
        decode_account::<T>(&fee_account, "费用账户"),
    );
    // 创世不铸非法人组织(UNIN)，无父级可传。
    let required = primitives::institution_constraints::required_protocol_account_kinds(
        institution_code,
        cid_number.as_bytes(),
        None,
    )
    .expect("固定机构 CID 与机构码必须一致");
    for kind in required {
        if matches!(
            kind,
            InstitutionProtocolAccountKind::Main | InstitutionProtocolAccountKind::Fee
        ) {
            continue;
        }
        let account_name = institution_protocol_account_name(*kind);
        let account_kind = institution_kind_by_name(cid_number.as_bytes(), account_name)
            .expect("协议账户名必须映射到唯一派生类型");
        insert_public_account::<T>(
            &cid,
            bounded_account_name::<T>(account_name, "协议账户名", cid_number),
            decode_account::<T>(
                &account_kind.derive(primitives::core_const::SS58_FORMAT),
                "协议账户",
            ),
        );
    }
    assert_eq!(RESERVED_NAME_MAIN, PUBLIC_RESERVED_NAME_MAIN);
    assert_eq!(RESERVED_NAME_FEE, PUBLIC_RESERVED_NAME_FEE);
}

fn insert_fixed_roles<T>(cid_number: &'static str, institution_code: InstitutionCode)
where
    T: publicentity::Config,
{
    let cid = bounded_cid::<T>(cid_number);
    let roles: Vec<_> = roles::fixed_seats(institution_code)
        .into_iter()
        .map(|(role_code, role_name, seats)| InstitutionRole {
            cid_number: cid.clone(),
            role_code: role_code.try_into().expect("固定岗位码符合上限"),
            role_name: bounded_account_name::<T>(&role_name, "岗位名称", cid_number),
            term_required: false,
            role_status: InstitutionRoleStatus::Active,
            seats,
        })
        .collect();

    let roles: publicentity::institution::role::InstitutionRolesOf<T> = roles
        .try_into()
        .unwrap_or_else(|_| panic!("genesis institution: {} 岗位数量超限", cid_number));
    let assignments = Default::default();
    publicentity::Pallet::<T>::store_genesis_roles_and_assignments(&cid, &roles, &assignments)
        .unwrap_or_else(|_| panic!("genesis institution: {} 岗位任职写入失败", cid_number));
    for role in &roles {
        publicentity::Pallet::<T>::store_genesis_fixed_role_permissions(&cid, &role.role_code)
            .unwrap_or_else(|_| panic!("genesis institution: {} 固定岗位权限写入失败", cid_number));
    }
    let legal_representative_role: publicentity::institution::role::RoleCodeOf =
        primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
            .to_vec()
            .try_into()
            .unwrap_or_else(|_| panic!("genesis institution: {} LR 岗位代码过长", cid_number));
    publicentity::Pallet::<T>::store_genesis_fixed_role_permissions(
        &cid,
        &legal_representative_role,
    )
    .unwrap_or_else(|_| panic!("genesis institution: {} LR 空权限写入失败", cid_number));

    let threshold = primitives::cid::code::fixed_governance_pass_threshold(&institution_code)
        .unwrap_or_else(|| panic!("genesis institution: {cid_number} 缺少固定机构阈值"));
    publicentity::InstitutionGovernanceThresholds::<T>::insert(&cid, threshold);
}

/// 写入联邦安全局(FSC)创世治理。
///
/// 机构身份和账户(含联邦公民安全基金)已由 `insert_public_institution` 写入;这里补齐:
/// 法定代表人和局长初始任职、对应人员 CID 及法定代表人公开资料、
/// 以及 2/2 严格过半内部治理阈值。两岗的多签转账 Propose+Vote 权限由固定权限目录经
/// `store_genesis_fixed_role_permissions` 写入,使 FSC 可经内部投票从该基金转出。
fn insert_fsc_genesis_governance<T>(cid_number: &'static str)
where
    T: publicentity::Config,
{
    let cid = bounded_cid::<T>(cid_number);
    let institution_code = institution_code_from_cid_number(cid_number)
        .expect("genesis institution: FSC cid_number 机构码解析失败");
    assert_eq!(
        institution_code, FSC,
        "genesis institution: insert_fsc_genesis_governance 只服务联邦安全局"
    );
    assert_eq!(
        FSC_FIXED_ROLES.len(),
        2,
        "genesis institution: FSC 必须恰好两条创世任职(LR + 局长)"
    );
    assert_eq!(
        FSC_GOVERNANCE_THRESHOLD, 2,
        "genesis institution: FSC 两席严格过半阈值必须为 2"
    );

    // 法定代表人任命的公开事实:原子写入 legal_representative(程伟)。
    let lr_cid: PublicCidNumberOf<T> = LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("genesis institution: FSC 法定代表人公民 CID 超过协议上限");
    let person_cid = CidNumber::try_from(lr_cid.to_vec()).expect("法定代表人 CID 符合上限");
    if T::CitizenIdentityReader::person_account(&person_cid).is_some() {
        publicentity::Institutions::<T>::mutate(&cid, |maybe| {
            let info = maybe.as_mut().expect("安全局必须已登记");
            info.legal_representative = Some(LegalRepresentative {
                family_name: bounded_account_name::<T>(LEGAL_REPRESENTATIVE_FAMILY_NAME.as_bytes(), "法定代表人姓", cid_number),
                given_name: bounded_account_name::<T>(LEGAL_REPRESENTATIVE_GIVEN_NAME.as_bytes(), "法定代表人名", cid_number),
                person_cid: lr_cid.clone(),
            });
        });
        for role in FSC_FIXED_ROLES {
            let role_code: publicentity::RoleCodeOf = role.to_vec().try_into().expect("固定岗位码符合上限");
            let rows: publicentity::institution::role::RoleAssignmentsOf<T> = vec![InstitutionAssignment {
                cid_number: cid.clone(), person_cid: lr_cid.clone(), role_code: role_code.clone(),
                term_start: 0, term_end: 0,
                assignment_source: InstitutionAssignmentSource::Genesis,
                assignment_source_ref: Default::default(),
                assignment_status: InstitutionAssignmentStatus::Active,
            }].try_into().expect("安全局每岗一席");
            publicentity::InstitutionRoleAssignments::<T>::insert(&cid, role_code, rows);
        }
    }

    // FSC 内部治理阈值(2/2 严格过半)。
    publicentity::InstitutionGovernanceThresholds::<T>::insert(&cid, FSC_GOVERNANCE_THRESHOLD);
}

/// 写入公民链技术发展基金会正式创世状态。
///
/// 机构身份、主账户及费用账户、三个固定岗位、法定代表人资料和 2/3
/// 内部治理阈值在同一创世构建中完成，任何一项不一致都直接中止创世。
fn insert_citizenchain_foundation<T>()
where
    T: privateentity::Config,
{
    let foundation = CITIZENCHAIN_FOUNDATION;
    let parts = primitives::cid::number::parse_cid_number_parts(foundation.cid_number)
        .unwrap_or_else(|err| panic!("genesis citizenchain: 基金会 CID 非法: {err}"));
    assert_eq!(
        parts.institution, *b"SFGY",
        "genesis citizenchain: 基金会必须属于公益组织"
    );
    assert_eq!(
        CITIZENCHAIN_FIXED_ROLES.len(),
        3,
        "genesis citizenchain: 必须恰好三个固定岗位"
    );
    assert_eq!(
        CITIZENCHAIN_GOVERNANCE_THRESHOLD, 2,
        "genesis citizenchain: 三个岗位席位必须采用 2/3 严格多数"
    );

    let cid: PrivateCidNumberOf<T> = foundation
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .unwrap_or_else(|_| panic!("genesis citizenchain: 基金会 CID 超过协议上限"));
    let bounded_name = |value: &[u8], label: &str| -> PrivateAccountNameOf<T> {
        value
            .to_vec()
            .try_into()
            .unwrap_or_else(|_| panic!("genesis citizenchain: {label} 超过 MaxAccountNameLength"))
    };
    let legal_representative_parts =
        primitives::cid::number::parse_cid_number_parts(LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER)
            .unwrap_or_else(|err| panic!("genesis citizenchain: 法定代表人公民 CID 非法: {err}"));
    assert_eq!(
        legal_representative_parts.institution, *b"CTZN",
        "genesis citizenchain: 法定代表人必须使用公民 CID"
    );
    let legal_representative_cid: PrivateCidNumberOf<T> = LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("genesis citizenchain: 法定代表人公民 CID 超过协议上限");

    let person_cid = CidNumber::try_from(legal_representative_cid.to_vec()).expect("法定代表人 CID 符合上限");
    let has_person = T::CitizenIdentityReader::person_account(&person_cid).is_some();

    privateentity::Institutions::<T>::insert(
        &cid,
        PrivateInstitutionInfoOf::<T> {
            cid_full_name: bounded_name(foundation.cid_full_name.as_bytes(), "机构全称"),
            cid_short_name: bounded_name(foundation.cid_short_name.as_bytes(), "机构简称"),
            town_code: BoundedVec::new(),
            legal_representative: has_person.then(|| LegalRepresentative {
                family_name: bounded_name(
                    LEGAL_REPRESENTATIVE_FAMILY_NAME.as_bytes(),
                    "法定代表人姓",
                ),
                given_name: bounded_name(
                    LEGAL_REPRESENTATIVE_GIVEN_NAME.as_bytes(),
                    "法定代表人名",
                ),
                person_cid: legal_representative_cid.clone(),
            }),
            institution_code: parts.institution,
            created_at: BlockNumberFor::<T>::default(),
        },
    );

    for (account_name, raw_account) in [
        (RESERVED_NAME_MAIN, foundation.main_account),
        (RESERVED_NAME_FEE, foundation.fee_account),
    ] {
        let account_name = bounded_name(account_name, "协议账户名");
        let account_id = decode_account::<T>(&raw_account, "基金会协议账户");
        privateentity::InstitutionAccounts::<T>::insert(
            &cid,
            &account_name,
            PrivateInstitutionAccountInfoOf::<T> {
                account_id: account_id.clone(),
                initial_balance: PrivateBalanceOf::<T>::zero(),
                created_at: BlockNumberFor::<T>::default(),
            },
        );
        privateentity::AccountRegisteredCid::<T>::insert(
            &account_id,
            PrivateRegisteredInstitutionOf::<T> {
                cid_number: cid.clone(),
                account_name,
            },
        );
        <<T as privateentity::Config>::OnAccountRegistered as frame_support::traits::OnNewAccount<T::AccountId>>::on_new_account(&account_id);
    }

    let mut roles: Vec<privateentity::institution::role::InstitutionRoleOf<T>> = Vec::new();
    let mut assignments: Vec<privateentity::institution::role::InstitutionAssignmentOf<T>> =
        Vec::new();
    for role in CITIZENCHAIN_FIXED_ROLES {
        let role_code: privateentity::institution::role::RoleCodeOf = role
            .role_code
            .to_vec()
            .try_into()
            .expect("genesis citizenchain: 固定岗位代码超过协议上限");
        roles.push(privateentity::InstitutionRole {
            cid_number: cid.clone(),
            role_code: role_code.clone(),
            role_name: bounded_name(role.role_name, "固定岗位名称"),
            term_required: false,
            role_status: privateentity::InstitutionRoleStatus::Active,
            seats: role.seats,
        });
        if has_person {
            assignments.push(privateentity::InstitutionAssignment {
            cid_number: cid.clone(),
            person_cid: legal_representative_cid.clone(),
            role_code,
            term_start: 0,
            term_end: 0,
            assignment_source: privateentity::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: privateentity::InstitutionAssignmentStatus::Active,
            });
        }
    }
    let roles: privateentity::institution::role::InstitutionRolesOf<T> = roles
        .try_into()
        .expect("genesis citizenchain: 固定岗位数量超过协议上限");
    let assignments: privateentity::institution::role::InstitutionAssignmentsOf<T> =
        assignments
            .try_into()
            .expect("genesis citizenchain: 固定岗位任职数量超过协议上限");
    privateentity::Pallet::<T>::store_genesis_roles_and_assignments(&cid, &roles, &assignments)
        .expect("genesis citizenchain: 固定岗位和任职写入失败");
    for role in &roles {
        privateentity::Pallet::<T>::store_genesis_fixed_role_permissions(&cid, &role.role_code)
            .expect("genesis citizenchain: 固定岗位权限写入失败");
    }
    privateentity::InstitutionGovernanceThresholds::<T>::insert(
        &cid,
        CITIZENCHAIN_GOVERNANCE_THRESHOLD,
    );

}

/// 创世写入内置公权机构和创世公职人员。
/// 创世直铸国家/省/市公权机构（注册与创世目录）：纯枚举（primitives 单源）。
/// → 落地存储;账户由 CID 号确定性派生,与 296 常量互不重号。
fn build_template_institutions<T: publicentity::Config>() {
    primitives::cid::official_derive::for_each_public_institution(|cid, full, short| {
        insert_derived_public_institution::<T>(cid, full, short);
    });
}

pub fn build<T>()
where
    T: publicentity::Config + privateentity::Config,
{
    for node in CHINA_CB.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
        let institution_code = institution_code_from_cid_number(node.cid_number)
            .expect("china_cb cid_number must encode institution code");
        insert_fixed_roles::<T>(node.cid_number, institution_code);
    }

    for node in CHINA_CH.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
        let institution_code = institution_code_from_cid_number(node.cid_number)
            .expect("china_ch cid_number must encode institution code");
        insert_fixed_roles::<T>(node.cid_number, institution_code);
    }

    // 常量数组全量直铸(注册与创世目录):ZF/JC/SF/LF/JY 逐节点写入机构+双账户,
    // 与上方 CB/CH 合计 296;NJD 和 FRG 的固定岗位在下方初始化。
    for node in CHINA_ZF.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
    }
    for node in CHINA_JC.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
    }
    for node in CHINA_SF.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
    }
    for node in CHINA_LF.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
    }
    for node in CHINA_JY.iter() {
        insert_public_institution::<T>(
            node.cid_number,
            node.cid_full_name,
            node.cid_short_name,
            node.main_account,
            node.fee_account,
        );
    }

    // NJD 的四类司法固定岗位。
    let njd_node = CHINA_SF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(NJD))
        .expect("china_sf must include NJD");
    insert_fixed_roles::<T>(njd_node.cid_number, NJD);

    // FRG 是一个机构，按省设置 43 个固定专员岗位。
    let frg_node = CHINA_ZF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(FRG))
        .expect("china_zf must include FRG");
    insert_fixed_roles::<T>(frg_node.cid_number, FRG);

    // 联邦安全局(FSC)创世治理:程伟任 LR + 局长两岗、配 2/2 严格过半阈值,
    // 使联邦公民安全基金可经 FSC 内部投票转出。
    let fsc_node = CHINA_ZF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(FSC))
        .expect("china_zf must include FSC");
    insert_fsc_genesis_governance::<T>(fsc_node.cid_number);

    // 创世直铸当前国家/省/市公权机构（注册与创世目录）：常量 296 + 派生 49,297。
    build_template_institutions::<T>();

    // 基金会归私权机构，不计入公权机构目录。
    insert_citizenchain_foundation::<T>();
}
