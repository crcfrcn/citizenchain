//! 完整机构调用基准：setup只准备所属临时状态，计时内执行真实注册局/岗位授权。
#![cfg(feature = "runtime-benchmarks")]
use crate::{pallet::*, Call, Config, InstitutionAccountInfo, Pallet, RegisteredInstitution};
use adminbase::{Admin, InstitutionAdminLifecycle, InstitutionAdminQuery as _};
extern crate alloc;
use alloc::format;
use codec::Encode;
use entitybase::{
    BusinessActionId, InstitutionAdminAssignment, InstitutionAssignmentSource,
    InstitutionAssignmentStatus, InstitutionRole, InstitutionRoleStatus, RoleBusinessPermission,
    RolePermissionOperation, RoleSubject,
};
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::{
    sp_std::{vec, vec::Vec},
    traits::SaturatedConversion,
};

/// 注册局上下文由Runtime复用其真实地址授权准备入口；不修改生产鉴权接口。
pub struct Registry<AccountId> {
    pub caller: AccountId,
    pub cid: Vec<u8>,
    pub role: Vec<u8>,
    pub province: Vec<u8>,
    pub city: Vec<u8>,
}
pub trait BenchmarkSetup: Config {
    fn registry() -> Registry<Self::AccountId>;
}
struct Fixture<T: Config> {
    registry: Registry<T::AccountId>,
    cid: CidNumberOf<T>,
    caller: T::AccountId,
    role: crate::institution::role::RoleCodeOf,
    admins: InstitutionAdminsInputOf<T>,
}
fn prepare<T: BenchmarkSetup>() -> Fixture<T> {
    frame_system::Pallet::<T>::set_block_number(1u32.into());
    let registry = T::registry();
    let province = core::str::from_utf8(&registry.province).unwrap();
    let city = core::str::from_utf8(&registry.city).unwrap();
    let number = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "weights-publicentity",
            p1: "0",
            province_code: province,
            province_name: "基准省",
            city_code: city,
            city_name: "基准市",
            year: 2026,
            institution: "CGOV",
        },
    )
    .unwrap();
    let cid: CidNumberOf<T> = number.into_bytes().try_into().unwrap();
    let role: crate::institution::role::RoleCodeOf = b"BENCHMARK_ROLE".to_vec().try_into().unwrap();
    let count = <T as Config>::MaxAdmins::get();
    let mut admins: InstitutionAdminsInputOf<T> = (0..count)
        .map(|i| Admin {
            account_id: account("entity-admin", i, 0),
            cid_number: Default::default(),
            family_name: vec![b'F'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                .try_into()
                .unwrap(),
            given_name: vec![b'G'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                .try_into()
                .unwrap(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let caller = admins[0].account_id.clone();
    Institutions::<T>::insert(
        &cid,
        crate::InstitutionInfo {
            cid_full_name: b"Benchmark".to_vec().try_into().unwrap(),
            cid_short_name: b"Bench".to_vec().try_into().unwrap(),
            town_code: Default::default(),
            legal_representative: None,
            institution_code: *b"CGOV",
            created_at: 1u32.into(),
        },
    );
    <T as Config>::AdminLifecycle::benchmark_set_institution_admins(
        crate::MODULE_TAG,
        cid.to_vec(),
        *b"CGOV",
        adminbase::AdminAccountKind::PublicInstitution,
        admins.to_vec(),
    )
    .unwrap();
    InstitutionGovernanceThresholds::<T>::insert(&cid, count / 2 + 1);
    InstitutionRoles::<T>::insert(
        &cid,
        &role,
        InstitutionRole {
            cid_number: cid.clone(),
            role_code: role.clone(),
            role_name: b"Benchmark".to_vec().try_into().unwrap(),
            term_required: false,
            role_status: InstitutionRoleStatus::Active,
        },
    );
    let assignments: crate::institution::role::RoleAssignmentsOf<T> = admins
        .iter()
        .map(|a| InstitutionAdminAssignment {
            cid_number: cid.clone(),
            account_id: a.account_id.clone(),
            role_code: role.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    InstitutionRoleAssignments::<T>::insert(&cid, &role, assignments);
    let permissions: crate::institution::role::RolePermissionsOf<T> = [
        entitybase::business_action::ACTION_INSTITUTION_CLOSE,
        entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE,
    ]
    .into_iter()
    .flat_map(|action| {
        [
            RolePermissionOperation::Propose,
            RolePermissionOperation::Vote,
        ]
        .map(|operation| RoleBusinessPermission {
            role_subject: RoleSubject {
                cid_number: cid.clone(),
                role_code: role.clone(),
            },
            business_action_id: BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec().try_into().unwrap(),
                action_code: action,
            },
            operation,
        })
    })
    .collect::<Vec<_>>()
    .try_into()
    .unwrap();
    InstitutionRolePermissions::<T>::insert(&cid, &role, permissions);
    for admin in admins.iter_mut() {
        admin.family_name = b"F".to_vec().try_into().unwrap();
        admin.given_name = b"G".to_vec().try_into().unwrap();
    }
    Fixture {
        registry,
        cid,
        caller,
        role,
        admins,
    }
}
#[cfg(test)]
impl BenchmarkSetup for crate::tests::Test {
    fn registry() -> Registry<Self::AccountId> {
        Registry {
            caller: crate::tests::creator_account_id(),
            cid: b"REGISTRY-CID".to_vec(),
            role: b"REGISTRY-ROLE".to_vec(),
            province: b"GD".to_vec(),
            city: b"001".to_vec(),
        }
    }
}

struct RegistrationFixture<T: Config> {
    registry: Registry<T::AccountId>,
    cid: CidNumberOf<T>,
    action: InstitutionGovernanceActionOf<T>,
    state: [u8; 32],
    threshold: Option<u32>,
    #[cfg(feature = "std")]
    work: (u32, u32, u32, u32),
}

/// 钱包先独立登记，n为准确名册规模，a为每岗任职规模；首次是首次岗位配置。
/// 校准时必须使用work的真实计数，而非把r*p/r*a误当作单个p/a。
fn registration_fixture<T: BenchmarkSetup>(
    n: u32,
    r: u32,
    p: u32,
    a: u32,
    existing: bool,
    operation: bool,
) -> RegistrationFixture<T> {
    let mut f = prepare::<T>();
    assert!(n.max(a) <= T::MaxAdmins::get() && r < T::MaxAdmins::get());
    let mut admins = f
        .admins
        .into_iter()
        .take(n.max(a) as usize)
        .collect::<Vec<_>>();
    for admin in &mut admins {
        admin.family_name = vec![b'F'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
            .try_into()
            .unwrap();
        admin.given_name = vec![b'G'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
            .try_into()
            .unwrap();
    }

    if operation {
        // ABI夹具只在基准externalities内建立实名投票身份。计时内仍执行真实四要素和闭环绑定校验。
        fn key(item: &[u8], encoded: &[u8]) -> Vec<u8> {
            let mut key = frame_support::storage::storage_prefix(b"CitizenIdentity", item).to_vec();
            key.extend(
                <frame_support::Blake2_128Concat as frame_support::StorageHasher>::hash(encoded),
            );
            key
        }
        fn seed<T: Config>(
            admin: &mut Admin<T::AccountId>,
            number: u32,
            registry: &Registry<T::AccountId>,
        ) {
            let block: frame_system::pallet_prelude::BlockNumberFor<T> = 1u32.into();

            let payload = format!("CN001CTZN{number:09}2026");
            let cid = format!(
                "CN001-CTZN{}-{number:09}-2026",
                primitives::cid::number::checksum_char_m1(&payload, true)
            )
            .into_bytes();
            admin.cid_number = cid.clone().try_into().unwrap();
            frame_support::storage::unhashed::put(
                &key(b"AccountIdByCid", &cid.encode()),
                &admin.account_id,
            );
            frame_support::storage::unhashed::put(
                &key(b"CidByAccountId", &admin.account_id.encode()),
                &cid,
            );
            frame_support::storage::unhashed::put(
                &key(b"BindingRevisionByCid", &cid.encode()),
                &1u64,
            );
            frame_support::storage::unhashed::put(
                &key(b"CidRegistry", &cid.encode()),
                &(
                    registry.cid.clone(),
                    [0u8; 32],
                    registry.province.clone(),
                    registry.city.clone(),
                    0u8,
                    block,
                    None::<frame_system::pallet_prelude::BlockNumberFor<T>>,
                ),
            );
            frame_support::storage::unhashed::put(
                &key(b"VotingIdentityByCid", &cid.encode()),
                &(
                    20200101u32,
                    20360101u32,
                    0u8,
                    registry.province.clone(),
                    registry.city.clone(),
                    Vec::<u8>::new(),
                    block,
                ),
            );
        }
        // 最大合法CREG名册、调用者排最后且全部有CID，覆盖签名者解析的最坏重复读取。
        // 两个R_岗位均按登记引用生成，只属于基准临时状态，不引入固定业务岗位。
        let actor_cid = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "weights-operation-registrar",
                p1: "0",
                province_code: core::str::from_utf8(&f.registry.province).unwrap(),
                province_name: "基准省",
                city_code: core::str::from_utf8(&f.registry.city).unwrap(),
                city_name: "基准市",
                year: 2026,
                institution: "CREG",
            },
        )
        .unwrap()
        .into_bytes();
        let actor_cid: CidNumberOf<T> = actor_cid.try_into().unwrap();
        let mut actor_admins = (0..T::MaxAdmins::get())
            .map(|index| Admin {
                account_id: account("operation-registrar", index, 0),
                cid_number: Default::default(),
                family_name: vec![b'A'; 128].try_into().unwrap(),
                given_name: vec![b'B'; 128].try_into().unwrap(),
            })
            .collect::<Vec<_>>();
        for (index, admin) in actor_admins.iter_mut().enumerate() {
            seed::<T>(admin, 710_000_000 + index as u32, &f.registry);
        }
        let caller = actor_admins.last().unwrap().account_id.clone();
        Institutions::<T>::insert(
            &actor_cid,
            crate::InstitutionInfo {
                cid_full_name: b"Benchmark registrar".to_vec().try_into().unwrap(),
                cid_short_name: Default::default(),
                town_code: Default::default(),
                legal_representative: None,
                institution_code: *b"CREG",
                created_at: 1u32.into(),
            },
        );
        T::AdminLifecycle::benchmark_set_institution_admins(
            crate::MODULE_TAG,
            actor_cid.to_vec(),
            *b"CREG",
            adminbase::AdminAccountKind::PublicInstitution,
            actor_admins,
        )
        .unwrap();
        let mut actor_role = None;
        for index in 0..2u64 {
            let code: crate::institution::role::RoleCodeOf =
                entitybase::institution_role::generate_registration_role_code(
                    crate::MODULE_TAG,
                    &actor_cid,
                    index,
                    b"benchmark-operation-actor",
                )
                .try_into()
                .unwrap();
            let role_name = format!("基准动态岗位{index}")
                .into_bytes()
                .try_into()
                .unwrap();
            InstitutionRoles::<T>::insert(
                &actor_cid,
                &code,
                InstitutionRole {
                    cid_number: actor_cid.clone(),
                    role_code: code.clone(),
                    role_name,
                    term_required: false,
                    role_status: InstitutionRoleStatus::Active,
                },
            );
            let mut perms = vec![RoleBusinessPermission {
                role_subject: RoleSubject {
                    cid_number: actor_cid.clone(),
                    role_code: code.clone(),
                },
                business_action_id: BusinessActionId {
                    module_tag: crate::MODULE_TAG.to_vec().try_into().unwrap(),
                    action_code: 3,
                },
                operation: RolePermissionOperation::Vote,
            }];
            if index == 0 {
                for (tag, action) in [
                    (crate::MODULE_TAG, 3),
                    (
                        entitybase::business_action::MODULE_INSTITUTION_REGISTRATION,
                        0,
                    ),
                ] {
                    perms.push(RoleBusinessPermission {
                        role_subject: RoleSubject {
                            cid_number: actor_cid.clone(),
                            role_code: code.clone(),
                        },
                        business_action_id: BusinessActionId {
                            module_tag: tag.to_vec().try_into().unwrap(),
                            action_code: action,
                        },
                        operation: RolePermissionOperation::Propose,
                    });
                }
                actor_role = Some(code.clone());
            }
            let perms: crate::institution::role::RolePermissionsOf<T> = perms.try_into().unwrap();
            InstitutionRolePermissions::<T>::insert(&actor_cid, &code, perms);
            let assigned: crate::institution::role::RoleAssignmentsOf<T> =
                vec![InstitutionAdminAssignment {
                    cid_number: actor_cid.clone(),
                    role_code: code.clone(),
                    account_id: caller.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source: InstitutionAssignmentSource::Registry,
                    assignment_source_ref: vec![b'r'; 128].try_into().unwrap(),
                    assignment_status: InstitutionAssignmentStatus::Active,
                }]
                .try_into()
                .unwrap();
            InstitutionRoleAssignments::<T>::insert(&actor_cid, &code, assigned);
            UsedRoleCodes::<T>::insert(&actor_cid, &code, true);
        }
        InstitutionGovernanceThresholds::<T>::insert(&actor_cid, 2);
        InstitutionRoleNonce::<T>::insert(&actor_cid, 2u64);
        f.registry.caller = caller;
        f.registry.cid = actor_cid.to_vec();
        f.registry.role = actor_role.unwrap().to_vec();
        for (index, admin) in admins.iter_mut().enumerate() {
            seed::<T>(admin, 700_000_000 + index as u32, &f.registry);
        }
        frame_support::storage::unhashed::put(
            &frame_support::storage::storage_prefix(b"GenesisPallet", b"Phase"),
            &1u8,
        );
    }
    // 钱包登记与岗位配置分开：setup先登记准确名册，计时只执行本次明确的治理配置。
    // 不恢复已删除的完整名册替换登记路径，也不清空已有钱包版本/存活引用。
    T::AdminLifecycle::benchmark_set_institution_admins(
        crate::MODULE_TAG,
        f.cid.to_vec(),
        *b"CGOV",
        adminbase::AdminAccountKind::PublicInstitution,
        admins.clone(),
    )
    .unwrap();
    InstitutionGovernanceThresholds::<T>::remove(&f.cid);
    InstitutionRoles::<T>::remove(&f.cid, &f.role);
    InstitutionRoleAssignments::<T>::remove(&f.cid, &f.role);
    InstitutionRolePermissions::<T>::remove(&f.cid, &f.role);
    InstitutionRoleNonce::<T>::insert(&f.cid, 1u64);
    let permissions = [
        (
            entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE,
            RolePermissionOperation::Propose,
        ),
        (
            entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE,
            RolePermissionOperation::Vote,
        ),
        (
            entitybase::business_action::ACTION_INSTITUTION_CLOSE,
            RolePermissionOperation::Propose,
        ),
        (
            entitybase::business_action::ACTION_INSTITUTION_CLOSE,
            RolePermissionOperation::Vote,
        ),
    ]
    .into_iter()
    .take(p as usize)
    .map(|(action_code, operation)| entitybase::RolePermissionSpec {
        business_action_id: BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code,
        },
        operation,
    })
    .collect::<Vec<_>>();
    let targets = admins
        .iter()
        .take(a as usize)
        .map(|admin| entitybase::InstitutionAssignmentTarget {
            account_id: admin.account_id.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: InstitutionAssignmentSource::Registry,
            assignment_source_ref: Vec::new(),
            assignment_status: InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    let mut mutations = Vec::new();
    let mut existing_changes = Vec::new();
    for index in 0..r {
        let mut name = vec![b'R'; T::MaxAccountNameLength::get() as usize];
        // 差异放末尾，真实覆盖最长共同前缀的名称唯一性比较。
        let end = name.len();
        name[end - 4..].copy_from_slice(&index.to_le_bytes());
        if existing {
            let code: crate::institution::role::RoleCodeOf =
                entitybase::institution_role::generate_registration_role_code(
                    crate::MODULE_TAG,
                    &f.cid,
                    index as u64,
                    b"benchmark-existing-role",
                )
                .try_into()
                .unwrap();
            InstitutionRoles::<T>::insert(
                &f.cid,
                &code,
                InstitutionRole {
                    cid_number: f.cid.clone(),
                    role_code: code.clone(),
                    role_name: name.try_into().unwrap(),
                    term_required: false,
                    role_status: InstitutionRoleStatus::Active,
                },
            );
            let stored = permissions
                .iter()
                .map(|spec| RoleBusinessPermission {
                    role_subject: RoleSubject {
                        cid_number: f.cid.clone(),
                        role_code: code.clone(),
                    },
                    business_action_id: BusinessActionId {
                        module_tag: spec
                            .business_action_id
                            .module_tag
                            .clone()
                            .try_into()
                            .unwrap(),
                        action_code: spec.business_action_id.action_code,
                    },
                    operation: spec.operation,
                })
                .collect::<Vec<_>>();
            let stored: crate::institution::role::RolePermissionsOf<T> = stored.try_into().unwrap();
            InstitutionRolePermissions::<T>::insert(&f.cid, &code, stored);
            let stored: crate::institution::role::RoleAssignmentsOf<T> = targets
                .iter()
                .map(|target| InstitutionAdminAssignment {
                    cid_number: f.cid.clone(),
                    role_code: code.clone(),
                    account_id: target.account_id.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source: InstitutionAssignmentSource::Registry,
                    assignment_source_ref: vec![b'r'; 128].try_into().unwrap(),
                    assignment_status: InstitutionAssignmentStatus::Active,
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            InstitutionRoleAssignments::<T>::insert(&f.cid, &code, stored);
            UsedRoleCodes::<T>::insert(&f.cid, &code, true);
            existing_changes.push(entitybase::InstitutionRoleAssignmentChange {
                role_code: code.to_vec(),
                assignments: targets.clone(),
            });
        } else {
            mutations.push(entitybase::InstitutionRoleMutation::Create {
                role_name: name,
                term_required: false,
                permissions: permissions.clone(),
                assignments: targets.clone(),
            });
        }
    }
    let threshold = r.saturating_mul(a) / 2 + 1;
    let action = if existing {
        InstitutionGovernanceThresholds::<T>::insert(&f.cid, threshold);
        InstitutionRoleNonce::<T>::insert(&f.cid, u64::from(r) + 1);
        entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
            role_mutations: Vec::new(),
            assignment_changes: existing_changes,
            legal_representative_change: None,
        }
    } else {
        entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
            role_mutations: mutations,
            assignment_changes: Vec::new(),
            legal_representative_change: None,
        }
    };
    let state = Pallet::<T>::registration_state_hash(&f.cid).unwrap();
    #[cfg(feature = "std")]
    let work = {
        use adminbase::InstitutionAdminQuery as _;
        let mut work = action.registration_work();
        work.0 = work.0.saturating_add(
            T::InstitutionAdminQuery::institution_admins_len(*b"CGOV", f.cid.as_slice())
                .unwrap_or(0),
        );
        for (code, _) in InstitutionRoles::<T>::iter_prefix(&f.cid) {
            work.1 += 1;
            work.2 +=
                InstitutionRolePermissions::<T>::decode_len(&f.cid, &code).unwrap_or(0) as u32;
            work.3 +=
                InstitutionRoleAssignments::<T>::decode_len(&f.cid, &code).unwrap_or(0) as u32;
        }
        work
    };
    RegistrationFixture {
        registry: f.registry,
        cid: f.cid,
        action,
        state,
        threshold: if existing { None } else { Some(threshold) },
        #[cfg(feature = "std")]
        work,
    }
}
fn verify_registration<T: Config>(cid: &CidNumberOf<T>, n: u32, r: u32, p: u32, a: u32) {
    assert_eq!(
        InstitutionGovernanceThresholds::<T>::get(cid),
        Some(r.saturating_mul(a) / 2 + 1)
    );
    // 配置调用中的名册一致性写入仍推进原状态nonce，建岗另占r次。
    assert_eq!(InstitutionRoleNonce::<T>::get(cid), u64::from(r) + 2);
    let info = Institutions::<T>::get(cid).unwrap();
    assert_eq!(
        T::InstitutionAdminQuery::institution_admins_len(info.institution_code, cid),
        Some(n.max(a))
    );
    let mut dynamic = 0;
    for (code, _) in InstitutionRoles::<T>::iter_prefix(cid) {
        if code.as_slice() == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE {
            continue;
        }
        dynamic += 1;
        assert_eq!(
            InstitutionRolePermissions::<T>::get(cid, &code).len(),
            p as usize
        );
        assert_eq!(
            InstitutionRoleAssignments::<T>::get(cid, &code).len(),
            a as usize
        );
    }
    assert_eq!(dynamic, r);
}

/// 原生内存实测：setup、状态哈希准备和verify均在计时外；不是WASM/数据库权重跑分。
#[cfg(feature = "std")]
pub fn measure_registration<T: BenchmarkSetup>(
    n: u32,
    r: u32,
    p: u32,
    a: u32,
    existing: bool,
) -> (u128, (u32, u32, u32, u32)) {
    let f = registration_fixture::<T>(n, r, p, a, existing, false);
    let started = std::time::Instant::now();
    Pallet::<T>::register_institution_admins(
        RawOrigin::Signed(f.registry.caller).into(),
        f.cid.clone(),
        f.action,
        f.registry.cid,
        f.registry.role,
        f.state,
        f.threshold,
    )
    .unwrap();
    let elapsed = started.elapsed().as_nanos();
    verify_registration::<T>(&f.cid, n, r, p, a);
    (elapsed, f.work)
}

#[cfg(feature = "std")]
pub fn measure_operation_registration<T: BenchmarkSetup>(
    n: u32,
    r: u32,
    p: u32,
    a: u32,
) -> (u128, (u32, u32, u32, u32)) {
    let f = registration_fixture::<T>(n, r, p, a, true, true);
    let started = std::time::Instant::now();
    Pallet::<T>::register_institution_admins(
        RawOrigin::Signed(f.registry.caller).into(),
        f.cid.clone(),
        f.action,
        f.registry.cid,
        f.registry.role,
        f.state,
        f.threshold,
    )
    .unwrap();
    let elapsed = started.elapsed().as_nanos();
    verify_registration::<T>(&f.cid, n, r, p, a);
    (elapsed, f.work)
}

#[benchmarks(where T:BenchmarkSetup)]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn update_institution_info() {
        let f = prepare::<T>();
        let name: AccountNameOf<T> = vec![b'n'; T::MaxAccountNameLength::get() as usize]
            .try_into()
            .unwrap();
        #[extrinsic_call]
        _(
            RawOrigin::Signed(f.registry.caller),
            f.cid.clone(),
            name.clone(),
            name.clone(),
            f.registry.cid,
            f.registry.role,
        );
        assert_eq!(Institutions::<T>::get(f.cid).unwrap().cid_full_name, name);
    }
    #[benchmark]
    fn register_institution_admins(
        n: Linear<2, 1989>,
        r: Linear<1, 64>,
        p: Linear<2, 4>,
        a: Linear<2, 1989>,
    ) {
        let f = registration_fixture::<T>(n, r, p, a, false, false);
        #[extrinsic_call]
        _(
            RawOrigin::Signed(f.registry.caller),
            f.cid.clone(),
            f.action,
            f.registry.cid,
            f.registry.role,
            f.state,
            f.threshold,
        );
        verify_registration::<T>(&f.cid, n, r, p, a);
    }
    /// 额外基准覆盖已初始化机构的快照读取；仍执行同一个已有call 9。
    #[benchmark(extra)]
    fn registration_existing(
        n: Linear<2, 1989>,
        r: Linear<1, 64>,
        p: Linear<2, 4>,
        a: Linear<2, 1989>,
    ) {
        let f = registration_fixture::<T>(n, r, p, a, true, false);
        #[block]
        {
            Pallet::<T>::register_institution_admins(
                RawOrigin::Signed(f.registry.caller).into(),
                f.cid.clone(),
                f.action,
                f.registry.cid,
                f.registry.role,
                f.state,
                f.threshold,
            )
            .unwrap();
        }
        verify_registration::<T>(&f.cid, n, r, p, a);
    }
    /// 运行期四要素和每人4个身份存储读取，不能用创世空CID样本冒充该路径权重。
    #[benchmark(extra)]
    fn registration_operation(
        n: Linear<2, 1989>,
        r: Linear<1, 64>,
        p: Linear<2, 4>,
        a: Linear<2, 1989>,
    ) {
        let f = registration_fixture::<T>(n, r, p, a, true, true);
        #[block]
        {
            Pallet::<T>::register_institution_admins(
                RawOrigin::Signed(f.registry.caller).into(),
                f.cid.clone(),
                f.action,
                f.registry.cid,
                f.registry.role,
                f.state,
                f.threshold,
            )
            .unwrap();
        }
        verify_registration::<T>(&f.cid, n, r, p, a);
    }
    #[benchmark]
    fn add_institution_account() {
        let f = prepare::<T>();
        let names: InstitutionAccountNamesOf<T> = (0..T::MaxInstitutionAccounts::get())
            .map(|i| {
                let mut name = vec![b'a'; T::MaxAccountNameLength::get() as usize];
                name[0] = b'A' + (i / 26) as u8;
                name[1] = b'A' + (i % 26) as u8;
                AccountNameOf::<T>::try_from(name).unwrap()
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        #[extrinsic_call]
        propose_add_institution_account(RawOrigin::Signed(f.caller), f.cid, names, f.role);
    }
    #[benchmark]
    fn propose_institution_governance() {
        let f = prepare::<T>();
        let action = entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
            role_mutations: vec![entitybase::InstitutionRoleMutation::Rename {
                role_code: f.role.to_vec(),
                role_name: b"Benchmark renamed".to_vec(),
            }],
            assignment_changes: vec![],
            legal_representative_change: None,
        };
        #[extrinsic_call]
        _(
            RawOrigin::Signed(f.caller),
            f.cid.clone(),
            action,
            f.cid.to_vec(),
            f.role,
        );
    }
    #[benchmark]
    fn governance_names() {
        let f = prepare::<T>();
        let action = entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
            role_mutations: vec![entitybase::InstitutionRoleMutation::Rename {
                role_code: f.role.to_vec(),
                role_name: vec![b'R'; T::MaxAccountNameLength::get() as usize],
            }],
            assignment_changes: vec![],
            legal_representative_change: None,
        };
        #[extrinsic_call]
        propose_institution_governance(
            RawOrigin::Signed(f.caller),
            f.cid.clone(),
            action,
            f.cid.to_vec(),
            f.role,
        );
    }
    #[benchmark]
    fn propose_close_public_institution() {
        let f = prepare::<T>();
        let name: AccountNameOf<T> = b"benchmark-close".to_vec().try_into().unwrap();
        let (funding, _) =
            Pallet::<T>::derive_institution_account(f.cid.as_slice(), name.as_slice()).unwrap();
        let beneficiary: T::AccountId = account("beneficiary", 0, 0);
        InstitutionAccounts::<T>::insert(
            &f.cid,
            &name,
            InstitutionAccountInfo {
                account_id: funding.clone(),
                initial_balance: T::Currency::minimum_balance(),
                created_at: 1u32.into(),
            },
        );
        AccountRegisteredCid::<T>::insert(
            &funding,
            RegisteredInstitution {
                cid_number: f.cid.clone(),
                account_name: name,
            },
        );
        drop(T::Currency::make_free_balance_be(
            &funding,
            1000000u128.saturated_into(),
        ));
        #[extrinsic_call]
        _(
            RawOrigin::Signed(f.caller),
            f.cid,
            f.role,
            funding.clone(),
            beneficiary,
        );
        assert!(InstitutionPendingClose::<T>::contains_key(funding));
    }
    #[benchmark]
    fn terminal_callback_ignored() {
        use frame_support::traits::Get;
        let mut data = sp_runtime::sp_std::vec![0u8;<T as engingcore::Config>::MaxProposalDataLen::get() as usize];
        data[..5].copy_from_slice(b"other");
        engingcore::Pallet::<T>::benchmark_proposal(0, b"other", data);
        #[block]
        {
            <crate::InternalVoteExecutor<T> as engingcore::InternalVoteResultCallback>::on_execution_failed_terminal(0).expect("其它业务数据必须被忽略");
        }
    }

    #[benchmark]
    fn terminal_callback() {
        let f = prepare::<T>();
        let names: InstitutionAccountNamesOf<T> = (0..T::MaxInstitutionAccounts::get())
            .map(|i| {
                let mut v = vec![b'a'; T::MaxAccountNameLength::get() as usize];
                v[0] = b'A' + i as u8;
                AccountNameOf::<T>::try_from(v).unwrap()
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let id = engingcore::Pallet::<T>::next_proposal_id();
        Pallet::<T>::propose_add_institution_account(
            RawOrigin::Signed(f.caller).into(),
            f.cid.clone(),
            names,
            f.role,
        )
        .unwrap();
        #[block]
        {
            <crate::InternalVoteExecutor<T> as engingcore::InternalVoteResultCallback>::on_execution_failed_terminal(id).unwrap();
        }
        assert!(!InstitutionPendingAdd::<T>::contains_key(f.cid));
    }
    #[cfg(test)]
    mod registration_fixture_tests {
        use super::*;
        #[test]
        fn registration_benchmark_fixture_executes_verified_dispatch() {
            use frame_benchmarking::{BenchmarkParameter, BenchmarkingSetup};
            for (n, r, p, a) in [(2, 1, 2, 2), (10, 8, 4, 10)] {
                for existing in [false, true] {
                    crate::tests::new_test_ext().execute_with(|| {
                        let components = [(BenchmarkParameter::n, n), (BenchmarkParameter::r, r), (BenchmarkParameter::p, p), (BenchmarkParameter::a, a)];
                        if existing {
                            <registration_existing as BenchmarkingSetup<crate::tests::Test>>::unit_test_instance(&registration_existing, &components).unwrap();
                        } else {
                            <register_institution_admins as BenchmarkingSetup<crate::tests::Test>>::unit_test_instance(&register_institution_admins, &components).unwrap();
                        }
                    });
                }
            }
        }
    }
}
