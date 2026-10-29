//! 复用本模块真实测试环境的行为与边界回归。
use super::*;

/// NodeGuard 镜像 `CidRecord` 的完整字段序和状态判别值；任一变化都属于 storage 契约变化。
#[test]
fn cid_record_scale_contract_matches_node_guard() {
    use codec::Encode;

    assert_eq!(CidRecordStatus::Active.encode(), vec![0]);
    assert_eq!(CidRecordStatus::Revoked.encode(), vec![1]);

    let record = CidRecord {
        registrar_cid_number: registrar_cid_number(),
        commitment: [2u8; 32],
        residence_province_code: b"GD".to_vec().try_into().expect("province"),
        residence_city_code: b"001".to_vec().try_into().expect("city"),
        status: CidRecordStatus::Revoked,
        registered_at: 8u32,
        revoked_at: Some(9u32),
    };
    assert_eq!(
        record.encode(),
        (
            registrar_cid_number().to_vec(),
            [2u8; 32],
            b"GD".to_vec(),
            b"001".to_vec(),
            CidRecordStatus::Revoked,
            8u32,
            Some(9u32),
        )
            .encode()
    );
}

#[test]
fn genesis_cid_binding_writes_active_registry_and_bidirectional_indexes() {
    use codec::Encode;

    let cid_number = cid(&citizen_cid_number("genesis-binding"));
    let registrar = registrar_cid_number();
    new_test_ext_with_cid_bindings(vec![(cid_number.clone(), 42, registrar.clone())]).execute_with(
        || {
            let record = CidRegistry::<Test>::get(&cid_number).expect("创世 CID 必须存在登记记录");
            assert_eq!(record.registrar_cid_number, registrar);
            assert_eq!(
                record.commitment,
                sp_io::hashing::blake2_256(&42u64.encode())
            );
            assert!(record.residence_province_code.is_empty());
            assert!(record.residence_city_code.is_empty());
            assert_eq!(record.status, CidRecordStatus::Active);
            assert_eq!(record.registered_at, 0);
            assert_eq!(record.revoked_at, None);
            assert_eq!(AccountIdByCid::<Test>::get(&cid_number), Some(42));
            assert_eq!(CidByAccountId::<Test>::get(42), Some(cid_number.clone()));
            assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(1));
            assert!(!VotingIdentityByCid::<Test>::contains_key(&cid_number));
            assert!(!CandidateIdentityByCid::<Test>::contains_key(&cid_number));
        },
    );
}

#[test]
#[should_panic(expected = "创世 CID 不能重复")]
fn genesis_cid_binding_rejects_duplicate_cid() {
    let cid_number = cid(&citizen_cid_number("genesis-duplicate-cid"));
    let registrar = registrar_cid_number();
    let _ = new_test_ext_with_cid_bindings(vec![
        (cid_number.clone(), 42, registrar.clone()),
        (cid_number, 43, registrar),
    ]);
}

#[test]
#[should_panic(expected = "创世 CID 必须是合法的 CTZN 或 NATP 人主体号码")]
fn genesis_cid_binding_rejects_invalid_cid_number() {
    let _ = new_test_ext_with_cid_bindings(vec![(cid(b"not-a-cid"), 42, registrar_cid_number())]);
}

#[test]
#[should_panic(expected = "创世 AccountId 不能绑定多个 CID")]
fn genesis_cid_binding_rejects_duplicate_account_id() {
    let registrar = registrar_cid_number();
    let _ = new_test_ext_with_cid_bindings(vec![
        (
            cid(&citizen_cid_number("genesis-account-a")),
            42,
            registrar.clone(),
        ),
        (cid(&citizen_cid_number("genesis-account-b")), 42, registrar),
    ]);
}

#[test]
fn self_occupy_cid_binds_account_and_stays_anonymous() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self1");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // 占即绑:双向绑定成立。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid(&cid_bytes)));
        // 登记记录:SELF registrar + 空居住地 + Active + commitment = blake2_256(account_id)。
        let rec = CidRegistry::<Test>::get(cid(&cid_bytes)).expect("record");
        assert_eq!(
            rec.registrar_cid_number.to_vec(),
            SELF_OCCUPY_REGISTRAR.to_vec()
        );
        assert!(rec.residence_province_code.is_empty());
        assert!(rec.residence_city_code.is_empty());
        assert_eq!(rec.status, CidRecordStatus::Active);
        assert_eq!(rec.commitment, sp_io::hashing::blake2_256(&1u64.encode()));
        // 匿名:无投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&cid_bytes)));
    });
}

#[test]
fn self_occupy_cid_is_idempotent_for_same_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self2");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // 同账户同 CID 重放幂等(commitment 同值,SELF registrar 同值),不报错。
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid(&cid_bytes)));
    });
}

