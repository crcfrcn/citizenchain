// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloc::vec;
use alloc::vec::Vec;
use sp_genesis_builder::PresetId;

#[cfg(feature = "std")]
use crate::AccountId;
#[cfg(feature = "std")]
use codec::Decode;
#[cfg(feature = "std")]
use primitives::{
    account_derive::AccountKind,
    cid::china::china_cb::{CHINA_CB, NRC_HE_ACCOUNT, SAFETY_FUND_ACCOUNT},
    cid::china::china_ch::CHINA_CH,
    cid::china::china_sf::CHINA_SF,
    cid::china::china_zf::CHINA_ZF,
    cid::china::citizenchain::{
        CITIZENCHAIN_FOUNDATION, CITIZENCHAIN_GENESIS_ADMINS,
        LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER,
    },
    cid::code::{institution_code_from_cid_number, FRG, FSC},
    core_const::SS58_FORMAT,
    genesis::{CITIZENS, COUNTRY, GENESIS_CITIZEN_MAX, GENESIS_ISSUANCE, HE_FUND_ISSUANCE},
};
#[cfg(feature = "std")]
use serde_json::{json, Value};
#[cfg(feature = "std")]
use sp_core::crypto::{Ss58AddressFormat, Ss58Codec};
#[cfg(feature = "std")]
use sp_core::ByteArray;
#[cfg(feature = "std")]
use sp_genesis_builder::{self};

#[cfg(feature = "std")]
fn account_to_genesis_ss58(account: &AccountId) -> String {
    // 创世配置地址使用链统一 SS58 前缀（2027）。
    account.to_ss58check_with_version(Ss58AddressFormat::custom(SS58_FORMAT))
}

#[cfg(feature = "std")]
fn grandpa_key_to_genesis_ss58(key: &[u8; 32]) -> String {
    let authority = sp_consensus_grandpa::AuthorityId::from_slice(key).unwrap_or_else(|error| {
        panic!("grandpa authority id must decode from 32 bytes: {error:?}")
    });
    authority.to_ss58check_with_version(Ss58AddressFormat::custom(SS58_FORMAT))
}

#[cfg(all(feature = "std", test))]
fn json_amount_to_u128(v: &Value) -> Option<u128> {
    if let Some(value) = v.as_u64() {
        return Some(value as u128);
    }
    v.as_str().and_then(|s| s.parse::<u128>().ok())
}

