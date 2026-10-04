//! 公民护照号与护照有效期工具。
//!
//! 本文件把归档 CPMS 的护照号算法复制到 OnChina,但不复制旧档案号。
//! 护照号终身唯一;资源回收只通过 `passport_number_recycle_pool` 提供的
//! 空闲护照号领取入口实现,不携带旧公民个人资料。

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

type Blake2b256 = Blake2b<U32>;

const MODULE_TAG: &[u8] = b"cid-cpms";
const PASSPORT_NO_MAX_RETRY: u32 = 1000;
const PASSPORT_BODY_SYMBOLS: usize = 8;
const PASSPORT_CITY_NAMESPACE_COUNT: u64 = 512;
const PASSPORT_BODY_SPACE: u64 = 1u64 << (PASSPORT_BODY_SYMBOLS * 5);
const PASSPORT_LOCAL_CAPACITY: u64 = PASSPORT_BODY_SPACE / PASSPORT_CITY_NAMESPACE_COUNT;
const PASSPORT_CITY_NAMESPACE_MULTIPLIER: u64 = 137;
const PASSPORT_SEQ_MULTIPLIER: u64 = 1_103_515_245;
const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub(crate) fn generate_passport_no_with_retry<C: postgres::GenericClient>(
    conn: &mut C,
    province_code: &str,
    city_code: &str,
    cid_number: &str,
) -> Result<String, String> {
    validate_passport_area_codes(province_code, city_code)?;
    if let Some(passport_no) = claim_recycled_passport_no(conn, cid_number)? {
        return Ok(passport_no);
    }

    let seq_key = format!(
        "passport_no:{}{}",
        province_code.trim().to_ascii_uppercase(),
        city_code.trim()
    );

    for _ in 0..PASSPORT_NO_MAX_RETRY {
        let seq = allocate_sequence(conn, &seq_key)?;
        let passport_no = build_passport_no(province_code, city_code, seq)?;
        if !passport_no_exists(conn, passport_no.as_str())? {
            return Ok(passport_no);
        }
    }

    Err("passport_no conflict, retry exhausted".to_string())
}

pub(crate) fn passport_validity_years(created_at: DateTime<Utc>, birth_date: NaiveDate) -> i32 {
    if age_at(created_at.date_naive(), birth_date) >= 16 {
        10
    } else {
        5
    }
}

pub(crate) fn passport_valid_from(created_at: DateTime<Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
}

pub(crate) fn passport_valid_until(created_at: DateTime<Utc>, years: i32) -> String {
    let date = created_at.date_naive();
    let target_year = date.year() + years;
    let anniversary = NaiveDate::from_ymd_opt(target_year, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(target_year, 2, 28))
        .unwrap_or(date);
    (anniversary - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string()
}

pub(crate) fn is_voting_age_at(today: NaiveDate, birth_date: NaiveDate) -> bool {
    age_at(today, birth_date) >= 16
}

fn age_at(today: NaiveDate, birth_date: NaiveDate) -> i32 {
    let mut age = today.year() - birth_date.year();
    if (today.month(), today.day()) < (birth_date.month(), birth_date.day()) {
        age -= 1;
    }
    age
}

fn validate_passport_area_codes(province_code: &str, city_code: &str) -> Result<(), String> {
    let province = province_code.trim();
    if province.len() != 2 || !province.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("invalid passport province_code".to_string());
    }
    passport_city_value(city_code).map(|_| ())
}

fn passport_city_namespace(province_code: &str, city_code: &str) -> Result<u64, String> {
    let city_value = passport_city_value(city_code)?;
    let offset = province_namespace_offset(province_code);
    // 用 0..511 置换把省内唯一 city_code 映射成城市隔离编号,
    // 护照号明文不直接暴露原始市代码。
    Ok((city_value * PASSPORT_CITY_NAMESPACE_MULTIPLIER + offset) % PASSPORT_CITY_NAMESPACE_COUNT)
}

fn passport_city_value(city_code: &str) -> Result<u64, String> {
    let city = city_code.trim();
    if city.len() != 3 || !city.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid passport city_code".to_string());
    }
    let city_value = city
        .parse::<u64>()
        .map_err(|_| "invalid passport city_code".to_string())?;
    if city_value == 0 || city_value >= PASSPORT_CITY_NAMESPACE_COUNT {
        return Err("invalid passport city_code".to_string());
    }
    Ok(city_value)
}

fn province_namespace_offset(province_code: &str) -> u64 {
    let mut hasher = Blake2b256::new();
    hasher.update(MODULE_TAG);
    hasher.update(b"|passport-city-offset|");
    hasher.update(province_code.trim().to_ascii_uppercase().as_bytes());
    let digest = hasher.finalize();
    u16::from_le_bytes([digest[0], digest[1]]) as u64 % PASSPORT_CITY_NAMESPACE_COUNT
}

fn scramble_passport_sequence(province_code: &str, city_code: &str, seq0: u64) -> u64 {
    let mut hasher = Blake2b256::new();
    hasher.update(MODULE_TAG);
    hasher.update(b"|passport-sequence-salt|");
    hasher.update(province_code.trim().to_ascii_uppercase().as_bytes());
    hasher.update(b"|");
    hasher.update(city_code.trim().as_bytes());
    let digest = hasher.finalize();
    let salt = u64::from_le_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]) & (PASSPORT_LOCAL_CAPACITY - 1);
    seq0.wrapping_mul(PASSPORT_SEQ_MULTIPLIER)
        .wrapping_add(salt)
        & (PASSPORT_LOCAL_CAPACITY - 1)
}