#[test]
fn self_occupy_cid_rejects_second_cid_for_same_account_id() {
    new_test_ext().execute_with(|| {
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&citizen_cid_number("self3a")),
        ));
        // 一账户一 CID:同账户再占另一个 CID 被拒。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(
                RuntimeOrigin::signed(1),
                cid(&citizen_cid_number("self3b")),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn self_occupy_cid_rejects_cid_taken_by_another_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self4");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // CID 已被账户 1 占绑,账户 2 占同一 CID 被拒(双向闭环)。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(RuntimeOrigin::signed(2), cid(&cid_bytes)),
            Error::<Test>::CidAccountIdBindingMismatch
        );
    });
}

#[test]
fn self_occupy_cid_rejects_non_citizen_code() {
    new_test_ext().execute_with(|| {
        // 自助占号只认 CTZN;机构码(CGOV)CID 被 ensure_valid_citizen_cid 拒。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(
                RuntimeOrigin::signed(1),
                cid(&public_cid_number("self5")),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn self_occupy_cid_accepts_resident_natp_type() {
    new_test_ext().execute_with(|| {
        // q3:用户可自选 NATP(居民)作为匿名身份类型(与 CTZN 公民并列)。
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "natp-self",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(3),
            cid(&natp),
        ));
        assert_eq!(CidByAccountId::<Test>::get(3), Some(cid(&natp)));
        // 匿名:居民也不写投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&natp)));
    });
}

#[test]
fn self_occupy_cid_rejects_smtp_type() {
    new_test_ext().execute_with(|| {
        // 自助只限 CTZN/NATP:智能人 SMTP 不可自助占号(须其它流程)。
        let smtp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "smtp-self",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "SMTP",
            },
        )
        .expect("smtp cid should generate")
        .into_bytes();
        assert_noop!(
            CitizenIdentity::self_occupy_cid(RuntimeOrigin::signed(3), cid(&smtp)),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn duplicate_cid_cannot_move_to_another_account_id() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(2, &citizen_cid_number("0001")),
                identity_version(&citizen_cid_number("0001")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::VotingIdentityAlreadyExists
        );
    });
}

#[test]
fn updating_identity_cannot_replace_permanent_cid() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置(占即绑,两号绑不同账户)。
        occupy_tag_as("0001", 1);
        occupy_tag_as("0002", 2);

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("0002")),
                identity_version(&citizen_cid_number("0002")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAccountIdBindingMismatch
        );
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0001"))),
            Some(1)
        );
        // 占即绑:0002 虽未登记投票身份,占号时已绑账户 2(匿名 CID)。
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0002"))),
            Some(2)
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(
            TownVotingCount::<Test>::get((code(b"43"), code(b"4301"), code(b"4301001"))),
            1
        );
    });
}

#[test]
fn citizen_subject_requires_active_bidirectional_cid_account_id_binding() {
    new_test_ext().execute_with(|| {
        occupy_tag("SUBJECT");
        let cid_number = citizen_cid_number("SUBJECT");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_number),
            identity_version(&cid_number),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(
            CitizenIdentity::citizen_subject(&1),
            Some(CitizenSubject {
                cid_number: cid(&cid_number),
                account_id: 1,
            })
        );

        // 反向绑定与账户存储键不一致时 fail-closed，不能只凭裸账户形成主体。
        AccountIdByCid::<Test>::insert(cid(&cid_number), 2);
        assert_eq!(CitizenIdentity::citizen_subject(&1), None);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn citizen_subject_rejects_revoked_identity_and_cid() {
    new_test_ext().execute_with(|| {
        occupy_tag("SUBJECT-REVOKED");
        let cid_number = citizen_cid_number("SUBJECT-REVOKED");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_number),
            identity_version(&cid_number),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_number),
        ));

        assert_eq!(CitizenIdentity::citizen_subject(&1), None);
    });
}

#[test]
fn revoke_identity_revision_preflight_failures_leave_state_unchanged() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("REVOKE-REV");
        occupy_tag("REVOKE-REV");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let cid_number = cid(&cid_bytes);
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let population_before = CountryVotingCount::<Test>::get();

        BindingRevisionByCid::<Test>::remove(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionMissing
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);

        BindingRevisionByCid::<Test>::insert(&cid_number, u64::MAX);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionOverflow
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            Some(u64::MAX)
        );
    });
}

