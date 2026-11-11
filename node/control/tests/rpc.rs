use super::*;

#[test]
fn chain_versions_keep_zero_and_validate_each_field_independently() {
    assert_eq!(
        parse_runtime_versions(&serde_json::json!({"specVersion": 7, "transactionVersion": 0})),
        (Some(7), Some(0))
    );
    assert_eq!(
        parse_runtime_versions(&serde_json::json!({"specVersion": -1, "transactionVersion": 5})),
        (None, Some(5))
    );
    assert_eq!(
        parse_runtime_versions(&serde_json::json!({
            "specVersion": 7, "transactionVersion": u64::from(u32::MAX) + 1,
        })),
        (Some(7), None)
    );
    assert_eq!(parse_runtime_versions(&serde_json::json!({})), (None, None));
}

#[test]
fn header_height_requires_hex_string_and_checks_overflow() {
    assert_eq!(
        header_block_height(&serde_json::json!({"number":"0xff"})),
        Some(255)
    );
    for value in [
        serde_json::json!({}),
        serde_json::json!({"number":255}),
        serde_json::json!({"number":"ff"}),
        serde_json::json!({"number":"0x10000000000000000"}),
    ] {
        assert_eq!(header_block_height(&value), None);
    }
}

#[test]
fn balances_keep_maximum_u128_and_reject_short_malformed_hex() {
    assert_eq!(
        scale_u128_from_storage_hex(&format!("0x{}", hex::encode(u128::MAX.to_le_bytes()))),
        Some(u128::MAX)
    );
    for value in ["0x0", "0xgg", "", "0x0000"] {
        assert_eq!(scale_u128_from_storage_hex(value), None);
    }
    let mut account = vec![0; 16];
    account.extend(111u128.to_le_bytes());
    assert_eq!(
        extract_free_balance(&format!("0x{}", hex::encode(account))),
        Some(111)
    );
    assert_eq!(format_fen_to_yuan(111), "1.11");
    assert_eq!(format_fen_to_yuan(123456789), "1,234,567.89");
}

#[test]
fn hex_parser_rejects_invalid_unicode_and_non_hex_without_panicking() {
    for value in [
        "中a",
        "0x中a",
        "é",
        "😀",
        "a中",
        "e\u{301}a",
        "0x0",
        "gg",
        "+a",
        "0Xff",
    ] {
        assert_eq!(hex_to_bytes(value), None, "invalid input: {value}");
        assert_eq!(scale_u128_from_storage_hex(value), None);
        assert_eq!(extract_free_balance(value), None);
    }
}

#[test]
fn hex_parser_preserves_empty_prefix_case_and_exact_bytes() {
    for value in ["", "0x"] {
        assert_eq!(hex_to_bytes(value), Some(Vec::new()));
    }
    for value in ["00aAfF", "0x00AAff"] {
        assert_eq!(hex_to_bytes(value), Some(vec![0, 170, 255]));
    }
    let value = format!("0x{}", "ff".repeat(32));
    assert_eq!(extract_free_balance(&value), Some(u128::MAX));
}

const FROZEN_TEST_HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Default)]
struct FrozenFixture {
    values: BTreeMap<String, String>,
    next_account: u32,
}

fn city_cid(serial: u32) -> String {
    let checksum =
        primitives::cid::number::checksum_char_m1(&format!("NX013CREG{serial:09}2026"), false);
    format!("NX013-CREG{checksum}-{serial:09}-2026")
}