// Returns the genesis config presets.
#[cfg(feature = "std")]
fn build_genesis() -> Value {
    // 国家储委会信息统一从常量数组入口读取。
    let nrc_account = CHINA_CB
        .first()
        .and_then(|n| AccountId::decode(&mut &n.main_account[..]).ok())
        .unwrap_or_else(|| panic!("NRC main_account must decode to AccountId"));

    // 每位国家储委会管理员创世预置 1000 万元（单位：分）。
    let admin_each: u128 = 1_000_000_000; // 1000万元 = 10亿分
    let nrc_admins = &CHINA_CB
        .first()
        .unwrap_or_else(|| panic!("CHINA_CB must have NRC entry"))
        .admins;
    let admin_total: u128 = admin_each * nrc_admins.len() as u128;

    // ── 创世分账:从创世发行中切出 700 亿元预置给 6 个指定账户 ──
    // 单位:分(1 元 = 100 分)。这 700 亿元从国家储委会主账户份额中切出,
    // 创世发行总量 GENESIS_ISSUANCE 保持不变。金额集中定义于此,便于审计。
    const ALLOC_FCSF: u128 = 1_000_000_000_000; // 联邦公民安全基金 100 亿元
    const ALLOC_CHENGWEI: u128 = 2_000_000_000_000; // 公民程伟钱包 200 亿元
    const ALLOC_FOUNDATION: u128 = 1_000_000_000_000; // 技术发展基金会主账户 100 亿元
    const ALLOC_FRG: u128 = 1_000_000_000_000; // 联邦注册局主账户 100 亿元
    const ALLOC_NJD: u128 = 1_000_000_000_000; // 国家司法院主账户 100 亿元
    const ALLOC_NRC_SAFETY: u128 = 1_000_000_000_000; // 国储会安全基金 100 亿元
    let allocated_total: u128 =
        ALLOC_FCSF + ALLOC_CHENGWEI + ALLOC_FOUNDATION + ALLOC_FRG + ALLOC_NJD + ALLOC_NRC_SAFETY;

    // 联邦公民安全基金地址必须与创世 seeder(FSC 特判)登记的派生地址完全一致,
    // 走账户派生唯一真源:OP_FCSF + 联邦安全局(FSC)的 CID,禁止硬抄地址。
    let fsc_cid_number = CHINA_ZF
        .iter()
        .find(|inst| institution_code_from_cid_number(inst.cid_number) == Some(FSC))
        .unwrap_or_else(|| panic!("CHINA_ZF must contain the Federal Security Bureau (FSC)"))
        .cid_number;
    let fcsf_account = AccountId::new(
        AccountKind::InstitutionFederalCitizenSecurityFund {
            cid_number: fsc_cid_number.as_bytes(),
        }
        .derive(SS58_FORMAT),
    );

    // 联邦注册局(FRG)主账户与国家司法院(CHINA_SF 首个 = 国家司法院)主账户。
    let frg_institution = CHINA_ZF
        .iter()
        .find(|inst| institution_code_from_cid_number(inst.cid_number) == Some(FRG))
        .unwrap_or_else(|| panic!("CHINA_ZF must contain the Federal Registry Bureau (FRG)"));
    let frg_main_account = frg_institution.main_account;
    let njd_main_account = CHINA_SF
        .first()
        .unwrap_or_else(|| panic!("CHINA_SF must contain the National Judicial Yuan"))
        .main_account;

    // 国家储委会多签账户 = 创世发行总量 - 管理员预置总额 - 分账总额，创世发行总量不变。
    let mut genesis_balances: Vec<(AccountId, u128)> = vec![(
        nrc_account.clone(),
        GENESIS_ISSUANCE - admin_total - allocated_total,
    )];

    // 19 位管理员各自获得创世余额。
    genesis_balances.extend(nrc_admins.iter().map(|key| {
        let account = AccountId::new(*key);
        (account, admin_each)
    }));

    // 省储行创立发行在创世时直接预置到各自 stake_account（无私钥永久质押地址）。
    genesis_balances.extend(
        CHINA_CH
            .iter()
            .map(|bank| (AccountId::new(bank.stake_account), bank.stake_amount)),
    );

    // 两和基金创世一次性发行到国家储委会两和基金账户（无私钥派生地址 NRC_HE_ACCOUNT），
    // 作为独立增发计入总供应量，国家储委会通过内部投票管理该基金。
    genesis_balances.push((AccountId::new(NRC_HE_ACCOUNT), HE_FUND_ISSUANCE));

    // ── 6 笔创世分账预置(总额 700 亿元已从上面 NRC 主账户份额中扣除) ──
    // 联邦公民安全基金(联邦安全局 FSC 专属,派生地址与 seeder 登记一致)。
    genesis_balances.push((fcsf_account, ALLOC_FCSF));
    // 公民程伟钱包(基金会创世管理员账户)。
    genesis_balances.push((
        AccountId::new(CITIZENCHAIN_GENESIS_ADMINS[0].account_id),
        ALLOC_CHENGWEI,
    ));
    // 公民链技术发展基金会主账户。
    genesis_balances.push((
        AccountId::new(CITIZENCHAIN_FOUNDATION.main_account),
        ALLOC_FOUNDATION,
    ));
    // 联邦注册局主账户。
    genesis_balances.push((AccountId::new(frg_main_account), ALLOC_FRG));
    // 国家司法院主账户。
    genesis_balances.push((AccountId::new(njd_main_account), ALLOC_NJD));
    // 国家储委会安全基金账户。
    genesis_balances.push((AccountId::new(SAFETY_FUND_ACCOUNT), ALLOC_NRC_SAFETY));

    // 创世账户统一输出为链 SS58 地址（前缀 2027）。
    let balances_json: Vec<Value> = genesis_balances
        .into_iter()
        .map(|(account, amount)| {
            let account_ss58 = account_to_genesis_ss58(&account);
            json!([account_ss58, amount])
        })
        .collect();

    // 决议发行合法收款账户改为链上存储初始化，后续可由治理动态更新。
    let issuance_allowed_recipients_json: Vec<Value> = CHINA_CB
        .iter()
        .skip(1)
        .map(|n| {
            let account = AccountId::decode(&mut &n.main_account[..]).unwrap_or_else(|error| {
                panic!("PRC main_account must decode to AccountId: {error:?}")
            });
            Value::String(account_to_genesis_ss58(&account))
        })
        .collect();

    // 正式链开发期 GRANDPA 只使用国家储委会（NRC）的第 1 把密钥，单节点即可 finalize。
    // 切换到运行期时通过 SwitchToProduction migration 扩展到全部 44 个权威。
    let grandpa_authorities_json: Vec<Value> = vec![json!([
        grandpa_key_to_genesis_ss58(&CHINA_CB[0].grandpa_key),
        1
    ])];

    let mut genesis = serde_json::to_value(crate::RuntimeGenesisConfig::default())
        .unwrap_or_else(|error| panic!("default runtime genesis config should serialize: {error}"));

    let root = genesis
        .as_object_mut()
        .unwrap_or_else(|| panic!("runtime genesis config should serialize to a JSON object"));

    root.insert(
        "balances".into(),
        json!({
            "balances": balances_json,
        }),
    );
    root.insert(
        "grandpa".into(),
        json!({
            "authorities": grandpa_authorities_json,
        }),
    );
    root.insert(
        "resolutionIssuance".into(),
        json!({
            "allowedRecipients": issuance_allowed_recipients_json,
        }),
    );

    // 创世常量写入 genesis-pallet 链上存储。
    let citizens_bytes: Vec<u8> = CITIZENS.as_bytes().to_vec();
    let country_bytes: Vec<u8> = COUNTRY.as_bytes().to_vec();
    root.insert(
        "genesisPallet".into(),
        json!({
            "citizensDeclaration": citizens_bytes,
            "countryDeclaration": country_bytes,
            "citizenMax": GENESIS_CITIZEN_MAX,
        }),
    );

    // 基金会创世管理员的永久公民 CID 已被公权管理员和法定代表人记录引用，
    // 必须同时写入 citizen-identity 的 Active 登记、CID↔AccountId 双向绑定及
    // BindingRevisionByCid=1。
    // 登记来源使用既有联邦注册局机构 CID；这里只建立匿名身份闭环，不伪造投票/
    // 竞选身份。CID 是永久主键，账户只是可依法换绑的授权凭证，禁止按当前账户重派 CID。
    let genesis_admin_account = AccountId::new(CITIZENCHAIN_GENESIS_ADMINS[0].account_id);
    root.insert(
        "citizenIdentity".into(),
        json!({
            "initialCidBindings": [[
                LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER.as_bytes(),
                account_to_genesis_ss58(&genesis_admin_account),
                frg_institution.cid_number.as_bytes(),
            ]],
        }),
    );

    genesis
}

