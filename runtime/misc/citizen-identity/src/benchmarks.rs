//! `citizen-identity` FRAME benchmark。
//!
//! 身份写入使用 fresh spec-genesis 中真实 FRG 省专员岗位完成授权；人口维护四项按
//! 可独立组合的最重完整路径生成安全上界，避免日期推进或集中到期被低估。

#![cfg(feature = "runtime-benchmarks")]

use alloc::format;

use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::weights::Weight;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_runtime::traits::Zero;

use crate::{
    pallet::{
        AccountIdByCid, BindingRevisionByCid, CidByAccountId, CidRegistry, Config,
        PopulationMaintenanceFault, PopulationReadyDate, VotingEligibilityVersionCount,
        VotingIdentityByCid,
    },
    AreaCodeBound, BenchmarkHelper, Call, CandidateIdentityPayload, CidNumberBound,
    CidOccupyAuthorization, CidRebindAuthorization, CidRecord, CidRecordStatus,
    CitizenIdentityAuthority, CitizenIdentityAuthorization, CitizenSex, CitizenStatus, FamilyName,
    GivenName, Pallet, RoleCodeBound, VotingIdentityPayload, MAX_CID_AUTHORIZATION_LIFETIME_SECS,
};

const BENCHMARK_TIMESTAMP_MILLIS: u64 = 1_800_000_000_000;
const ONE_DAY_MILLIS: u64 = 86_400_000;

type Authority<AccountId> = (
    AccountId,
    CidNumberBound,
    RoleCodeBound,
    AreaCodeBound,
    AreaCodeBound,
);

type BenchmarkSigner<AccountId> = (sp_core::sr25519::Public, AccountId);

fn authority<T: Config>() -> Authority<T::AccountId> {
    T::CitizenIdentityAuthority::benchmark_authority()
        .expect("runtime benchmark must provide a real registrar role subject")
}

fn set_time<T: Config>(timestamp_millis: u64) -> u32 {
    T::CitizenIdentityAuthority::benchmark_set_timestamp(timestamp_millis);
    let date = Pallet::<T>::current_date_int();
    assert_ne!(
        date, 0,
        "benchmark timestamp must resolve to a calendar date"
    );
    date
}

fn genesis_hash<T: Config>() -> T::Hash {
    frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero())
}

fn citizen_cid(tag: u32) -> CidNumberBound {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: &format!("benchmark-{tag}"),
            p1: "1",
            province_code: "ZS",
            province_name: "中枢省",
            city_code: "001",
            city_name: "基准市",
            year: "2027",
            institution: "CTZN",
        },
    )
    .expect("benchmark citizen CID must satisfy the production CID protocol")
    .into_bytes()
    .try_into()
    .expect("benchmark citizen CID must fit the runtime bound")
}

fn signer<T: Config>() -> BenchmarkSigner<T::AccountId> {
    T::BenchmarkHelper::signer()
}

fn signature<T: Config>(
    signer: &sp_core::sr25519::Public,
    op_tag: u8,
    payload: &impl Encode,
) -> crate::pallet::SignatureOf<T> {
    let message = primitives::sign::signing_message(op_tag, &payload.encode());
    T::BenchmarkHelper::sign(signer, &message)
}

/// 身份基准与生产接口共用完整授权载荷，按链上当前身份版本和有效时间签名。
fn identity_signature<T: Config>(
    signer: &sp_core::sr25519::Public,
    cid_number: &CidNumberBound,
    payload: &(impl Encode + Clone),
) -> (u64, u64, crate::pallet::SignatureOf<T>) {
    let expected_identity_version = VotingEligibilityVersionCount::<T>::get(cid_number);
    let expires_at = <T::TimeProvider as frame_support::traits::UnixTime>::now()
        .as_secs()
        .saturating_add(MAX_CID_AUTHORIZATION_LIFETIME_SECS);
    let authorization = CitizenIdentityAuthorization {
        genesis_hash: genesis_hash::<T>(),
        payload: payload.clone(),
        expected_identity_version,
        expires_at,
    };
    let signature = signature::<T>(
        signer,
        primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
        &authorization,
    );
    (expected_identity_version, expires_at, signature)
}

fn voting_payload<T: Config>(
    account_id: T::AccountId,
    cid_number: CidNumberBound,
    province: AreaCodeBound,
    city: AreaCodeBound,
    town: &[u8],
    valid_from: u32,
    valid_until: u32,
) -> VotingIdentityPayload<T::AccountId> {
    VotingIdentityPayload {
        cid_number,
        account_id,
        passport_valid_from: valid_from,
        passport_valid_until: valid_until,
        citizen_status: CitizenStatus::Normal,
        residence_province_code: province,
        residence_city_code: city,
        residence_town_code: town.to_vec().try_into().expect("benchmark town code fits"),
    }
}

