//! 复用本模块真实测试环境的行为与边界回归。
use super::*;

/// 四端共同签名协议的字段声明顺序就是 SCALE 顺序；任何重排都必须让本测试失败。
#[test]
fn cid_authorization_scale_contract_keeps_canonical_field_order() {
    use codec::Encode;

    let genesis_hash = sp_core::H256::repeat_byte(0x11);
    let cid_number: CidNumberBound = b"CID-AUTH".to_vec().try_into().expect("authorization CID");
    let current_account_id = [0x22u8; 32];
    let new_account_id = [0x33u8; 32];

    let rebind = CidRebindAuthorization {
        genesis_hash,
        cid_number: cid_number.clone(),
        current_account_id,
        new_account_id,
        expected_binding_revision: 7,
        expires_at: 8,
    };
    assert_eq!(
        rebind.encode(),
        (
            genesis_hash,
            cid_number.clone(),
            current_account_id,
            new_account_id,
            7u64,
            8u64,
        )
            .encode()
    );

    let occupy = CidOccupyAuthorization {
        genesis_hash,
        cid_number: cid_number.clone(),
        account_id: new_account_id,
        expected_binding_revision: 0,
        expires_at: 9,
    };
    assert_eq!(
        occupy.encode(),
        (genesis_hash, cid_number, new_account_id, 0u64, 9u64).encode()
    );
}

#[test]
fn self_rebind_cid_account_id_moves_binding_to_new_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind1");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        // 新账户 2 作 origin(证新账户受控)，当前账户 1 授权签名。
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(2),
            cid(&cid_bytes),
            1,
            expires_at,
            rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at),
        ));
        // 换绑后：CID 绑定新账户 2，此前账户 1 的反向索引清除。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        assert_eq!(CidByAccountId::<Test>::get(2), Some(cid(&cid_bytes)));
        assert_eq!(CidByAccountId::<Test>::get(1), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(2));
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_unoccupied_cid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&citizen_cid_number("rebind_none")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::NotBoundToAnyCid
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_invalid_current_account_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind2");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let bad_sig: pallet::SignatureOf<Test> = b"nope".to_vec().try_into().expect("sig fits");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                bad_sig,
            ),
            Error::<Test>::InvalidRebindSignature
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_new_account_bound_to_another_cid() {
    new_test_ext().execute_with(|| {
        // 账户 1 占 cidA;账户 2 占 cidB;把 cidA 换绑到已绑 cidB 的账户 2 → 拒。
        let cid_a = citizen_cid_number("rebind3a");
        let cid_b = citizen_cid_number("rebind3b");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_a),
        ));
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(2),
            cid(&cid_b),
        ));
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_a),
                1,
                expires_at,
                rebind_signature(1, &cid_a, 1, 2, 1, expires_at),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_civic_cid() {
    new_test_ext().execute_with(|| {
        // civic:占号 + 注册投票身份 → 有 VotingIdentity → 自助换绑拒(q1,走注册局)。
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
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&citizen_cid_number("0001")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CivicRebindRequiresRegistrar
        );
    });
}

#[test]
fn self_rebind_rejects_wrong_revision_expiry_same_account_and_cross_chain_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind-guards");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        let signature = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                2,
                expires_at,
                signature.clone(),
            ),
            Error::<Test>::BindingRevisionMismatch
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                test_time_secs(),
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                test_time_secs() + MAX_CID_AUTHORIZATION_LIFETIME_SECS + 1,
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(1),
                cid(&cid_bytes),
                1,
                expires_at,
                rebind_signature(1, &cid_bytes, 1, 1, 1, expires_at),
            ),
            Error::<Test>::RebindAccountIdUnchanged
        );

        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(9));
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidRebindSignature
        );
    });
}

#[test]
fn self_rebind_rejects_a_historical_authorization_after_two_later_rotations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind-replay");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        let historical = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(2),
            cid(&cid_bytes),
            1,
            expires_at,
            historical.clone(),
        ));
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
            2,
            expires_at,
            rebind_signature(2, &cid_bytes, 2, 1, 2, expires_at),
        ));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(3));

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                historical,
            ),
            Error::<Test>::BindingRevisionMismatch
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_moves_binding_to_new_account_id() {
    new_test_ext().execute_with(|| {
        // 注册局占号绑账户 1（匿名），随后由有权限注册局直接换绑到账户 2；
        // 本入口只要求注册局鉴权与新账户签名，不要求当前账户签名。
        occupy_tag_as("adm1", 1);
        let cid_bytes = citizen_cid_number("adm1");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::admin_rebind_cid_account_id(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            2,
            1,
            expires_at,
            rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
        ));
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        assert_eq!(CidByAccountId::<Test>::get(2), Some(cid(&cid_bytes)));
        assert_eq!(CidByAccountId::<Test>::get(1), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(2));
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_unoccupied_cid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("adm_none")),
                2,
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::NotBoundToAnyCid
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_rebinds_civic_cid_with_scoped_registrar() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("0001");
        occupy_tag("0001");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let voting_identity_before =
            VotingIdentityByCid::<Test>::get(cid(&cid_bytes)).expect("voting identity exists");
        let candidate_identity_before = CandidateIdentityByCid::<Test>::get(cid(&cid_bytes))
            .expect("candidate identity exists");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::admin_rebind_cid_account_id(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            2,
            1,
            expires_at,
            rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
        ));
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        // 投票、候选资料始终按 CID 存储；换绑只替换当前签名账户，不迁移或改写资料。
        assert_eq!(
            VotingIdentityByCid::<Test>::get(cid(&cid_bytes)),
            Some(voting_identity_before)
        );
        assert_eq!(
            CandidateIdentityByCid::<Test>::get(cid(&cid_bytes)),
            Some(candidate_identity_before)
        );
        assert!(CitizenIdentity::citizen_subject(&1).is_none());
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_none());
        assert_eq!(
            CitizenIdentity::citizen_subject(&2)
                .expect("实名 CID 应由新钱包继续使用")
                .cid_number,
            cid(&cid_bytes)
        );
        assert!(CitizenIdentity::voting_subject(&2, &town_scope()).is_some());
        assert!(CitizenIdentity::candidate_subject(&2, &town_scope()).is_some());
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_unauthorized_registrar() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("adm3", 1);
        let cid_bytes = citizen_cid_number("adm3");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(999),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
            ),
            Error::<Test>::UnauthorizedRegistrar
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_invalid_new_signature() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("adm2", 1);
        let bad_sig: pallet::SignatureOf<Test> = b"nope".to_vec().try_into().expect("sig fits");
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("adm2")),
                2,
                1,
                rebind_expires_at(),
                bad_sig,
            ),
            Error::<Test>::InvalidAdminRebindSignature
        );
    });
}

