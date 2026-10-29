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
use frame_support::traits::BuildGenesisConfig;

#[cfg(feature = "std")]
use sp_runtime::AccountId32 as AccountId;
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

/// 构造整链创世配置；调用方传入实际汇总类型，genesis 不反向依赖主 Runtime。
/// 默认配置、固定分配与初始化内容沿用原实现，禁止据此替换冻结链规。
#[cfg(feature = "std")]
pub fn genesis_config<G: BuildGenesisConfig + Default>() -> Value {
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

    let mut genesis = serde_json::to_value(G::default())
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

    // 创世常量写入 genesis 链上存储。
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
    // 必须同时写入 identity 的 Active 登记、CID↔AccountId 双向绑定及
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

/// 返回指定预设的 JSON 字节；未知预设及 no_std 模式保持返回 None。
pub fn get_preset<G: BuildGenesisConfig + Default>(id: &PresetId) -> Option<Vec<u8>> {
    #[cfg(not(feature = "std"))]
    {
        let _ = id;
        return None;
    }

    #[cfg(feature = "std")]
    {
        let patch = match id.as_ref() {
            sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => genesis_config::<G>(),
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

/// 列出支持的创世预设。
pub fn preset_names() -> Vec<PresetId> {
    vec![PresetId::from(
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET,
    )]
}