fn candidate_payload<T: Config>(
    voting: VotingIdentityPayload<T::AccountId>,
) -> CandidateIdentityPayload<T::AccountId> {
    CandidateIdentityPayload {
        birth_province_code: voting.residence_province_code.clone(),
        birth_city_code: voting.residence_city_code.clone(),
        birth_town_code: voting.residence_town_code.clone(),
        family_name: FamilyName::try_from("基准".as_bytes().to_vec())
            .expect("benchmark family name fits"),
        given_name: GivenName::try_from("公民".as_bytes().to_vec())
            .expect("benchmark given name fits"),
        citizen_sex: CitizenSex::Female,
        birth_date: 19900101,
        voting,
    }
}

fn seed_occupied<T: Config>(
    actor_cid_number: &CidNumberBound,
    cid_number: &CidNumberBound,
    account_id: &T::AccountId,
) {
    CidRegistry::<T>::insert(
        cid_number,
        CidRecord {
            registrar_cid_number: actor_cid_number.clone(),
            commitment: sp_io::hashing::blake2_256(&account_id.encode()),
            residence_province_code: AreaCodeBound::default(),
            residence_city_code: AreaCodeBound::default(),
            status: CidRecordStatus::Active,
            registered_at: frame_system::Pallet::<T>::block_number(),
            revoked_at: None,
        },
    );
    AccountIdByCid::<T>::insert(cid_number, account_id);
    CidByAccountId::<T>::insert(account_id, cid_number);
    BindingRevisionByCid::<T>::insert(cid_number, 1);
}

fn setup_registration<T: Config>(
    tag: u32,
    valid_from: u32,
    valid_until: u32,
) -> (
    Authority<T::AccountId>,
    VotingIdentityPayload<T::AccountId>,
    sp_core::sr25519::Public,
) {
    let authority = authority::<T>();
    let cid_number = citizen_cid(tag);
    let (signer, account_id) = signer::<T>();
    seed_occupied::<T>(&authority.1, &cid_number, &account_id);
    let payload = voting_payload::<T>(
        account_id,
        cid_number,
        authority.3.clone(),
        authority.4.clone(),
        b"ZS01001",
        valid_from,
        valid_until,
    );
    (authority, payload, signer)
}