#[test]
fn invalid_citizen_code_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, b"OLD-0001"),
                identity_version(b"OLD-0001"),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn strict_calendar_validation_handles_months_leap_years_and_year_boundary() {
    new_test_ext().execute_with(|| {
        assert!(CitizenIdentity::is_plausible_yyyymmdd(20240229));
        assert!(!CitizenIdentity::is_plausible_yyyymmdd(20230229));
        assert!(!CitizenIdentity::is_plausible_yyyymmdd(20260431));
        assert_eq!(
            CitizenIdentity::next_calendar_date(20240229),
            Some(20240301)
        );
        assert_eq!(
            CitizenIdentity::next_calendar_date(20261231),
            Some(20270101)
        );
        assert_eq!(CitizenIdentity::next_calendar_date(99991231), None);
    });
}

#[test]
fn current_date_int_matches_fixed_time() {
    new_test_ext().execute_with(|| {
        // FixedTime = 2026-07-02 00:00 UTC → UTC+8 折算 20260702。
        assert_eq!(CitizenIdentity::current_date_int(), 20260702);
    });
}

#[test]
fn age_from_birth_date_handles_birthday_boundary() {
    new_test_ext().execute_with(|| {
        // 固定链上日 20260702。
        assert_eq!(CitizenIdentity::age_from_birth_date(20000701), Some(26)); // 生日已过
        assert_eq!(CitizenIdentity::age_from_birth_date(20000702), Some(26)); // 今日生日
        assert_eq!(CitizenIdentity::age_from_birth_date(20000703), Some(25)); // 生日未到
        assert_eq!(CitizenIdentity::age_from_birth_date(0), None); // 空
        assert_eq!(CitizenIdentity::age_from_birth_date(20300101), None); // 未来出生
    });
}

#[test]
fn non_citizen_family_code_is_rejected() {
    new_test_ext().execute_with(|| {
        // 真实格式的公权机构号(CGOV)打到公民入口必须被家族断言拒绝。
        let institution_number = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "gov",
                p1: "0",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "CGOV",
            },
        )
        .expect("institution cid should generate")
        .into_bytes();

        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &institution_number),
                identity_version(&institution_number),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn occupy_cid_binds_account_and_stays_anonymous() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("OCC-BIND");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            7,
            expires_at,
            occupy_signature(7, &cid_bytes, expires_at),
        ));
        // 占即绑:双向绑定成立。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(7));
        assert_eq!(CidByAccountId::<Test>::get(7), Some(cid(&cid_bytes)));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        // 登记记录:注册局 registrar + 空居住地(全国号)+ Active + commitment=blake2_256(账户)。
        let rec = CidRegistry::<Test>::get(cid(&cid_bytes)).expect("record");
        assert_eq!(rec.registrar_cid_number, registrar_cid_number());
        assert!(rec.residence_province_code.is_empty());
        assert!(rec.residence_city_code.is_empty());
        assert_eq!(rec.status, CidRecordStatus::Active);
        assert_eq!(rec.commitment, sp_io::hashing::blake2_256(&7u64.encode()));
        // 匿名:无投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&cid_bytes)));
    });
}

#[test]
fn occupy_cid_accepts_resident_natp_type() {
    new_test_ext().execute_with(|| {
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "occ-natp",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&natp),
            8,
            expires_at,
            occupy_signature(8, &natp, expires_at),
        ));
        assert_eq!(CidByAccountId::<Test>::get(8), Some(cid(&natp)));
    });
}

#[test]
fn occupy_cid_rejects_replay_after_first_binding() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("OCC-1", 1);
        let cid_bytes = citizen_cid_number("OCC-1");
        let expires_at = rebind_expires_at();
        // 首次绑定证明公开后不得再次提交，哪怕注册局、CID 与账户完全相同。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                expires_at,
                occupy_signature(1, &cid_bytes, expires_at),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
        // 另一账户抢占同 CID 同样由“只允许 revision=0 首次绑定”拒绝。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                expires_at,
                occupy_signature(2, &cid_bytes, expires_at),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
    });
}

#[test]
fn register_without_occupation_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("NO-OCC")),
                identity_version(&citizen_cid_number("NO-OCC")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidNotOccupied
        );
    });
}

#[test]
fn revoke_cid_tombstones_and_revokes_bound_identity() {
    new_test_ext().execute_with(|| {
        occupy_tag("RV-1");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("RV-1")),
            identity_version(&citizen_cid_number("RV-1")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CountryVotingCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("RV-1")),
        ));
        // 登记表墓碑 + 身份联动吊销 + 退出人口分母。
        let rec = CidRegistry::<Test>::get(cid(&citizen_cid_number("RV-1"))).expect("record kept");
        assert_eq!(rec.status, CidRecordStatus::Revoked);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(cid(&citizen_cid_number("RV-1"))),
            Some(2)
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(cid(&citizen_cid_number("RV-1")))
                .expect("identity kept")
                .citizen_status,
            CitizenStatus::Revoked
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);

        // 再吊销:已墓碑。
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("RV-1")),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        // 墓碑号原绑定账户也不可再占(号码永不复用;绑定闭环放行、do_occupy 见墓碑拒)。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("RV-1")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
        // 墓碑号也不能再注册身份：永久 CID 身份与账户绑定均保留，
        // 归属检查先于墓碑检查拦截(双保险,谁先触发都拒绝)。
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(2, &citizen_cid_number("RV-1")),
                identity_version(&citizen_cid_number("RV-1")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
    });
}