#[test]
fn admin_rebind_rejects_stale_expired_same_account_and_cross_chain_authorizations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("adm-guards");
        occupy_tag_as("adm-guards", 1);
        let expires_at = rebind_expires_at();
        let signature = rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at);

        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                2,
                expires_at,
                signature.clone(),
            ),
            Error::<Test>::BindingRevisionMismatch
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                test_time_secs(),
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                1,
                expires_at,
                rebind_signature(1, &cid_bytes, 1, 1, 1, expires_at),
            ),
            Error::<Test>::RebindAccountIdUnchanged
        );

        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(6));
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidAdminRebindSignature
        );
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(1));
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_new_account_bound_to_another_cid() {
    new_test_ext().execute_with(|| {
        // adm4a 绑账户 1、adm4b 绑账户 2;把 adm4a 换绑到已绑 adm4b 的账户 2 → 拒。
        occupy_tag_as("adm4a", 1);
        occupy_tag_as("adm4b", 2);
        let cid_bytes = citizen_cid_number("adm4a");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn occupy_cid_rejects_invalid_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("BAD-SIG");
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                rebind_expires_at(),
                b"nope".to_vec().try_into().expect("signature should fit"),
            ),
            Error::<Test>::InvalidOccupySignature
        );
    });
}

#[test]
fn occupy_cid_rejects_expired_overlong_and_cross_chain_authorizations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("OCC-GUARDS");
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                test_time_secs(),
                occupy_signature(1, &cid_bytes, test_time_secs()),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        let overlong = test_time_secs() + MAX_CID_AUTHORIZATION_LIFETIME_SECS + 1;
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                overlong,
                occupy_signature(1, &cid_bytes, overlong),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );

        let expires_at = rebind_expires_at();
        let signature = occupy_signature(1, &cid_bytes, expires_at);
        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(7));
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidOccupySignature
        );
    });
}

#[test]
fn occupy_cid_rejects_unauthorized_registrar_and_bad_number() {
    new_test_ext().execute_with(|| {
        // 非在册注册局账户:匿名鉴权拒。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(999),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("OCC-2")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::UnauthorizedRegistrar
        );
        // 公权机构号打公民占号入口:类型断言拒(CTZN|NATP 之外)。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&public_cid_number("OCC-2")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn revoked_cid_rejects_self_and_admin_rebind_without_state_changes() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("RV-REBIND");
        occupy_tag_as("RV-REBIND", 1);
        let expires_at = rebind_expires_at();
        let previous_self_authorization = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);
        let previous_admin_authorization = rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
        ));
        let cid_number = cid(&cid_bytes);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(2));

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid_number.clone(),
                1,
                expires_at,
                previous_self_authorization,
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
                2,
                1,
                expires_at,
                previous_admin_authorization,
            ),
            Error::<Test>::CidAlreadyRevoked
        );

        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(AccountIdByCid::<Test>::get(&cid_number), Some(1));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid_number.clone()));
        assert_eq!(CidByAccountId::<Test>::get(2), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(2));
    });
}

/// 过期授权被拒。
#[test]
fn expired_identity_authorization_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("IDENT-EXPIRED");
        let cid_bytes = citizen_cid_number("IDENT-EXPIRED");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes),
                test_time_secs(),
                valid_signature(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
    });
}

/// 授权有效期超过 600 秒上限被拒。
#[test]
fn overlong_identity_authorization_lifetime_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("IDENT-LONG");
        let cid_bytes = citizen_cid_number("IDENT-LONG");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes),
                identity_expires_at() + 1,
                valid_signature(),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );
    });
}

/// 授权载荷 SCALE 字段序：创世哈希、载荷、版本、过期时间；四端按此构造待签字节。
#[test]
fn citizen_identity_authorization_scale_contract_is_stable() {
    new_test_ext().execute_with(|| {
        let payload = voting_payload(1, &citizen_cid_number("SCALE-ORDER"));
        let authorization = CitizenIdentityAuthorization {
            genesis_hash: frame_system::Pallet::<Test>::block_hash(0u64),
            payload: payload.clone(),
            expected_identity_version: 7,
            expires_at: 1_700_000_000,
        };

        let mut expected = frame_system::Pallet::<Test>::block_hash(0u64).encode();
        expected.extend_from_slice(&payload.encode());
        expected.extend_from_slice(&7u64.encode());
        expected.extend_from_slice(&1_700_000_000u64.encode());
        assert_eq!(authorization.encode(), expected);
    });
}
