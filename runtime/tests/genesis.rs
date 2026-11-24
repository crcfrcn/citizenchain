//! 验证创世预设、Runtime API及机构和账户初始状态。

#[test]
fn presets_match_builder_and_runtime_api() {
    use codec::{DecodeAll, Encode};
    use sp_genesis_builder::{PresetId, LOCAL_TESTNET_RUNTIME_PRESET};

    let local = PresetId::from(LOCAL_TESTNET_RUNTIME_PRESET);
    let expected = serde_json::to_vec(&::genesis::config::genesis_config::<RuntimeGenesisConfig>())
        .expect("完整创世配置可序列化");
    assert_eq!(::genesis::config::preset_names(), vec![local.clone()]);
    assert_eq!(
        ::genesis::config::get_preset::<RuntimeGenesisConfig>(&local),
        Some(expected.clone()),
    );
    let encoded =
        crate::apis::api::dispatch("GenesisBuilder_get_preset", &Some(local.clone()).encode())
            .expect("实际Runtime API已注册");
    let actual =
        Option::<Vec<u8>>::decode_all(&mut &encoded[..]).expect("实际Runtime API返回规范SCALE");
    assert_eq!(actual, Some(expected));

    // None按FRAME合同返回实际汇总类型默认配置；未知和空预设均明确拒绝。
    let encoded = crate::apis::api::dispatch(
        "GenesisBuilder_get_preset",
        &Option::<PresetId>::None.encode(),
    )
    .expect("默认配置入口已注册");
    let actual = Option::<Vec<u8>>::decode_all(&mut &encoded[..]).expect("默认配置返回规范SCALE");
    assert_eq!(
        actual,
        Some(serde_json::to_vec(&RuntimeGenesisConfig::default()).expect("实际默认配置可序列化"))
    );
    for id in [PresetId::from(""), PresetId::from("unknown")] {
        assert!(::genesis::config::get_preset::<RuntimeGenesisConfig>(&id).is_none());
        let encoded = crate::apis::api::dispatch("GenesisBuilder_get_preset", &Some(id).encode())
            .expect("拒绝路径API仍已注册");
        assert_eq!(
            Option::<Vec<u8>>::decode_all(&mut &encoded[..]).expect("拒绝结果返回规范SCALE"),
            None
        );
    }
    let encoded =
        crate::apis::api::dispatch("GenesisBuilder_preset_names", &[]).expect("预设清单API已注册");
    assert_eq!(
        Vec::<PresetId>::decode_all(&mut &encoded[..]).expect("预设清单返回规范SCALE"),
        vec![local]
    );
}

use crate::AccountId;
use crate::RuntimeGenesisConfig;
use codec::Decode;
use ed25519_dalek::VerifyingKey;
use primitives::cid::china::china_cb::CHINA_CB;
use primitives::{
    cid::china::china_ch::CHINA_CH,
    cid::china::china_zf::CHINA_ZF,
    cid::china::citizenchain::{
        LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER,
    },
    cid::code::{institution_code_from_cid_number, FRG},
    core_const::SS58_FORMAT,
    genesis::{GENESIS_ISSUANCE, HE_FUND_ISSUANCE},
};
use serde_json::{json, Value};
use sp_core::crypto::{Ss58AddressFormat, Ss58Codec};
use std::collections::BTreeSet;

fn frozen_patch() -> Value {
    let data: Value = serde_json::from_slice(::genesis::config::frozen_chain_spec()).unwrap();
    data.pointer("/genesis/runtimeGenesis/patch").unwrap().clone()
}

fn foundation_genesis_wallet() -> AccountId {
    let patch = frozen_patch();
    let wallet = patch["citizenIdentity"]["initialCidBindings"][0][1].as_str().unwrap();
    AccountId::from_ss58check(wallet).unwrap()
}

fn account_to_genesis_ss58(account: &AccountId) -> String {
    account.to_ss58check_with_version(Ss58AddressFormat::custom(SS58_FORMAT))
}

fn json_amount_to_u128(v: &Value) -> Option<u128> {
    if let Some(value) = v.as_u64() {
        return Some(value as u128);
    }
    v.as_str().and_then(|s| s.parse::<u128>().ok())
}