#[test]
fn revoke_cid_revision_preflight_failures_leave_state_unchanged() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("RV-REV");
        occupy_tag("RV-REV");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let cid_number = cid(&cid_bytes);
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let population_before = CountryVotingCount::<Test>::get();

        BindingRevisionByCid::<Test>::remove(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionMissing
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);

        BindingRevisionByCid::<Test>::insert(&cid_number, u64::MAX);
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionOverflow
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            Some(u64::MAX)
        );
    });
}

#[test]
fn permanent_cid_update_keeps_registry_record_active() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("CHG-A", 1);
        occupy_tag_as("CHG-B", 2);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("CHG-A")),
            identity_version(&citizen_cid_number("CHG-A")),
            identity_expires_at(),
            valid_signature(),
        ));
        let mut updated = voting_payload(1, &citizen_cid_number("CHG-A"));
        updated.residence_town_code = code(b"4301002");
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            updated.clone(),
            voting_version(&updated),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("CHG-A")))
                .expect("permanent record kept")
                .status,
            CidRecordStatus::Active
        );
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("CHG-B")))
                .expect("unrelated occupied record kept")
                .status,
            CidRecordStatus::Active
        );
    });
}

/// 创世登记条数直接就是初始计数：挖矿页读到的第一个数字必须等于创世公民数。
#[test]
fn cid_count_starts_at_genesis_binding_len() {
    let registrar = registrar_cid_number();
    new_test_ext_with_cid_bindings(vec![
        (
            cid(&citizen_cid_number("count-genesis-a")),
            41,
            registrar.clone(),
        ),
        (cid(&citizen_cid_number("count-genesis-b")), 42, registrar),
    ])
    .execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 无创世登记时计数为 0，不是 `ValueQuery` 默认值巧合，而是没有任何 Active 记录。
#[test]
fn cid_count_is_zero_without_any_occupied_cid() {
    new_test_ext().execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 0);
        assert_eq!(CidRegistry::<Test>::iter().count(), 0);
    });
}

/// 自助占号每成功一个 +1。
#[test]
fn cid_count_increments_on_self_occupy() {
    new_test_ext().execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 0);
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&citizen_cid_number("count-self-1")),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(2),
            cid(&citizen_cid_number("count-self-2")),
        ));
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 注册局占即绑同样 +1，与自助口径一致。
#[test]
fn cid_count_increments_on_registrar_occupy() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-reg-1", 1);
        assert_eq!(CidCount::<Test>::get(), 1);
        occupy_tag_as("count-reg-2", 2);
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 幂等重入不得加 1：同账户同 CID 重复提交只是恢复路径，链上没有新登记记录。
/// 少了这条守卫，用户建档失败重试一次就把计数虚增一个，且永远无法回落。
#[test]
fn cid_count_stays_flat_on_idempotent_reoccupy() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("count-idem");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        // 同账户同 CID 重放：do_occupy_cid 走幂等分支，不写库也不计数。
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        assert_eq!(CidRegistry::<Test>::iter().count(), 1);
    });
}

/// 注册局吊销 −1，且墓碑仍留在登记表里：计数只跟 Active，不跟记录条数。
#[test]
fn cid_count_decrements_on_revoke_cid_while_tombstone_remains() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-revoke", 1);
        assert_eq!(CidCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("count-revoke")),
        ));
        assert_eq!(CidCount::<Test>::get(), 0);
        // 墓碑不删除：登记表仍有 1 条记录，但它已不是有效 CID。
        assert_eq!(CidRegistry::<Test>::iter().count(), 1);
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("count-revoke")))
                .expect("tombstone kept")
                .status,
            CidRecordStatus::Revoked
        );
    });
}

/// 身份吊销联动路径（`revoke_identity`）同样 −1：两条吊销入口共用 tombstone helper，
/// 任何一条漏减都会让计数永久偏高。
#[test]
fn cid_count_decrements_on_revoke_identity() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-revoke-id", 1);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("count-revoke-id")),
            identity_version(&citizen_cid_number("count-revoke-id")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("count-revoke-id")),
        ));
        assert_eq!(CidCount::<Test>::get(), 0);
    });
}

/// 版本必须精确等于链上当前值，超前同样被拒。
#[test]
fn future_identity_version_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("VER-AHEAD");
        let cid_bytes = citizen_cid_number("VER-AHEAD");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes) + 1,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );
    });
}