/// 返回 citizenchain 创世配置。
#[cfg(feature = "std")]
pub fn genesis_config() -> Value {
    build_genesis()
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    #[cfg(not(feature = "std"))]
    {
        let _ = id;
        return None;
    }

    #[cfg(feature = "std")]
    {
        let patch = match id.as_ref() {
            sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => genesis_config(),
            _ => return None,
        };
        Some(
            serde_json::to_string(&patch)
                .unwrap_or_else(|error| {
                    panic!("serialization to json is expected to work: {error}")
                })
                .into_bytes(),
        )
    }
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
    vec![PresetId::from(
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET,
    )]
}

#[cfg(all(test, feature = "std"))]
// 创世夹具异常必须立即中止测试，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::RuntimeGenesisConfig;
    use ed25519_dalek::VerifyingKey;
    use primitives::cid::china::china_cb::CHINA_CB;
    use std::collections::BTreeSet;

    #[test]
    fn genesis_contains_nrc_and_all_provincialbank_balances() {
        let patch = genesis_config();
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
        let patch = genesis_config();
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

        let patch = genesis_config();
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

        // 联邦公民安全基金:派生地址必须与 build_genesis / seeder 完全一致。
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
        let patch = genesis_config();
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
        let patch = genesis_config();
        let parsed: Result<RuntimeGenesisConfig, _> = serde_json::from_value(patch);
        assert!(
            parsed.is_ok(),
            "runtime genesis json should deserialize: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn genesis_account_strings_deserialize_individually() {
        let patch = genesis_config();

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
        let patch = genesis_config();

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

        let resolution_issuance: Result<resolution_issuance::GenesisConfig<crate::Runtime>, _> =
            serde_json::from_value(patch["resolutionIssuance"].clone());
        assert!(
            resolution_issuance.is_ok(),
            "resolutionIssuance should deserialize: {:?}",
            resolution_issuance.err()
        );

        let citizen_identity: Result<citizen_identity::GenesisConfig<crate::Runtime>, _> =
            serde_json::from_value(patch["citizenIdentity"].clone());
        assert!(
            citizen_identity.is_ok(),
            "citizenIdentity should deserialize: {:?}",
            citizen_identity.err()
        );
    }

    #[test]
    fn genesis_contains_fixed_admin_citizen_identity_binding() {
        let patch = genesis_config();
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
            .find(|institution| {
                institution_code_from_cid_number(institution.cid_number) == Some(FRG)
            })
            .expect("CHINA_ZF must contain FRG")
            .cid_number;
        assert_eq!(binding[2], json!(frg_cid_number.as_bytes()));
    }
}
