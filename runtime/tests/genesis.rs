//! 主Runtime实际类型装配下的独立测试域。

// 实际Runtime API必须与genesis公开入口保持同一预设字节；不引入第二个构造器。
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
        CITIZENCHAIN_GENESIS_ADMINS, LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER,
    },
    cid::code::{institution_code_from_cid_number, FRG},
    core_const::SS58_FORMAT,
    genesis::{GENESIS_ISSUANCE, HE_FUND_ISSUANCE},
};
use serde_json::{json, Value};
use sp_core::crypto::{Ss58AddressFormat, Ss58Codec};
use std::collections::BTreeSet;

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

    // 创世包含 1 个国家储委会多签账户 + 19 个 NRC 管理员 + 43 个省储行 stake 质押地址
    // + 1 个国家储委会两和基金账户 + 6 笔创世分账账户。
    let nrc_admins_len = CHINA_CB.first().map(|n| n.admins.len()).unwrap_or(0);
    assert_eq!(balances.len(), 1 + nrc_admins_len + CHINA_CH.len() + 1 + 6);

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

    // 创世发行分配到国家储委会多签账户 = 总发行量 - NRC 管理员预置总额 - 六账户分账总额(700 亿元)。
    let admin_each: u128 = 1_000_000_000;
    let nrc_admins_len = CHINA_CB
        .first()
        .map(|n| n.admins.len() as u128)
        .unwrap_or(0);
    let allocated_total: u128 = 7_000_000_000_000; // 700 亿元 = 6 笔创世分账合计
    let expected_nrc = GENESIS_ISSUANCE - admin_each * nrc_admins_len - allocated_total;
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
    use primitives::cid::china::citizenchain::{
        CITIZENCHAIN_FOUNDATION, CITIZENCHAIN_GENESIS_ADMINS,
    };
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
        amount_of(&AccountId::new(CITIZENCHAIN_GENESIS_ADMINS[0].account_id)),
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
fn china_cb_grandpa_keys_are_valid_unique_ed25519_pubkeys() {
    let mut uniq = BTreeSet::new();
    for node in CHINA_CB {
        VerifyingKey::from_bytes(&node.grandpa_key)
            .expect("CHINA_CB.grandpa_key must be valid ed25519 point");
        assert!(
            uniq.insert(node.grandpa_key),
            "CHINA_CB.grandpa_key must be unique"
        );
    }
    assert_eq!(uniq.len(), 44, "must contain exactly 44 grandpa keys");
}

#[test]
fn governance_admin_keys_unique_and_counts_match_seats() {
    // 治理机构管理员公钥不变量:各机构数量必须匹配岗位席位,且全部公钥全局唯一。
    // 每把 32 字节的长度合法性已由 `hex!` 宏 + `[u8; 32]` 类型在编译期强制,
    // 编译器抓不到的是「数量不符」和「公钥重复」,故在此显式断言(替代脆弱的正则脚本)。
    use primitives::cid::china::china_ch::CHINA_CH;
    use primitives::cid::china::china_sf::NATIONAL_JUDICIAL_YUAN_ADMINS;
    use primitives::cid::china::china_zf::FEDERAL_REGISTRY_ADMINS;
    use primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ADMINS;
    use primitives::cid::code::PROVINCE_CODE_INFOS;
    use primitives::count_const::{
        FRG_PROVINCE_GROUP_ADMIN_COUNT, NJD_ADMIN_COUNT, NRC_ADMIN_COUNT, PRB_ADMIN_COUNT,
        PRC_ADMIN_COUNT,
    };

    let mut all: Vec<[u8; 32]> = Vec::new();

    // 国家储委会(NRC=第 1 个)19 席 + 43 个省储委会(PRC)各 9 席。
    for (i, cb) in CHINA_CB.iter().enumerate() {
        let expected = if i == 0 {
            NRC_ADMIN_COUNT as usize
        } else {
            PRC_ADMIN_COUNT as usize
        };
        assert_eq!(
            cb.admins.len(),
            expected,
            "储委会 {} 管理员数量与岗位席位不符",
            cb.cid_number
        );
        all.extend_from_slice(cb.admins);
    }

    // 43 个省储行(PRB)各 9 席。
    for ch in CHINA_CH {
        assert_eq!(
            ch.admins.len(),
            PRB_ADMIN_COUNT as usize,
            "省储行 {} 管理员数量与岗位席位不符",
            ch.cid_number
        );
        all.extend_from_slice(ch.admins);
    }

    // 国家司法院(NJD)15 席。
    assert_eq!(
        NATIONAL_JUDICIAL_YUAN_ADMINS.len(),
        NJD_ADMIN_COUNT as usize,
        "国家司法院管理员数量与岗位席位不符"
    );
    all.extend_from_slice(NATIONAL_JUDICIAL_YUAN_ADMINS);

    // 联邦注册局(FRG)= 省数 × 每省组人数。
    assert_eq!(
        FEDERAL_REGISTRY_ADMINS.len(),
        PROVINCE_CODE_INFOS.len() * FRG_PROVINCE_GROUP_ADMIN_COUNT as usize,
        "联邦注册局管理员数量与岗位席位不符"
    );
    all.extend_from_slice(FEDERAL_REGISTRY_ADMINS);

    // 公民链技术发展基金会创世管理员(程伟,1 名)。
    assert_eq!(
        CITIZENCHAIN_GENESIS_ADMINS.len(),
        1,
        "基金会创世管理员数量应为 1"
    );
    all.extend(
        CITIZENCHAIN_GENESIS_ADMINS
            .iter()
            .map(|admin| admin.account_id),
    );

    // 全局唯一:去重后数量必须与原始数量相等。
    let unique: BTreeSet<[u8; 32]> = all.iter().copied().collect();
    assert_eq!(unique.len(), all.len(), "存在重复的治理机构管理员公钥");
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

    for account in patch["resolutionIssuance"]["allowedRecipients"]
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

    let binding = patch["citizenIdentity"]["initialCidBindings"][0]
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
        serde_json::from_value(patch["resolutionIssuance"].clone());
    assert!(
        resolutionissue.is_ok(),
        "resolutionIssuance should deserialize: {:?}",
        resolutionissue.err()
    );

    let identity: Result<identity::GenesisConfig<crate::Runtime>, _> =
        serde_json::from_value(patch["citizenIdentity"].clone());
    assert!(
        identity.is_ok(),
        "citizenIdentity should deserialize: {:?}",
        identity.err()
    );
}

#[test]
fn genesis_contains_fixed_admin_citizen_identity_binding() {
    let patch = ::genesis::config::genesis_config::<RuntimeGenesisConfig>();
    let bindings = patch["citizenIdentity"]["initialCidBindings"]
        .as_array()
        .expect("citizen identity bindings should be an array");
    assert_eq!(bindings.len(), 1, "创世只预置固定基金会管理员身份");

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
        json!(account_to_genesis_ss58(&AccountId::new(
            CITIZENCHAIN_GENESIS_ADMINS[0].account_id
        )))
    );
    let frg_cid_number = CHINA_ZF
        .iter()
        .find(|institution| institution_code_from_cid_number(institution.cid_number) == Some(FRG))
        .expect("CHINA_ZF must contain FRG")
        .cid_number;
    assert_eq!(binding[2], json!(frg_cid_number.as_bytes()));
}