fn passport_check_char(source: &str) -> char {
    let mut hasher = Blake2b256::new();
    hasher.update(MODULE_TAG);
    hasher.update(b"|passport-no-check|");
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    CROCKFORD_ALPHABET[(digest[0] & 0x1f) as usize] as char
}

fn crockford_fixed(value: u64, width: usize) -> String {
    let mut out = String::with_capacity(width);
    for pos in (0..width).rev() {
        let idx = ((value >> (pos * 5)) & 0x1f) as usize;
        out.push(CROCKFORD_ALPHABET[idx] as char);
    }
    out
}

fn build_passport_no(
    province_code: &str,
    city_code: &str,
    sequence: i64,
) -> Result<String, String> {
    let province = province_code.trim().to_ascii_uppercase();
    let seq0 = u64::try_from(sequence.saturating_sub(1))
        .map_err(|_| "passport_no capacity exhausted".to_string())?;
    if seq0 >= PASSPORT_LOCAL_CAPACITY {
        return Err("passport_no capacity exhausted".to_string());
    }

    let namespace = passport_city_namespace(&province, city_code)?;
    let scrambled_seq = scramble_passport_sequence(&province, city_code, seq0);
    let body_number = scrambled_seq * PASSPORT_CITY_NAMESPACE_COUNT + namespace;
    let body = crockford_fixed(body_number, PASSPORT_BODY_SYMBOLS);
    let source = format!("{province}{body}");
    let check = passport_check_char(&source);
    Ok(format!("{source}{check}"))
}

fn passport_no_exists<C: postgres::GenericClient>(
    conn: &mut C,
    passport_no: &str,
) -> Result<bool, String> {
    let row = conn
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM passport_numbers WHERE passport_no = $1)
             OR EXISTS(SELECT 1 FROM citizens WHERE passport_no = $1)",
            &[&passport_no],
        )
        .map_err(|e| format!("passport lookup failed: {e}"))?;
    Ok(row.get(0))
}

fn allocate_sequence<C: postgres::GenericClient>(
    conn: &mut C,
    seq_key: &str,
) -> Result<i64, String> {
    let row = conn
        .query_one(
            "INSERT INTO sequence_counters (seq_key, next_seq)
             VALUES ($1, 2)
             ON CONFLICT (seq_key) DO UPDATE SET next_seq = sequence_counters.next_seq + 1
             RETURNING next_seq - 1",
            &[&seq_key],
        )
        .map_err(|e| format!("sequence alloc failed: {e}"))?;
    Ok(row.get(0))
}

fn claim_recycled_passport_no<C: postgres::GenericClient>(
    conn: &mut C,
    cid_number: &str,
) -> Result<Option<String>, String> {
    let row = conn
        .query_opt(
            "SELECT pool_id, passport_no
             FROM passport_number_recycle_pool
             WHERE used_at IS NULL
             ORDER BY released_at, pool_id
             LIMIT 1
             FOR UPDATE SKIP LOCKED",
            &[],
        )
        .map_err(|e| format!("passport number recycle lookup failed: {e}"))?;

    let Some(row) = row else {
        return Ok(None);
    };
    let pool_id: String = row.get(0);
    let passport_no: String = row.get(1);
    let affected = conn
        .execute(
            "UPDATE passport_number_recycle_pool
             SET used_at = now(), used_by_cid_number = $1
             WHERE pool_id = $2 AND used_at IS NULL",
            &[&cid_number, &pool_id],
        )
        .map_err(|e| format!("passport number recycle claim failed: {e}"))?;
    if affected != 1 {
        return Err("passport number recycle claim failed".to_string());
    }
    Ok(Some(passport_no))
}

#[cfg(test)]
// 护照号测试依赖固定合法夹具，断言式解包仅用于测试失败定位。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{
        build_passport_no, passport_city_namespace, passport_valid_until, passport_validity_years,
        PASSPORT_CITY_NAMESPACE_COUNT, PASSPORT_LOCAL_CAPACITY,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn passport_city_namespace_is_unique_for_city_codes_in_scope() {
        let mut seen = std::collections::BTreeSet::new();
        for city in 1..=392 {
            let code = format!("{city:03}");
            let namespace = passport_city_namespace("GD", &code).expect("namespace");
            assert!(namespace < PASSPORT_CITY_NAMESPACE_COUNT);
            assert!(seen.insert(namespace));
        }
    }

    #[test]
    fn passport_no_uses_province_body_and_check_char() {
        let no = build_passport_no("gd", "001", 1).expect("passport no");
        assert_eq!(no.len(), 11);
        assert!(no.starts_with("GD"));
        assert!(no
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn passport_no_supports_last_local_sequence() {
        let no =
            build_passport_no("GD", "392", PASSPORT_LOCAL_CAPACITY as i64).expect("passport no");
        assert_eq!(no.len(), 11);
    }

    #[test]
    fn passport_validity_switches_at_sixteen() {
        let created = Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0).unwrap();
        assert_eq!(
            passport_validity_years(
                created,
                chrono::NaiveDate::from_ymd_opt(2010, 6, 30).unwrap()
            ),
            10
        );
        assert_eq!(
            passport_validity_years(
                created,
                chrono::NaiveDate::from_ymd_opt(2010, 7, 1).unwrap()
            ),
            5
        );
        assert_eq!(passport_valid_until(created, 10), "2036-06-29");
    }
}
