//! 实体共用编码、动作权限和岗位规则测试。
mod contract {

    use crate::*;
    use codec::Encode;

    /// 机构记录的声明序就是 SCALE 存储契约，NodeGuard 按同一顺序完整解码。
    #[test]
    fn institution_info_field_order_matches_node_guard() {
        let value = InstitutionInfo {
            cid_full_name: b"full".to_vec(),
            cid_short_name: b"short".to_vec(),
            town_code: b"001".to_vec(),
            legal_representative: Some(LegalRepresentative {
                family_name: b"family".to_vec(),
                given_name: b"given".to_vec(),
                cid_number: b"citizen-cid".to_vec(),
                account_id: [9u8; 32],
            }),
            institution_code: *b"NRCG",
            created_at: 7u32,
        };
        assert_eq!(
            value.encode(),
            (
                b"full".to_vec(),
                b"short".to_vec(),
                b"001".to_vec(),
                Some((
                    b"family".to_vec(),
                    b"given".to_vec(),
                    b"citizen-cid".to_vec(),
                    [9u8; 32],
                )),
                *b"NRCG",
                7u32,
            )
                .encode()
        );
    }
}
mod actions {
    use crate::RolePermissionOperation;
    use primitives::cid::china::citizenchain::{
        ROLE_CODE_GENESIS_PRODUCT_MANAGER, ROLE_CODE_GENESIS_PROGRAMMER,
    };
    use primitives::cid::code::{FSC, NJD, PRB};
    use primitives::governance_skeleton::{
        ROLE_CODE_CHIEF_JUSTICE, ROLE_CODE_COMMITTEE_MEMBER, ROLE_CODE_DIRECTOR, ROLE_CODE_JUSTICE,
    };
    use primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE;

    use crate::business_action::*;
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
mod roles {
    use codec::Encode;

    use crate::institution_role::*;

    /// Node Guard 按枚举声明序解码岗位和任职状态；任一重排都必须先修改节点协议。
    #[test]
    fn status_discriminants_match_governance_skeleton() {
        assert_eq!(
            InstitutionRoleStatus::Active as u8,
            primitives::governance_skeleton::ROLE_STATUS_ACTIVE
        );
        assert_eq!(
            InstitutionAssignmentStatus::Active as u8,
            primitives::governance_skeleton::ASSIGNMENT_STATUS_ACTIVE
        );
    }

    /// 权限操作和授权主体的枚举序号属于跨端 SCALE 契约。
    #[test]
    fn role_permission_discriminants_are_stable() {
        assert_eq!(RolePermissionOperation::Propose.encode(), vec![0]);
        assert_eq!(RolePermissionOperation::Vote.encode(), vec![1]);

        let institution =
            AuthorizationSubject::<Vec<u8>, Vec<u8>, [u8; 32]>::Institution(RoleSubject {
                cid_number: b"CID-1".to_vec(),
                role_code: b"ROLE-1".to_vec(),
            });
        let personal =
            AuthorizationSubject::<Vec<u8>, Vec<u8>, [u8; 32]>::PersonalMultisig([9u8; 32]);
        assert_eq!(institution.encode()[0], 0);
        assert_eq!(personal.encode()[0], 1);
    }

    #[test]
    fn dynamic_role_code_uses_exact_domain_and_never_accepts_lowercase() {
        let code = generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 0, 42);
        assert_eq!(code.len(), 34);
        assert!(code.starts_with(b"R_"));
        assert!(code[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte)));
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 1, 42),
            "nonce 变化必须生成不同岗位码"
        );
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 0, 43),
            "proposal_id 变化必须生成不同岗位码"
        );
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pri-mgmt", b"CID-1", 0, 42),
            "MODULE_TAG 变化必须生成不同岗位码(哈希域分隔)"
        );
    }

    /// 完整岗位业务权限的字段声明序就是五端共同使用的 SCALE 格式。
    #[test]
    fn role_business_permission_field_order_is_stable() {
        let permission = RoleBusinessPermission {
            role_subject: RoleSubject {
                cid_number: b"CID-1".to_vec(),
                role_code: b"ROLE-1".to_vec(),
            },
            business_action_id: BusinessActionId {
                module_tag: b"rt-upg".to_vec(),
                action_code: 7,
            },
            operation: RolePermissionOperation::Propose,
        };
        assert_eq!(
            permission.encode(),
            (
                (b"CID-1".to_vec(), b"ROLE-1".to_vec()),
                (b"rt-upg".to_vec(), 7u32),
                RolePermissionOperation::Propose,
            )
                .encode()
        );
    }

    /// 岗位字段声明序就是 `PublicManage::InstitutionRoles` 的链上 SCALE 值格式。
    #[test]
    fn institution_role_field_order_matches_node_guard() {
        let role = InstitutionRole {
            cid_number: b"CID-1".to_vec(),
            role_code: b"ROLE-1".to_vec(),
            role_name: "岗位一".as_bytes().to_vec(),
            term_required: true,
            role_status: InstitutionRoleStatus::Active,
        };
        assert_eq!(
            role.encode(),
            (
                b"CID-1".to_vec(),
                b"ROLE-1".to_vec(),
                "岗位一".as_bytes().to_vec(),
                true,
                InstitutionRoleStatus::Active,
            )
                .encode()
        );
    }

    /// 任职字段声明序就是 `PublicManage::InstitutionRoleAssignments` 元素的链上 SCALE 格式。
    #[test]
    fn institution_assignment_field_order_matches_node_guard() {
        let assignment = InstitutionAdminAssignment {
            cid_number: b"CID-1".to_vec(),
            account_id: [7u8; 32],
            role_code: b"ROLE-1".to_vec(),
            term_start: 10,
            term_end: 20,
            assignment_source: InstitutionAssignmentSource::PopularElection,
            assignment_source_ref: b"VOTE-1".to_vec(),
            assignment_status: InstitutionAssignmentStatus::Active,
        };
        assert_eq!(
            assignment.encode(),
            (
                b"CID-1".to_vec(),
                [7u8; 32],
                b"ROLE-1".to_vec(),
                10u32,
                20u32,
                InstitutionAssignmentSource::PopularElection,
                b"VOTE-1".to_vec(),
                InstitutionAssignmentStatus::Active,
            )
                .encode()
        );
    }
}