fn register<T: Config>(
    authority: &Authority<T::AccountId>,
    payload: VotingIdentityPayload<T::AccountId>,
    signer: &sp_core::sr25519::Public,
) {
    let (expected_identity_version, expires_at, signature) =
        identity_signature::<T>(signer, &payload.cid_number, &payload);
    Pallet::<T>::register_voting_identity(
        RawOrigin::Signed(authority.0.clone()).into(),
        authority.1.clone(),
        authority.2.clone(),
        payload,
        expected_identity_version,
        expires_at,
        signature,
    )
    .expect("benchmark voting identity registration must succeed");
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_voting_identity() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, payload, signer) = setup_registration::<T>(1, today, 20991231);
        let cid_number = payload.cid_number.clone();
        let (expected_identity_version, expires_at, signature) =
            identity_signature::<T>(&signer, &payload.cid_number, &payload);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            payload,
            expected_identity_version,
            expires_at,
            signature,
        );

        assert!(VotingIdentityByCid::<T>::contains_key(cid_number));
    }

    #[benchmark]
    fn upgrade_to_candidate_identity() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, voting, signer) = setup_registration::<T>(2, today, 20991231);
        register::<T>(&authority, voting.clone(), &signer);
        let payload = candidate_payload::<T>(voting);
        let cid_number = payload.voting.cid_number.clone();
        let (expected_identity_version, expires_at, signature) =
            identity_signature::<T>(&signer, &payload.voting.cid_number, &payload);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            payload,
            expected_identity_version,
            expires_at,
            signature,
        );

        assert!(crate::pallet::CandidateIdentityByCid::<T>::contains_key(
            cid_number
        ));
    }

    #[benchmark]
    fn update_voting_identity() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, initial, signer) = setup_registration::<T>(3, today, 20991231);
        register::<T>(&authority, initial.clone(), &signer);
        let mut payload = initial;
        payload.residence_town_code = b"ZS01002".to_vec().try_into().expect("town code fits");
        let cid_number = payload.cid_number.clone();
        let (expected_identity_version, expires_at, signature) =
            identity_signature::<T>(&signer, &payload.cid_number, &payload);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            payload,
            expected_identity_version,
            expires_at,
            signature,
        );

        assert!(VotingIdentityByCid::<T>::contains_key(cid_number));
    }

    #[benchmark]
    fn update_candidate_identity() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, voting, signer) = setup_registration::<T>(4, today, 20991231);
        register::<T>(&authority, voting.clone(), &signer);
        let initial = candidate_payload::<T>(voting);
        let (expected_identity_version, expires_at, initial_signature) =
            identity_signature::<T>(&signer, &initial.voting.cid_number, &initial);
        Pallet::<T>::upgrade_to_candidate_identity(
            RawOrigin::Signed(authority.0.clone()).into(),
            authority.1.clone(),
            authority.2.clone(),
            initial.clone(),
            expected_identity_version,
            expires_at,
            initial_signature,
        )
        .expect("benchmark candidate upgrade must succeed");
        let mut payload = initial;
        payload.voting.residence_town_code =
            b"ZS01002".to_vec().try_into().expect("town code fits");
        let cid_number = payload.voting.cid_number.clone();
        let (expected_identity_version, expires_at, signature) =
            identity_signature::<T>(&signer, &payload.voting.cid_number, &payload);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            payload,
            expected_identity_version,
            expires_at,
            signature,
        );

        assert!(crate::pallet::CandidateIdentityByCid::<T>::contains_key(
            cid_number
        ));
    }

    #[benchmark]
    fn revoke_identity() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, voting, signer) = setup_registration::<T>(5, today, 20991231);
        let cid_number = voting.cid_number.clone();
        register::<T>(&authority, voting.clone(), &signer);
        let candidate = candidate_payload::<T>(voting);
        let (expected_identity_version, expires_at, signature) =
            identity_signature::<T>(&signer, &candidate.voting.cid_number, &candidate);
        Pallet::<T>::upgrade_to_candidate_identity(
            RawOrigin::Signed(authority.0.clone()).into(),
            authority.1.clone(),
            authority.2.clone(),
            candidate,
            expected_identity_version,
            expires_at,
            signature,
        )
        .expect("benchmark candidate upgrade must succeed");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            cid_number.clone(),
        );

        assert_eq!(
            CidRegistry::<T>::get(cid_number).map(|record| record.status),
            Some(CidRecordStatus::Revoked)
        );
    }

    #[benchmark]
    fn occupy_cid() {
        set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        let authority = authority::<T>();
        let (signer, account_id) = signer::<T>();
        let cid_number = citizen_cid(6);
        let expires_at = <T::TimeProvider as frame_support::traits::UnixTime>::now()
            .as_secs()
            .saturating_add(MAX_CID_AUTHORIZATION_LIFETIME_SECS);
        let authorization = CidOccupyAuthorization {
            genesis_hash: genesis_hash::<T>(),
            cid_number: cid_number.clone(),
            account_id: account_id.clone(),
            expected_binding_revision: 0,
            expires_at,
        };
        let signature = signature::<T>(
            &signer,
            primitives::sign::OP_SIGN_CID_OCCUPY,
            &authorization,
        );

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            cid_number.clone(),
            account_id.clone(),
            expires_at,
            signature,
        );

        assert!(CidRegistry::<T>::contains_key(&cid_number));
        assert_eq!(AccountIdByCid::<T>::get(&cid_number), Some(account_id));
        assert_eq!(BindingRevisionByCid::<T>::get(&cid_number), Some(1));
    }

    #[benchmark]
    fn self_occupy_cid() {
        let account_id: T::AccountId = whitelisted_caller();
        let cid_number = citizen_cid(7);

        #[extrinsic_call]
        _(RawOrigin::Signed(account_id.clone()), cid_number.clone());

        assert!(CidRegistry::<T>::contains_key(&cid_number));
        assert_eq!(AccountIdByCid::<T>::get(&cid_number), Some(account_id));
    }

    #[benchmark]
    fn self_rebind_cid_account_id() {
        set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        // 先自助占号建立当前绑定，再换绑到新账户。
        let (current_signer, current_account_id) = signer::<T>();
        let new_account_id: T::AccountId = whitelisted_caller();
        let cid_number = citizen_cid(8);
        Pallet::<T>::self_occupy_cid(
            RawOrigin::Signed(current_account_id.clone()).into(),
            cid_number.clone(),
        )
        .expect("self occupy sets up the binding");
        let expires_at = <T::TimeProvider as frame_support::traits::UnixTime>::now()
            .as_secs()
            .saturating_add(MAX_CID_AUTHORIZATION_LIFETIME_SECS);
        let authorization = CidRebindAuthorization {
            genesis_hash: genesis_hash::<T>(),
            cid_number: cid_number.clone(),
            current_account_id,
            new_account_id: new_account_id.clone(),
            expected_binding_revision: 1,
            expires_at,
        };
        let signature = signature::<T>(
            &current_signer,
            primitives::sign::OP_SIGN_CID_REBIND,
            &authorization,
        );

        #[extrinsic_call]
        _(
            RawOrigin::Signed(new_account_id.clone()),
            cid_number.clone(),
            1,
            expires_at,
            signature,
        );

        assert_eq!(AccountIdByCid::<T>::get(&cid_number), Some(new_account_id));
        assert_eq!(BindingRevisionByCid::<T>::get(&cid_number), Some(2));
    }

    #[benchmark]
    fn admin_rebind_cid_account_id() {
        set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        // 先自助占号建立当前绑定，再由注册局代换绑到新账户。
        let authority = authority::<T>();
        let current_account_id: T::AccountId = account("admin_rebind_current", 0, 0);
        let (new_signer, new_account_id) = signer::<T>();
        let cid_number = citizen_cid(9);
        Pallet::<T>::self_occupy_cid(
            RawOrigin::Signed(current_account_id.clone()).into(),
            cid_number.clone(),
        )
        .expect("self occupy sets up the binding");
        let expires_at = <T::TimeProvider as frame_support::traits::UnixTime>::now()
            .as_secs()
            .saturating_add(MAX_CID_AUTHORIZATION_LIFETIME_SECS);
        let authorization = CidRebindAuthorization {
            genesis_hash: genesis_hash::<T>(),
            cid_number: cid_number.clone(),
            current_account_id,
            new_account_id: new_account_id.clone(),
            expected_binding_revision: 1,
            expires_at,
        };
        let signature = signature::<T>(
            &new_signer,
            primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
            &authorization,
        );

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            cid_number.clone(),
            new_account_id.clone(),
            1,
            expires_at,
            signature,
        );

        assert_eq!(AccountIdByCid::<T>::get(&cid_number), Some(new_account_id));
        assert_eq!(BindingRevisionByCid::<T>::get(&cid_number), Some(2));
    }

    #[benchmark]
    fn revoke_cid() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::put(today);
        let (authority, voting, signer) = setup_registration::<T>(7, today, 20991231);
        let cid_number = voting.cid_number.clone();
        register::<T>(&authority, voting, &signer);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(authority.0),
            authority.1,
            authority.2,
            cid_number.clone(),
        );

        assert_eq!(
            CidRegistry::<T>::get(cid_number).map(|record| record.status),
            Some(CidRecordStatus::Revoked)
        );
    }

    #[benchmark]
    fn population_maintenance_base() {
        set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationMaintenanceFault::<T>::put(crate::PopulationFault::InvalidReadyDate);

        #[block]
        {
            Pallet::<T>::process_population_maintenance(Weight::MAX);
        }

        assert!(PopulationMaintenanceFault::<T>::get().is_some());
    }

    #[benchmark]
    fn initialize_population_date() {
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);
        PopulationReadyDate::<T>::kill();

        #[block]
        {
            Pallet::<T>::process_population_maintenance(Weight::MAX);
        }

        assert_eq!(PopulationReadyDate::<T>::get(), today);
    }

    #[benchmark]
    fn advance_population_day() {
        let yesterday = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS - ONE_DAY_MILLIS);
        PopulationReadyDate::<T>::put(yesterday);
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);

        #[block]
        {
            Pallet::<T>::process_population_maintenance(Weight::MAX);
        }

        assert_eq!(PopulationReadyDate::<T>::get(), today);
    }

    #[benchmark]
    fn process_population_transition() {
        let yesterday = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS - ONE_DAY_MILLIS);
        PopulationReadyDate::<T>::put(yesterday);
        let (authority, voting, signer) = setup_registration::<T>(8, 20000101, yesterday);
        let cid_number = voting.cid_number.clone();
        register::<T>(&authority, voting, &signer);
        let today = set_time::<T>(BENCHMARK_TIMESTAMP_MILLIS);

        #[block]
        {
            Pallet::<T>::process_population_maintenance(Weight::MAX);
        }

        assert_eq!(PopulationReadyDate::<T>::get(), today);
        assert!(AccountIdByCid::<T>::contains_key(cid_number));
    }
}