impl FrozenFixture {
    fn add(
        &mut self,
        cid: &str,
        name: &[u8],
        progress: Option<(u64, Option<u32>)>,
        free: Option<u128>,
    ) -> (String, String, String) {
        self.next_account += 1;
        let mut account = [0u8; 32];
        account[..4].copy_from_slice(&self.next_account.to_le_bytes());
        let account_key =
            nodeclient::query::map_key("PublicManage", "AccountRegisteredCid", &account);
        let progress_key = nodeclient::query::map_key(
            "CitizenIdentity",
            "CityRegistryProgress",
            &cid.as_bytes().to_vec().encode(),
        );
        let balance_key = nodeclient::query::map_key("System", "Account", &account);
        let registered = RegisteredInstitution {
            cid_number: cid.as_bytes().to_vec(),
            account_name: name.to_vec(),
        };
        self.values.insert(
            account_key.clone(),
            format!("0x{}", hex::encode(registered.encode())),
        );
        if let Some(progress) = progress {
            self.values.insert(
                progress_key.clone(),
                format!("0x{}", hex::encode(progress.encode())),
            );
        }
        if let Some(free) = free {
            let mut balance = vec![0u8; 16];
            balance.extend(free.to_le_bytes());
            balance.extend([0u8; 48]); // reserved / frozen / flags 均为零，统计只使用实际 free。
            self.values
                .insert(balance_key.clone(), format!("0x{}", hex::encode(balance)));
        }
        (account_key, progress_key, balance_key)
    }

    fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "state_getKeysPaged" => {
                assert_eq!(params[3], FROZEN_TEST_HASH);
                let prefix = params[0].as_str().unwrap();
                let start = params[2].as_str();
                let count = params[1].as_u64().unwrap() as usize;
                Ok(serde_json::json!(self
                    .values
                    .keys()
                    .filter(|key| key.starts_with(prefix)
                        && start.is_none_or(|start| key.as_str() > start))
                    .take(count)
                    .collect::<Vec<_>>()))
            }
            "state_queryStorageAt" => {
                assert_eq!(params[1], FROZEN_TEST_HASH);
                let changes: Vec<_> = params[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|key| serde_json::json!([key, self.values.get(key.as_str().unwrap())]))
                    .collect();
                Ok(serde_json::json!([{"block": FROZEN_TEST_HASH, "changes": changes}]))
            }
            _ => panic!("unexpected RPC {method}"),
        }
    }

    fn total(&self) -> Result<u128, String> {
        total_frozen_at(FROZEN_TEST_HASH, &|method, params| self.rpc(method, params))
    }
}

#[test]
fn frozen_total_counts_only_unqualified_city_main_accounts_and_excludes_them_after_qualification() {
    use primitives::account_derive::{RESERVED_NAME_FEE, RESERVED_NAME_MAIN};
    let mut fixture = FrozenFixture::default();
    let cid = city_cid(1);
    let (_, progress, _) =
        fixture.add(&cid, RESERVED_NAME_MAIN, Some((1_000, None)), Some(123_456));
    fixture.add(&cid, RESERVED_NAME_FEE, None, Some(999_999));
    fixture.add(&cid, "其他账户".as_bytes(), None, Some(999_999));
    fixture.add(
        primitives::cid::china::china_cb::CHINA_CB[0].cid_number,
        RESERVED_NAME_MAIN,
        None,
        Some(999_999),
    );
    fixture.add(
        &city_cid(2),
        RESERVED_NAME_MAIN,
        Some((10_001, Some(101))),
        Some(999_999),
    );
    fixture.add(&city_cid(3), RESERVED_NAME_MAIN, None, Some(111));
    fixture.add(&city_cid(4), RESERVED_NAME_MAIN, None, None);
    assert_eq!(fixture.total().unwrap(), 123_567);
    fixture.values.insert(
        progress,
        format!("0x{}", hex::encode((1_001u64, Some(1u32)).encode())),
    );
    assert_eq!(fixture.total().unwrap(), 111);
}