#[test]
fn genesis_contains_nrc_and_all_provincialbank_balances() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let balances = patch["balances"]["balances"]
        .as_array()
        .expect("balances.balances should be an array");

    // 创世包含 1 个国家储委会多签账户 + 19 个历史拨付账户 + 43 个省储行 stake 质押地址
    // + 1 个国家储委会两和基金账户 + 6 笔创世分账账户。
    let nrc_initial_grants = primitives::count_const::NRC_COMMITTEE_SEATS as usize;
    assert_eq!(balances.len(), 1 + nrc_initial_grants + CHINA_CH.len() + 1 + 6);

    assert_eq!(patch["balances"], frozen_patch()["balances"]);
    assert_eq!(balances.iter().filter(|row| json_amount_to_u128(&row[1])==Some(1_000_000_000)).count(), nrc_initial_grants);

    // 每家省储行的创立发行必须逐户精确进入无私钥 stake_account，不能改发主账户或汇总账户。
    for bank in CHINA_CH {
        let stake_ss58 = account_to_genesis_ss58(&AccountId::new(bank.stake_account));
        let amount = balances
            .iter()
            .find_map(|entry| {
                let fields = entry.as_array()?;
                (fields.first()?.as_str()? == stake_ss58)
                    .then(|| fields.get(1).and_then(json_amount_to_u128))
                    .flatten()
            })
            .expect("每家省储行 stake_account 都必须有创立发行余额");
        assert_eq!(amount, bank.stake_amount);
    }
}

#[test]
fn genesis_issuance_splits_nrc_main_and_six_allocations() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let balances = patch["balances"]["balances"]
        .as_array()
        .expect("balances.balances should be an array");

    let nrc_account = CHINA_CB
        .first()
        .and_then(|n| AccountId::decode(&mut &n.main_account[..]).ok())
        .expect("NRC main_account must decode to AccountId");
    let nrc_ss58 = account_to_genesis_ss58(&nrc_account);

    let nrc_amount = balances
        .iter()
        .find_map(|entry| {
            let arr = entry.as_array()?;
            let account = arr.first()?.as_str()?;
            if account == nrc_ss58 {
                arr.get(1).and_then(json_amount_to_u128)
            } else {
                None
            }
        })
        .expect("NRC balance entry should exist");

    // 创世发行分配到国家储委会多签账户 = 总发行量 - 19 笔国储会历史拨付 - 六账户分账总额(700 亿元)。
    let grant_each: u128 = 1_000_000_000;
    let nrc_grants = u128::from(primitives::count_const::NRC_COMMITTEE_SEATS);
    let allocated_total: u128 = 7_000_000_000_000; // 700 亿元 = 6 笔创世分账合计
    let expected_nrc = GENESIS_ISSUANCE - grant_each * nrc_grants - allocated_total;
    assert_eq!(nrc_amount, expected_nrc);

    let total_in_patch: u128 = balances
        .iter()
        .map(|entry| {
            entry
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(json_amount_to_u128)
                .expect("each balance amount must be u64 number or u128 string")
        })
        .sum();
    let total_provincialbank_stake: u128 = CHINA_CH.iter().map(|n| n.stake_amount).sum();

    // 创世总注入 = 创世发行 + 省储行创立发行 + 两和基金发行。
    // 六账户分账从 NRC 主账户份额切出,创世发行总量不变,故此等式保持成立。
    assert_eq!(
        total_in_patch,
        GENESIS_ISSUANCE + total_provincialbank_stake + HE_FUND_ISSUANCE
    );
}

