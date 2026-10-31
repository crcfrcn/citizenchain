//! 完整机构调用基准：setup只准备所属临时状态，计时内执行真实注册局/岗位授权。
#![cfg(feature = "runtime-benchmarks")]
use crate::{pallet::*, Call, Config, InstitutionAccountInfo, Pallet, RegisteredInstitution};
use adminbase::{Admin, InstitutionAdminLifecycle};
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
            year: "2026",
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
    <T as Config>::AdminLifecycle::set_institution_admins(
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
    fn register_institution_admins(n: Linear<1, 1989>) {
        let f = prepare::<T>();
        assert!(n <= T::MaxAdmins::get());
        let admins: InstitutionAdminsInputOf<T> = f
            .admins
            .into_iter()
            .take(n as usize)
            .map(|mut admin| {
                admin.family_name = vec![b'F'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                    .try_into()
                    .unwrap();
                admin.given_name = vec![b'G'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                    .try_into()
                    .unwrap();
                admin
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        #[extrinsic_call]
        _(
            RawOrigin::Signed(f.registry.caller),
            f.cid,
            admins,
            f.registry.cid,
            f.registry.role,
        );
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
        let action = entitybase::InstitutionGovernanceAction::ReplaceAdmins {
            admins: f.admins.to_vec(),
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
        let mut admins = f.admins.to_vec();
        for a in &mut admins {
            a.family_name = vec![b'F'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                .try_into()
                .unwrap();
            a.given_name = vec![b'G'; adminbase::ADMIN_PERSON_NAME_MAX_BYTES as usize]
                .try_into()
                .unwrap();
        }
        while admins.encoded_size() + 2048
            > <T as votebase::Config>::MaxProposalDataLen::get() as usize
        {
            admins.pop().expect("基准载荷必须容纳至少一名管理员");
        }
        let action = entitybase::InstitutionGovernanceAction::ReplaceAdmins { admins };
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
        let mut data = sp_runtime::sp_std::vec![0u8;<T as votebase::Config>::MaxProposalDataLen::get() as usize];
        data[..5].copy_from_slice(b"other");
        votebase::Pallet::<T>::benchmark_proposal(0, b"other", data);
        #[block]
        {
            <crate::InternalVoteExecutor<T> as votebase::InternalVoteResultCallback>::on_execution_failed_terminal(0).expect("其它业务数据必须被忽略");
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
        let id = votebase::Pallet::<T>::next_proposal_id();
        Pallet::<T>::propose_add_institution_account(
            RawOrigin::Signed(f.caller).into(),
            f.cid.clone(),
            names,
            f.role,
        )
        .unwrap();
        #[block]
        {
            <crate::InternalVoteExecutor<T> as votebase::InternalVoteResultCallback>::on_execution_failed_terminal(id).unwrap();
        }
        assert!(!InstitutionPendingAdd::<T>::contains_key(f.cid));
    }
}