#[test]
fn frozen_total_scans_every_page_at_one_finalized_snapshot() {
    let mut fixture = FrozenFixture::default();
    for serial in 1..=257 {
        fixture.add(
            &city_cid(serial),
            primitives::account_derive::RESERVED_NAME_MAIN,
            None,
            Some(111),
        );
    }
    let pages = std::cell::Cell::new(0);
    let total = total_frozen_at(FROZEN_TEST_HASH, &|method, params| {
        if method == "state_getKeysPaged" {
            pages.set(pages.get() + 1);
        }
        fixture.rpc(method, params)
    })
    .unwrap();
    assert_eq!(pages.get(), 2);
    assert_eq!(total, 257 * 111);
    assert_eq!(FrozenFixture::default().total().unwrap(), 0);
}

#[test]
fn frozen_total_rejects_partial_results_wrong_blocks_and_malformed_storage() {
    let mut fixture = FrozenFixture::default();
    let (account_key, progress_key, balance_key) = fixture.add(
        &city_cid(1),
        primitives::account_derive::RESERVED_NAME_MAIN,
        None,
        Some(111),
    );
    for fault in [
        "rpc",
        "missing",
        "duplicate",
        "unexpected",
        "block",
        "value",
    ] {
        let result = total_frozen_at(FROZEN_TEST_HASH, &|method, params| {
            if method != "state_queryStorageAt" {
                return fixture.rpc(method, params);
            }
            if fault == "rpc" {
                return Err("测试查询中断".into());
            }
            let mut response = fixture.rpc(method, params)?;
            match fault {
                "missing" => {
                    response[0]["changes"].as_array_mut().unwrap().clear();
                }
                "duplicate" => {
                    let duplicate = response[0]["changes"][0].clone();
                    response[0]["changes"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
                "unexpected" => {
                    response[0]["changes"][0][0] = Value::String("0xunexpected".into());
                }
                "block" => {
                    response[0]["block"] = Value::String("0xother".into());
                }
                "value" => {
                    response[0]["changes"][0][1] = serde_json::json!(12);
                }
                _ => unreachable!(),
            }
            Ok(response)
        });
        assert!(result.is_err(), "{fault} must not publish a partial total");
    }
    for key in [account_key, progress_key, balance_key] {
        let previous = fixture.values.insert(key.clone(), "0x01".into());
        assert!(fixture.total().is_err());
        fixture.values.remove(&key);
        if let Some(value) = previous {
            fixture.values.insert(key, value);
        }
    }
    let repeated = total_frozen_at(FROZEN_TEST_HASH, &|method, params| {
        if method == "state_getKeysPaged" {
            let key = fixture
                .values
                .keys()
                .find(|key| key.starts_with(params[0].as_str().unwrap()))
                .unwrap();
            return Ok(serde_json::json!([key, key]));
        }
        fixture.rpc(method, params)
    });
    assert!(repeated.is_err());
}

#[test]
fn frozen_total_keeps_u128_precision_and_rejects_sum_overflow() {
    let mut fixture = FrozenFixture::default();
    fixture.add(
        &city_cid(1),
        primitives::account_derive::RESERVED_NAME_MAIN,
        None,
        Some(u128::MAX),
    );
    assert_eq!(fixture.total().unwrap(), u128::MAX);
    fixture.add(
        &city_cid(2),
        primitives::account_derive::RESERVED_NAME_MAIN,
        None,
        Some(1),
    );
    assert!(fixture.total().unwrap_err().contains("溢出"));
}

#[test]
fn frozen_cache_reuses_only_successful_results_for_the_exact_finalized_hash() {
    let cache = Mutex::new(None);
    assert_eq!(cached_total_frozen(&cache, "a", || Ok(111)).unwrap(), 111);
    assert_eq!(
        cached_total_frozen(&cache, "a", || panic!("cached block must not rescan")).unwrap(),
        111
    );
    assert!(cached_total_frozen(&cache, "b", || Err("查询不完整".into())).is_err());
    assert_eq!(cached_total_frozen(&cache, "b", || Ok(0)).unwrap(), 0);
    assert_eq!(cached_total_frozen(&cache, "a", || Ok(222)).unwrap(), 222);
}