#[test]
fn genesis_six_allocations_have_exact_amounts() {
    use primitives::account_derive::AccountKind;
    use primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT;
    use primitives::cid::china::china_sf::CHINA_SF;
    use primitives::cid::china::china_zf::CHINA_ZF;
    use primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
    use primitives::cid::code::{institution_code_from_cid_number, FRG, FSC};
    use primitives::core_const::SS58_FORMAT;

    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let balances = patch["balances"]["balances"]
        .as_array()
        .expect("balances.balances should be an array");

    // 按 SS58 地址在创世 balances 中查该账户金额;不存在即断言失败。
    let amount_of = |account: &AccountId| -> u128 {
        let ss58 = account_to_genesis_ss58(account);
        balances
            .iter()
            .find_map(|entry| {
                let arr = entry.as_array()?;
                (arr.first()?.as_str()? == ss58)
                    .then(|| arr.get(1).and_then(json_amount_to_u128))
                    .flatten()
            })
            .unwrap_or_else(|| panic!("创世 balances 缺少账户 {ss58}"))
    };

    // 联邦公民安全基金:派生地址必须与 genesis::config / seeder 完全一致。
    let fsc_cid = CHINA_ZF
        .iter()
        .find(|i| institution_code_from_cid_number(i.cid_number) == Some(FSC))
        .expect("CHINA_ZF must contain FSC")
        .cid_number;
    let fcsf = AccountId::new(
        AccountKind::InstitutionFederalCitizenSecurityFund {
            cid_number: fsc_cid.as_bytes(),
        }
        .derive(SS58_FORMAT),
    );
    let frg_main = CHINA_ZF
        .iter()
        .find(|i| institution_code_from_cid_number(i.cid_number) == Some(FRG))
        .expect("CHINA_ZF must contain FRG")
        .main_account;
    let njd_main = CHINA_SF
        .first()
        .expect("CHINA_SF must contain National Judicial Yuan")
        .main_account;

    // 六账户金额逐一精确核对(单位:分)。
    assert_eq!(amount_of(&fcsf), 1_000_000_000_000); // 联邦公民安全基金 100 亿元
    assert_eq!(
        amount_of(&foundation_genesis_wallet()),
        2_000_000_000_000
    ); // 公民程伟钱包 200 亿元
    assert_eq!(
        amount_of(&AccountId::new(CITIZENCHAIN_FOUNDATION.main_account)),
        1_000_000_000_000
    ); // 技术发展基金会主账户 100 亿元
    assert_eq!(amount_of(&AccountId::new(frg_main)), 1_000_000_000_000); // 联邦注册局主账户 100 亿元
    assert_eq!(amount_of(&AccountId::new(njd_main)), 1_000_000_000_000); // 国家司法院主账户 100 亿元
    assert_eq!(
        amount_of(&AccountId::new(SAFETY_FUND_ACCOUNT)),
        1_000_000_000_000
    ); // 国储会安全基金 100 亿元
}

#[test]
fn genesis_omits_national_institutional_registry_without_runtime_pallet() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    assert!(
            patch.get("nationalInstitutionalRegistry").is_none(),
            "nationalInstitutionalRegistry should be absent until the runtime pallet is wired into genesis"
        );
}

#[test]
fn frozen_grandpa_registrations_match_cids_and_valid_keys() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    assert_eq!(patch["grandpa"], frozen_patch()["grandpa"]);
    assert_eq!(patch["grandpa"]["authorities"].as_array().unwrap().len(), 1);
    let registered: Vec<(Vec<u8>, [u8; 32])> =
        serde_json::from_value(patch["grandpaKey"]["registeredKeys"].clone()).unwrap();
    let mut unique = BTreeSet::new();
    assert_eq!(registered.len(), 44);
    for ((cid, key), institution) in registered.iter().zip(CHINA_CB) {
        assert_eq!(cid.as_slice(), institution.cid_number.as_bytes());
        assert!(!VerifyingKey::from_bytes(key).unwrap().is_weak());
        assert!(unique.insert(*key));
    }
}

#[test]
fn fixed_institution_seats_match_declared_positions() {
    use primitives::cid::code::{FRG, NJD, NRC, PRB, PRC};
    use primitives::count_const::*;
    use primitives::governance_skeleton::{fixed_role_specs, frg_province_groups};
    use primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE;

    for (code, count) in [
        (NRC, NRC_COMMITTEE_SEATS), (PRC, PRC_COMMITTEE_SEATS),
        (PRB, PRB_DIRECTOR_SEATS), (NJD, NJD_JUDICIAL_SEATS),
    ] {
        let roles = fixed_role_specs(code);
        let (representative, ordinary): (Vec<_>, Vec<_>) = roles.into_iter()
            .partition(|role| role.role_code == ROLE_CODE_LEGAL_REPRESENTATIVE);
        assert_eq!(representative.len(), 1);
        assert_eq!(representative[0].seats, 1);
        assert_eq!(ordinary.iter().map(|role| role.seats).sum::<u32>(), count);
    }
    let groups = frg_province_groups();
    assert_eq!(groups.len(), primitives::cid::code::PROVINCE_CODE_INFOS.len());
    assert!(groups.iter().all(|(_, seats)| *seats == FRG_COMMISSIONER_SEATS));
    assert_eq!(fixed_role_specs(FRG).len(), 0);
}

