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

//! 构造创世配置及预设，将账户和GRANDPA密钥转换为创世地址。

use alloc::vec;
use alloc::vec::Vec;
use frame_support::traits::BuildGenesisConfig;
use sp_genesis_builder::PresetId;

#[cfg(feature = "std")]
use codec::{DecodeAll, Encode};
#[cfg(feature = "std")]
use primitives::{
    cid::china::china_cb::CHINA_CB,
    core_const::SS58_FORMAT,
    genesis::{CITIZENS, COUNTRY, GENESIS_CITIZEN_MAX},
};
#[cfg(feature = "std")]
use sc_chain_spec::{ChainSpec as _, GenericChainSpec, NoExtension};
#[cfg(feature = "std")]
use serde_json::{json, Value};
#[cfg(feature = "std")]
use sp_core::crypto::{Ss58AddressFormat, Ss58Codec};

/// 冻结链规是历史拨付、初始绑定和公钥的唯一宿主资料来源。
#[cfg(feature = "std")]
pub fn frozen_chain_spec() -> &'static [u8] {
    include_bytes!("../../../node/citizenchain.json")
}

#[cfg(feature = "std")]
struct FrozenInputs {
    patch: Value,
    registered_keys: Vec<(Vec<u8>, [u8; 32])>,
}

#[cfg(feature = "std")]
fn frozen_inputs() -> &'static FrozenInputs {
    static DATA: std::sync::OnceLock<FrozenInputs> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let document: Value =
            serde_json::from_slice(frozen_chain_spec()).expect("冻结链规 JSON 可解析");
        let patch = document
            .pointer("/genesis/runtimeGenesis/patch")
            .expect("冻结链规包含创世配置")
            .clone();
        // 使用冻结文件内原始 WASM 的存储读取，与节点加载同一份历史资料。
        let spec = GenericChainSpec::<NoExtension>::from_json_bytes(frozen_chain_spec())
            .expect("冻结链规可解析");
        let storage = spec
            .as_storage_builder()
            .build_storage()
            .expect("读取冻结原始状态");
        let current =
            frame_support::storage::storage_prefix(b"GrandpaKeyChange", b"CurrentGrandpaKeys");
        let reverse =
            frame_support::storage::storage_prefix(b"GrandpaKeyChange", b"GrandpaKeyOwnerByKey");
        let mut registered_keys = Vec::with_capacity(CHINA_CB.len());
        for entry in CHINA_CB {
            let cid = entry.cid_number.as_bytes().to_vec();
            let encoded_cid = cid.encode();
            let mut key = current.to_vec();
            key.extend_from_slice(&sp_io::hashing::blake2_128(&encoded_cid));
            key.extend_from_slice(&encoded_cid);
            let value = storage.top.get(&key).expect("冻结机构公钥索引完整");
            let public =
                <[u8; 32]>::decode_all(&mut value.as_slice()).expect("冻结公钥 SCALE 正确");
            let mut owner = reverse.to_vec();
            owner.extend_from_slice(&sp_io::hashing::blake2_128(&public));
            owner.extend_from_slice(&public);
            assert_eq!(
                storage.top.get(&owner),
                Some(&encoded_cid),
                "冻结公钥反向索引一致"
            );
            registered_keys.push((cid, public));
        }
        FrozenInputs {
            patch,
            registered_keys,
        }
    })
}

/// 组装宿主预设。历史分配及身份保留冻结值，候选元数据使用现行注册名。
#[cfg(feature = "std")]
pub fn genesis_config<G: BuildGenesisConfig + Default>() -> Value {
    let frozen = frozen_inputs();
    let allowed_recipients: Vec<_> = CHINA_CB
        .iter()
        .skip(1)
        .map(|entry| {
            sp_runtime::AccountId32::new(entry.main_account)
                .to_ss58check_with_version(Ss58AddressFormat::custom(SS58_FORMAT))
        })
        .collect();
    let mut genesis = serde_json::to_value(G::default()).expect("汇总预设可序列化");
    let root = genesis.as_object_mut().expect("汇总预设是 JSON 对象");
    for (target, source) in [
        ("balances", "balances"),
        ("grandpa", "grandpa"),
        ("identity", "citizenIdentity"),
    ] {
        let value = frozen.patch.get(source).expect("冻结预设字段完整");
        root.insert(target.into(), value.clone());
    }
    root.get_mut("grandpaKey")
        .and_then(Value::as_object_mut)
        .expect("现行预设包含 GRANDPA 公钥登记")
        .insert("registeredKeys".into(), json!(frozen.registered_keys));
    root.insert(
        "resolutionIssue".into(),
        json!({ "allowedRecipients": allowed_recipients }),
    );
    root.insert(
        "genesis".into(),
        json!({
            "citizensDeclaration": CITIZENS.as_bytes(),
            "countryDeclaration": COUNTRY.as_bytes(),
            "citizenMax": GENESIS_CITIZEN_MAX,
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