#[test]
fn genesis_json_deserializes_into_runtime_genesis_config() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let parsed: Result<RuntimeGenesisConfig, _> = serde_json::from_value(patch);
    assert!(
        parsed.is_ok(),
        "runtime genesis json should deserialize: {:?}",
        parsed.err()
    );
}

#[test]
fn genesis_account_strings_deserialize_individually() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();

    for entry in patch["balances"]["balances"]
        .as_array()
        .expect("balances should be an array")
    {
        let account = entry[0].clone();
        let parsed: Result<AccountId, _> = serde_json::from_value(account.clone());
        assert!(
            parsed.is_ok(),
            "balance account should deserialize: value={account:?} err={:?}",
            parsed.err()
        );
    }

    for account in patch["resolutionIssue"]["allowedRecipients"]
        .as_array()
        .expect("allowedRecipients should be an array")
    {
        let parsed: Result<AccountId, _> = serde_json::from_value(account.clone());
        assert!(
            parsed.is_ok(),
            "allowed recipient_account_id should deserialize: value={account:?} err={:?}",
            parsed.err()
        );
    }

    let binding = patch["identity"]["initialCidBindings"][0]
        .as_array()
        .expect("citizen identity binding should be a three-field tuple");
    let parsed: Result<AccountId, _> = serde_json::from_value(binding[1].clone());
    assert!(
        parsed.is_ok(),
        "citizen identity account_id should deserialize: value={:?} err={:?}",
        binding[1],
        parsed.err()
    );
}

#[test]
fn genesis_top_level_sections_deserialize_individually() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();

    let balances: Result<pallet_balances::GenesisConfig<crate::Runtime>, _> =
        serde_json::from_value(patch["balances"].clone());
    assert!(
        balances.is_ok(),
        "balances should deserialize: {:?}",
        balances.err()
    );

    let grandpa: Result<pallet_grandpa::GenesisConfig<crate::Runtime>, _> =
        serde_json::from_value(patch["grandpa"].clone());
    assert!(
        grandpa.is_ok(),
        "grandpa should deserialize: {:?}",
        grandpa.err()
    );

    let resolutionissue: Result<resolutionissue::GenesisConfig<crate::Runtime>, _> =
        serde_json::from_value(patch["resolutionIssue"].clone());
    assert!(
        resolutionissue.is_ok(),
        "resolutionIssuance should deserialize: {:?}",
        resolutionissue.err()
    );

    let identity: Result<identity::GenesisConfig<crate::Runtime>, _> =
        serde_json::from_value(patch["identity"].clone());
    assert!(
        identity.is_ok(),
        "citizenIdentity should deserialize: {:?}",
        identity.err()
    );
}

#[test]
fn genesis_contains_frozen_citizen_identity_binding() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let bindings = patch["identity"]["initialCidBindings"]
        .as_array()
        .expect("citizen identity bindings should be an array");
    assert_eq!(bindings.len(), 1, "冻结预设含原有公民绑定");

    let binding = bindings[0]
        .as_array()
        .expect("citizen identity binding should be a tuple");
    assert_eq!(binding.len(), 3);
    assert_eq!(
        binding[0],
        json!(LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER.as_bytes())
    );
    assert_eq!(
        binding[1],
        json!(account_to_genesis_ss58(&foundation_genesis_wallet()))
    );
    let frg_cid_number = CHINA_ZF
        .iter()
        .find(|institution| institution_code_from_cid_number(institution.cid_number) == Some(FRG))
        .expect("CHINA_ZF must contain FRG")
        .cid_number;
    assert_eq!(binding[2], json!(frg_cid_number.as_bytes()));
}
