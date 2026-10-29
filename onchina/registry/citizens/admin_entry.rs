//! 注册局直接录入公民 handler。
//!
//! 公民由注册局管理员在办理市先录入本地档案。请求只提交公民档案字段;
//! 身份 CID、护照号、护照有效期由服务端确定性生成并落库。
//! 链账户留到链上身份推送阶段录入，并由该账户签名确认。

// Handler 直接复用统一 Axum Response；日期 expect 只消费本模块已严格校验并规范化的日期。
#![allow(clippy::result_large_err, clippy::expect_used)]

use crate::registry::citizens::model::{citizen_row_from_record, citizen_status_text};
use axum::http::{HeaderMap, StatusCode};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cid::{generate_cid_number, GenerateCidInput};
use crate::core::chain_citizen_identity::FinalizedCitizenIdentity;
use crate::crypto::pubkey::normalize_account_id;
use crate::registry::citizens::passport_no::{
    generate_passport_no_with_retry, is_voting_age_at, passport_valid_from, passport_valid_until,
    passport_validity_years,
};
use crate::*;

/// 直接录入公民请求 DTO。
///
/// 居住省市由当前注册局办理上下文校验,前端只负责回传当前选择。
/// 本地建档不得要求链账户；儿童或暂未开户公民同样能先发放电子护照。
/// 匿名占号请求(第一段):只需岗位码 + 人主体类型。姓名/出生/居住/性别等档案全部
/// 后期在详情页编辑完善(现有严格逻辑),占号阶段不收。用户钱包账户与签名在第二段
/// `submit_citizen_occupy` 收集。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminCreateCitizenInput {
    /// 当前注册局内的任职岗位码；与机构 CID、管理员签名钱包共同构成权限主体。
    pub(crate) actor_role_code: String,
    /// 人主体类型:CTZN(公民)/ NATP(居民)。必填,决定发号机构码,区分公民与居民。
    pub(crate) cid_type: String,
}

/// 直接录入公民返回 DTO。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminCreateCitizenOutput {
    pub(crate) id: u64,
    pub(crate) cid_number: String,
    pub(crate) passport_no: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) citizen_sex: String,
    pub(crate) citizen_birth_date: String,
    pub(crate) citizen_status: CitizenStatus,
    pub(crate) voting_eligible: bool,
    pub(crate) account_id: Option<String>,
    pub(crate) passport_valid_from: String,
    pub(crate) passport_valid_until: String,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
    pub(crate) archive_hash: Option<String>,
}

/// 建档输入校验产物:两阶段占号流程经会话 JSON 往返(ADR-031 D6)。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValidatedCitizenInput {
    /// 人主体类型:CTZN / NATP(已校验)。发号机构码单源。
    pub(crate) cid_type: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) citizen_sex: String,
    /// YYYY-MM-DD(已校验)。
    pub(crate) citizen_birth_date: String,
    pub(crate) province_name: String,
    pub(crate) city_name: String,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
    pub(crate) voting_eligible: bool,
}

/// 校验建档输入(占号 prepare 阶段调用;不生成号、不落库,ADR-031 占号先行)。
pub(crate) fn validate_citizen_input(
    ctx: &crate::authentication::login::AdminAuthContext,
    input: &AdminCreateCitizenInput,
) -> Result<ValidatedCitizenInput, axum::response::Response> {
    let cid_type = match input.cid_type.trim() {
        "CTZN" => "CTZN".to_string(),
        "NATP" => "NATP".to_string(),
        _ => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "人主体类型必须是 CTZN(公民) 或 NATP(居民)",
            ))
        }
    };
    // 匿名占号:居住省/市取办理注册局作用域(给本地记录一个省分区键);其余档案(姓名/出生/
    // 性别/出生地/居住镇)后期在详情页编辑完善(现有严格逻辑),占号阶段一律留空。
    let scope = crate::authorization::get_visible_scope(ctx);
    if !scope.can_write {
        return Err(api_error(StatusCode::FORBIDDEN, 1003, "当前登录无办理权限"));
    }
    let Some(province_name) = scope.locked_province_name.clone() else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "当前注册局缺少省作用域,无法占号",
        ));
    };
    let Some(province_code) = crate::cid::china::province_code_by_name(province_name.as_str())
    else {
        return Err(api_error(StatusCode::BAD_REQUEST, 1001, "未知的办理省份"));
    };
    let province_code = province_code.to_string();
    let (city_name, city_code) = match scope.locked_city_name.clone() {
        Some(city_name) => {
            let Some(city_code) =
                crate::cid::china::city_code_by_name(province_name.as_str(), city_name.as_str())
            else {
                return Err(api_error(StatusCode::BAD_REQUEST, 1001, "未知的办理城市"));
            };
            (city_name, city_code.to_string())
        }
        None => (String::new(), String::new()),
    };

    Ok(ValidatedCitizenInput {
        cid_type,
        family_name: String::new(),
        given_name: String::new(),
        citizen_sex: String::new(),
        citizen_birth_date: String::new(),
        province_name,
        city_name,
        province_code,
        city_code,
        town_code: String::new(),
        birth_province_code: String::new(),
        birth_city_code: String::new(),
        birth_town_code: String::new(),
        voting_eligible: false,
    })
}

/// 发号种子:链上中国注册局所有人主体登记(匿名/投票/竞选)统一走本种子,
/// 由档案稳定字段确定性派生(落库失败恢复时同种子续用同号)。
pub(crate) fn cid_seed(v: &ValidatedCitizenInput) -> String {
    // 出生日期可空(匿名占号档案选填),种子按字符串原样哈希,不解析成 NaiveDate。
    local_citizen_cid_seed(
        &v.family_name,
        &v.given_name,
        &v.citizen_sex,
        &v.citizen_birth_date,
        &v.province_code,
        &v.city_code,
        &v.town_code,
        &v.birth_province_code,
        &v.birth_city_code,
        &v.birth_town_code,
    )
}

/// 按种子 + nonce 后缀生成候选号(碰撞重试用;nonce=0 与历史种子字节一致)。
pub(crate) fn generate_citizen_cid_candidate(
    v: &ValidatedCitizenInput,
    seed: &str,
    nonce: u32,
) -> Result<String, axum::response::Response> {
    let seeded = if nonce == 0 {
        seed.to_string()
    } else {
        format!("{seed}|n{nonce}")
    };
    let cid_number = generate_cid_number(GenerateCidInput {
        account_id: seeded.as_str(),
        p1: "1",
        province_name: v.province_name.as_str(),
        city_name: v.city_name.as_str(),
        institution: v.cid_type.as_str(),
    })
    .map_err(|err| {
        let detail = format!("公民身份CID生成失败: {err}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, detail.as_str())
    })?;
    if crate::cid::validate_cid_number_format(cid_number.as_str()).is_err() {
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "公民身份CID格式生成失败",
        ));
    }
    Ok(cid_number)
}

/// 占号进块后落库(submit 阶段调用):护照签发 + 档案入库 + 审计。
pub(crate) fn persist_citizen_record(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
    v: &ValidatedCitizenInput,
    cid_number: &str,
    onchain_tx_hash: &str,
    finalized: &FinalizedCitizenIdentity,
) -> Result<CitizenRecord, String> {
    // 落库的账户与 revision 一律取自 finalized 链读快照,不取请求体、也不取本地投影:
    // 本地档案是链上绑定的投影,来源必须唯一。绑定无效(号未占/已吊销/无当前账户)时
    // fail-closed 直接拒,绝不落一条 account_id 为空或指向旧账户的档案。
    let Some((citizen_account_id, binding_revision)) = finalized.active_binding() else {
        return Err("finalized CID 绑定无效".into());
    };
    // 账户与块哈希统一按 ADR-040 规范文本落库:小写 0x + 十六进制。
    let citizen_account_id = format!("0x{}", hex::encode(citizen_account_id));
    let binding_block_hash = format!("0x{}", hex::encode(finalized.finalized_block_hash));
    // 匿名占号:档案选填,出生日期可空,不解析成 NaiveDate(空串会 panic),按字符串原样落库。
    let citizen_birth_date = v.citizen_birth_date.clone();
    let family_name = v.family_name.clone();
    let given_name = v.given_name.clone();
    let citizen_sex = v.citizen_sex.clone();
    let province_code = v.province_code.clone();
    let city_code = v.city_code.clone();
    let town_code = v.town_code.clone();
    let birth_province_code = v.birth_province_code.clone();
    let birth_city_code = v.birth_city_code.clone();
    let birth_town_code = v.birth_town_code.clone();
    let cid_number = cid_number.to_string();

    let now = Utc::now();
    // 匿名占号不发护照:护照号 + 有效期留空;护照随详情页完善(填入出生日期)时由
    // admin_update_citizen 一次性签发(护照有效期属投票 CID 档,非匿名档)。
    let passport_no = String::new();
    let valid_from = String::new();
    let valid_until = String::new();
    // ID 在正式写入事务中按 CID 统一分配。
    let id = 0;

    let mut record = CitizenRecord {
        id,
        cid_number: cid_number.clone(),
        passport_no: passport_no.clone(),
        family_name: family_name.clone(),
        given_name: given_name.clone(),
        citizen_sex: citizen_sex.clone(),
        citizen_birth_date: citizen_birth_date.clone(),
        // 占即绑:占号阶段就绑定用户钱包账户(链上 occupy_cid 已绑)。
        account_id: Some(citizen_account_id),
        binding_revision,
        binding_block_number: Some(i64::from(finalized.finalized_block_number)),
        binding_block_hash: Some(binding_block_hash),
        citizen_status: CitizenStatus::Normal,
        voting_eligible: v.voting_eligible,
        passport_valid_from: valid_from.clone(),
        passport_valid_until: valid_until.clone(),
        status_updated_at: Some(now.timestamp()),
        province_code: province_code.clone(),
        city_code: city_code.clone(),
        town_code: town_code.clone(),
        birth_province_code: birth_province_code.clone(),
        birth_city_code: birth_city_code.clone(),
        birth_town_code: birth_town_code.clone(),
        archive_hash: None,
        onchain_tx_hash: Some(onchain_tx_hash.to_string()),
        onchain_block_number: Some(i64::from(finalized.finalized_block_number)),
        onchain_at: Some(now),
        creator_account_id: account_id.to_string(),
        created_at: now,
        updater_account_id: None,
        updated_at: now,
    };
    record.archive_hash = Some(citizen_archive_hash(&record));

    Db::lock_citizen_conn(conn, &cid_number)?;
    if let Some(existing) = Db::find_citizen_by_cid_conn(conn, &cid_number)? {
        // 重复恢复不能把后来补齐的姓名、护照、附件关联或较新链版本重置成匿名初始值。
        return Ok(existing);
    }
    record.id = Db::upsert_target_citizen_rows(conn, &record)?;
    Ok(record)
}

/// 建档返回 DTO(submit 阶段复用)。
pub(crate) fn create_output_from_record(record: CitizenRecord) -> AdminCreateCitizenOutput {
    AdminCreateCitizenOutput {
        id: record.id,
        cid_number: record.cid_number,
        passport_no: record.passport_no,
        family_name: record.family_name,
        given_name: record.given_name,
        citizen_sex: record.citizen_sex,
        citizen_birth_date: record.citizen_birth_date,
        citizen_status: record.citizen_status,
        voting_eligible: record.voting_eligible,
        account_id: record.account_id,
        passport_valid_from: record.passport_valid_from,
        passport_valid_until: record.passport_valid_until,
        province_code: record.province_code,
        city_code: record.city_code,
        town_code: record.town_code,
        birth_province_code: record.birth_province_code,
        birth_city_code: record.birth_city_code,
        birth_town_code: record.birth_town_code,
        archive_hash: record.archive_hash,
    }
}

pub(crate) struct ResolvedCitizenAccount {
    pub(crate) account_id: String,
}

#[allow(clippy::too_many_arguments)]
fn local_citizen_cid_seed(
    family_name: &str,
    given_name: &str,
    citizen_sex: &str,
    citizen_birth_date: &str,
    province_code: &str,
    city_code: &str,
    town_code: &str,
    birth_province_code: &str,
    birth_city_code: &str,
    birth_town_code: &str,
) -> String {
    // 发号种子来自可得档案字段(匿名占号时多为空,靠 nonce 碰撞重试保唯一);
    // 钱包账户属占即绑,不进种子(避免回头改变本地身份号)。
    let mut hasher = Sha256::new();
    for part in [
        family_name,
        given_name,
        citizen_sex,
        citizen_birth_date,
        province_code,
        city_code,
        town_code,
        birth_province_code,
        birth_city_code,
        birth_town_code,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("citizen-local-0x{}", hex::encode(hasher.finalize()))
}

pub(crate) fn resolve_citizen_account(
    account_id: &str,
) -> Result<ResolvedCitizenAccount, axum::response::Response> {
    let account_id = account_id.trim();
    if account_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id 不能为空",
        ));
    }
    let Some(account_id) = normalize_account_id(account_id) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id 必须是小写 0x 加 64 位十六进制",
        ));
    };
    Ok(ResolvedCitizenAccount { account_id })
}

fn citizen_archive_hash(record: &CitizenRecord) -> String {
    let value = serde_json::json!({
        "cidNumber": record.cid_number,
        "passportNo": record.passport_no,
        "familyName": record.family_name,
        "givenName": record.given_name,
        "citizenSex": record.citizen_sex,
        "citizenBirthDate": record.citizen_birth_date,
        "provinceCode": record.province_code,
        "cityCode": record.city_code,
        "townCode": record.town_code,
        "birthProvinceCode": record.birth_province_code,
        "birthCityCode": record.birth_city_code,
        "birthTownCode": record.birth_town_code,
        "passportValidFrom": record.passport_valid_from,
        "passportValidUntil": record.passport_valid_until,
        "votingEligible": record.voting_eligible,
    });
    let mut hasher = Sha256::new();
    hasher.update(value.to_string().as_bytes());
    format!("0x{}", hex::encode(hasher.finalize()))
}

impl Db {
    /// 按账户 ID 查公民档案。仅已完成账户绑定的公民会命中本查询。
    pub(crate) fn find_citizen_by_account_id(
        &self,
        account_id: &str,
    ) -> Result<Option<CitizenRecord>, String> {
        let account_id = account_id.trim().to_string();
        if account_id.is_empty() {
            return Ok(None);
        }
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT COALESCE(row_id, 0), cid_number, passport_no, family_name,
                            given_name, citizen_sex, birth_date, account_id,
                            binding_revision, binding_block,
                            binding_hash, citizen_status, voting_eligible,
                            valid_from, valid_until, status_at,
                            province_code, city_code, town_code,
                            birth_province, birth_city, birth_town,
                            archive_hash, tx_hash, block_number, onchain_at,
                            creator_account, created_at, updater_account, updated_at
                     FROM citizen
                     WHERE account_id = $1
                     ORDER BY created_at DESC
                     LIMIT 1",
                    &[&account_id],
                )
                .map_err(|e| format!("query citizen failed: {e}"))?;
            Ok(row.as_ref().map(citizen_record_from_row))
        })
    }

    pub(crate) fn allocate_passport_no(
        &self,
        province_code: &str,
        city_code: &str,
        cid_number: &str,
    ) -> Result<String, String> {
        let province_code = province_code.to_string();
        let city_code = city_code.to_string();
        let cid_number = cid_number.to_string();
        self.with_client(move |conn| {
            generate_passport_no_with_retry(conn, &province_code, &city_code, &cid_number)
        })
    }
}

fn citizen_status_from_db(status: &str) -> CitizenStatus {
    match status {
        "NORMAL" => CitizenStatus::Normal,
        _ => CitizenStatus::Revoked,
    }
}

pub(crate) fn citizen_record_from_row(row: &postgres::Row) -> CitizenRecord {
    let id: i64 = row.get(0);
    CitizenRecord {
        id: u64::try_from(id).unwrap_or(0),
        cid_number: row.get(1),
        passport_no: row.get(2),
        family_name: row.get(3),
        given_name: row.get(4),
        citizen_sex: row.get(5),
        citizen_birth_date: row.get(6),
        account_id: row.get(7),
        binding_revision: u64::try_from(row.get::<_, i64>(8)).unwrap_or(0),
        binding_block_number: row.get(9),
        binding_block_hash: row.get(10),
        citizen_status: citizen_status_from_db(row.get::<_, String>(11).as_str()),
        voting_eligible: row.get(12),
        passport_valid_from: row.get(13),
        passport_valid_until: row.get(14),
        status_updated_at: row.get(15),
        province_code: row.get(16),
        city_code: row.get(17),
        town_code: row.get(18),
        birth_province_code: row.get(19),
        birth_city_code: row.get(20),
        birth_town_code: row.get(21),
        archive_hash: row.get(22),
        onchain_tx_hash: row.get(23),
        onchain_block_number: row.get(24),
        onchain_at: row.get(25),
        creator_account_id: row.get(26),
        created_at: row.get(27),
        updater_account_id: row.get(28),
        updated_at: row.get(29),
    }
}

// ───────────────── 公民资料编辑 ─────────────────
//
// 字段可变性按"现实是否可变"定死:
//   可变:姓、名、居住市、居住镇(人会改名、会搬家)、选举资格。
//   不可变:性别、出生日期、出生省/市/镇、护照号——出生时空/身份唯一号现实不可变;
//           一经初始化保存即永久锁定(创世/待补公民现存为空时才允许初始化)。
//   固定不动:居住省(= CID 省 = 分区键)、公民 CID(主键)。跨省居住迁移属"跨地区",
//           涉及分区迁移与注册局交接,本入口暂不做,后续单独处理。

/// 编辑公民资料输入。
///
/// 护照号与护照有效期由服务端确定性签发(与建档同源:`allocate_passport_no` +
/// 出生日期派生年限),绝不接受前端直填,避免破坏护照号唯一性与有效期口径。
/// 居住省与身份 CID 不在本入口变更。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminEditCitizenInput {
    // ── 可变:姓名与居住市镇 ──
    #[serde(default)]
    pub(crate) family_name: String,
    #[serde(default)]
    pub(crate) given_name: String,
    /// 居住市(限本省内改;跨省不在本入口)。
    #[serde(default)]
    pub(crate) city_code: String,
    /// 居住镇。
    #[serde(default)]
    pub(crate) town_code: String,
    #[serde(default)]
    pub(crate) voting_eligible: bool,
    // ── 不可变:现存为空可初始化,一经保存即锁定 ──
    #[serde(default)]
    pub(crate) citizen_sex: String,
    #[serde(default)]
    pub(crate) citizen_birth_date: String,
    #[serde(default)]
    pub(crate) birth_province_code: String,
    #[serde(default)]
    pub(crate) birth_city_code: String,
    #[serde(default)]
    pub(crate) birth_town_code: String,
}

/// 不可变字段规则:现存非空即锁定——入参为空或与现存一致则保持,试图改成不同值则 Err。
/// 现存为空则用入参初始化(初始化保存成功后即受锁定)。
fn lock_immutable(field: &str, existing: &str, incoming: &str) -> Result<String, String> {
    let existing = existing.trim();
    let incoming = incoming.trim();
    if existing.is_empty() {
        Ok(incoming.to_string())
    } else if incoming.is_empty() || incoming == existing {
        Ok(existing.to_string())
    } else {
        Err(format!("{field}一经确认不可修改"))
    }
}

/// 锁定后的不可变身份字段(现存优先,空则用入参初始化)。姓名不在此列(可变)。
#[derive(Debug)]
pub(crate) struct LockedIdentity {
    pub(crate) citizen_sex: String,
    pub(crate) citizen_birth_date: String,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
}

/// 逐字段套用不可变锁,产出最终不可变身份字段;任一字段试图改动已确认值即整体拒绝。
pub(crate) fn resolve_locked_identity(
    existing: &CitizenRecord,
    input: &AdminEditCitizenInput,
) -> Result<LockedIdentity, String> {
    Ok(LockedIdentity {
        citizen_sex: lock_immutable("性别", &existing.citizen_sex, &input.citizen_sex)?,
        citizen_birth_date: lock_immutable(
            "出生日期",
            &existing.citizen_birth_date,
            &input.citizen_birth_date,
        )?,
        birth_province_code: lock_immutable(
            "出生省",
            &existing.birth_province_code,
            &input.birth_province_code,
        )?,
        birth_city_code: lock_immutable(
            "出生市",
            &existing.birth_city_code,
            &input.birth_city_code,
        )?,
        birth_town_code: lock_immutable(
            "出生镇",
            &existing.birth_town_code,
            &input.birth_town_code,
        )?,
    })
}

/// 校验最终字段:不可变字段只校验已填部分的格式(允许分批补全);居住市镇按固定
/// 居住省(existing.province_code)校验行政区归属;选举资格开启必须年满投票年龄。
fn validate_locked_identity(
    existing: &CitizenRecord,
    locked: &LockedIdentity,
    voting_eligible: bool,
    residence_city_code: &str,
    residence_town_code: &str,
) -> Result<(), String> {
    if !locked.citizen_sex.is_empty()
        && locked.citizen_sex != "MALE"
        && locked.citizen_sex != "FEMALE"
    {
        return Err("性别取值非法".to_string());
    }
    let today = Utc::now().date_naive();
    let birth_date = if locked.citizen_birth_date.is_empty() {
        None
    } else {
        let parsed = NaiveDate::parse_from_str(locked.citizen_birth_date.as_str(), "%Y-%m-%d")
            .map_err(|_| "出生日期格式非法".to_string())?;
        if parsed > today {
            return Err("出生日期不能晚于今天".to_string());
        }
        Some(parsed)
    };
    // 出生地三段:要么全空,要么三段齐全且是合法行政区三元组。
    let birth_filled = [
        &locked.birth_province_code,
        &locked.birth_city_code,
        &locked.birth_town_code,
    ]
    .iter()
    .filter(|v| !v.is_empty())
    .count();
    if birth_filled != 0 {
        if birth_filled != 3 {
            return Err("出生地必须省市镇一并填写".to_string());
        }
        let ok = crate::cid::china::area_name_by_codes(
            locked.birth_province_code.as_str(),
            Some(locked.birth_city_code.as_str()),
            Some(locked.birth_town_code.as_str()),
        )
        .map(|(p, c, t)| !p.is_empty() && c.is_some() && t.is_some())
        .unwrap_or(false);
        if !ok {
            return Err("未知的出生省市镇代码".to_string());
        }
    }
    // 居住地:省固定(existing.province_code),市/镇可变但须归属该省。填了镇必先有市。
    let residence_city = residence_city_code.trim();
    let residence_town = residence_town_code.trim();
    if !residence_town.is_empty() && residence_city.is_empty() {
        return Err("填写居住镇前必须先选居住市".to_string());
    }
    if !residence_city.is_empty() {
        let town_arg = (!residence_town.is_empty()).then_some(residence_town);
        let (city_ok, town_ok) = crate::cid::china::area_name_by_codes(
            existing.province_code.as_str(),
            Some(residence_city),
            town_arg,
        )
        .map(|(_, c, t)| (c.is_some(), t.is_some()))
        .unwrap_or((false, false));
        if !city_ok {
            return Err("未知的居住市代码".to_string());
        }
        if town_arg.is_some() && !town_ok {
            return Err("未知的居住镇代码".to_string());
        }
    }
    if voting_eligible {
        match birth_date {
            Some(birth) if is_voting_age_at(today, birth) => {}
            Some(_) => return Err("未满16周岁不能设置选举资格".to_string()),
            None => return Err("设置选举资格前必须先填写出生日期".to_string()),
        }
    }
    Ok(())
}

/// 编辑公民资料端点:注册局补齐/更新本地公民档案。
///
/// 可变:姓、名、居住市、居住镇、选举资格。不可变(初始化后锁定):性别、出生日期、
/// 出生地;护照号与有效期由服务端在出生日期就绪时确定性签发一次。居住省(CID 省/分区键)
/// 与 CID 不变;链下正本(创建人、created_at)、链上承诺(onchain_*)、账户与状态一律保留。
pub(crate) async fn admin_update_citizen(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    headers: HeaderMap,
    axum::extract::Path(cid_number): axum::extract::Path<String>,
    axum::Json(input): axum::Json<AdminEditCitizenInput>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let existing = match state.db.find_citizen_by_cid(cid_number.as_str()) {
        Ok(Some(record)) => record,
        Ok(None) => return crate::api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query citizen for edit failed");
            return crate::api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案查询失败");
        }
    };
    if let Err(resp) =
        crate::registry::citizens::chain_identity::ensure_record_in_admin_scope(&ctx, &existing)
    {
        return resp;
    }

    // 可变姓名:必填非空(允许改名,但不允许清空)。
    let family_name = input.family_name.trim().to_string();
    let given_name = input.given_name.trim().to_string();
    if family_name.is_empty() || given_name.is_empty() {
        return crate::api_error(StatusCode::BAD_REQUEST, 1001, "姓名不能为空");
    }
    // 可变居住市镇:入参为空则保持现存(允许只改其一)。
    let residence_city = {
        let v = input.city_code.trim();
        if v.is_empty() {
            existing.city_code.clone()
        } else {
            v.to_string()
        }
    };
    let residence_town = {
        let v = input.town_code.trim();
        if v.is_empty() {
            existing.town_code.clone()
        } else {
            v.to_string()
        }
    };

    let locked = match resolve_locked_identity(&existing, &input) {
        Ok(v) => v,
        Err(msg) => return crate::api_error(StatusCode::BAD_REQUEST, 1001, msg.as_str()),
    };
    if let Err(msg) = validate_locked_identity(
        &existing,
        &locked,
        input.voting_eligible,
        residence_city.as_str(),
        residence_town.as_str(),
    ) {
        return crate::api_error(StatusCode::BAD_REQUEST, 1001, msg.as_str());
    }

    // 本地档案编辑只消费一次 Passkey，早于护照分配和任何业务写入。
    if let Err(resp) = crate::authentication::actions::require_current_admin(&state, &ctx).await {
        return resp;
    }
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
        &state,
        &headers,
        &ctx.account_id,
    ) {
        Ok(proof) => proof,
        Err(response) => return response,
    };
    // 网络鉴权已经结束；在同一事务内重新读取档案、校验锁定字段并签发护照。
    let updated = match state.db.with_client(|conn| {
        apply_citizen_edit_conn(conn, &ctx, passkey.binding_id(), &cid_number, &input)
    }) {
        Ok(Ok(record)) => record,
        Ok(Err(response)) => return response,
        Err(err) => {
            tracing::error!(error = %err, "citizen edit transaction failed");
            return crate::api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案更新失败");
        }
    };
    axum::Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: crate::registry::citizens::model::citizen_row_from_record(&updated),
    })
    .into_response()
}

/// 绑定 -> CID -> 计数器的固定锁顺序；锁后再次校验，避免并发初始化覆盖不可变字段。
fn apply_citizen_edit_conn(
    conn: &mut postgres::Client,
    ctx: &crate::authentication::login::AdminAuthContext,
    expected_binding_id: &str,
    cid: &str,
    input: &AdminEditCitizenInput,
) -> Result<Result<CitizenRecord, axum::response::Response>, String> {
    let mut tx = conn
        .transaction()
        .map_err(|e| format!("begin citizen edit failed: {e}"))?;
    crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
    let binding = crate::authentication::repo::get_active_node_binding_conn(&mut tx)?;
    if !binding.is_some_and(|binding| {
        binding.binding_id == expected_binding_id
            && binding.institution_cid_number == ctx.institution_cid_number
            && binding.institution_code == ctx.institution_code
    }) {
        return Ok(Err(crate::api_error(
            StatusCode::CONFLICT,
            1005,
            "节点绑定已改变，请重新登录",
        )));
    }
    Db::lock_citizen_conn(&mut tx, cid)?;
    let Some(existing) = Db::find_citizen_by_cid_conn(&mut tx, cid)? else {
        return Ok(Err(crate::api_error(
            StatusCode::NOT_FOUND,
            1004,
            "公民档案不存在",
        )));
    };
    if let Err(response) =
        crate::registry::citizens::chain_identity::ensure_record_in_admin_scope(ctx, &existing)
    {
        return Ok(Err(response));
    }
    let family_name = input.family_name.trim().to_string();
    let given_name = input.given_name.trim().to_string();
    if family_name.is_empty() || given_name.is_empty() {
        return Ok(Err(crate::api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "姓名不能为空",
        )));
    }
    let city = if input.city_code.trim().is_empty() {
        existing.city_code.clone()
    } else {
        input.city_code.trim().to_string()
    };
    let town = if input.town_code.trim().is_empty() {
        existing.town_code.clone()
    } else {
        input.town_code.trim().to_string()
    };
    let locked = match resolve_locked_identity(&existing, input) {
        Ok(locked) => locked,
        Err(message) => {
            return Ok(Err(crate::api_error(
                StatusCode::BAD_REQUEST,
                1001,
                &message,
            )))
        }
    };
    if let Err(message) =
        validate_locked_identity(&existing, &locked, input.voting_eligible, &city, &town)
    {
        return Ok(Err(crate::api_error(
            StatusCode::BAD_REQUEST,
            1001,
            &message,
        )));
    }
    let now = Utc::now();
    let mut updated = existing;
    updated.family_name = family_name;
    updated.given_name = given_name;
    updated.city_code = city;
    updated.town_code = town;
    updated.voting_eligible = input.voting_eligible;
    updated.citizen_sex = locked.citizen_sex;
    updated.citizen_birth_date = locked.citizen_birth_date;
    updated.birth_province_code = locked.birth_province_code;
    updated.birth_city_code = locked.birth_city_code;
    updated.birth_town_code = locked.birth_town_code;
    if updated.passport_no.trim().is_empty() && !updated.citizen_birth_date.is_empty() {
        let birth = NaiveDate::parse_from_str(&updated.citizen_birth_date, "%Y-%m-%d")
            .map_err(|_| "validated birth date became invalid")?;
        updated.passport_no = generate_passport_no_with_retry(
            &mut tx,
            &updated.province_code,
            &updated.city_code,
            &updated.cid_number,
        )?;
        updated.passport_valid_from = passport_valid_from(now);
        updated.passport_valid_until =
            passport_valid_until(now, passport_validity_years(now, birth));
    }
    updated.status_updated_at = Some(now.timestamp());
    updated.updated_at = now;
    updated.archive_hash = Some(citizen_archive_hash(&updated));
    Db::upsert_target_citizen_rows(&mut tx, &updated)?;
    let saved = Db::find_citizen_by_cid_conn(&mut tx, cid)?.ok_or("saved citizen row missing")?;
    tx.commit()
        .map_err(|e| format!("commit citizen edit failed: {e}"))?;
    Ok(Ok(saved))
}

#[cfg(test)]
// 公民资料编辑契约测试使用断言式解包定位不可变字段回归。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod edit_tests {
    use super::*;

    fn base_record() -> CitizenRecord {
        CitizenRecord {
            id: 1,
            cid_number: format!(
                "CN000-CTZN{}-100000000-2026",
                primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
            ),
            passport_no: String::new(),
            family_name: String::new(),
            given_name: String::new(),
            citizen_sex: String::new(),
            citizen_birth_date: String::new(),
            account_id: None,
            binding_revision: 0,
            binding_block_number: None,
            binding_block_hash: None,
            citizen_status: CitizenStatus::Normal,
            voting_eligible: false,
            passport_valid_from: String::new(),
            passport_valid_until: String::new(),
            status_updated_at: None,
            province_code: "GZ".to_string(),
            city_code: "018".to_string(),
            town_code: String::new(),
            birth_province_code: String::new(),
            birth_city_code: String::new(),
            birth_town_code: String::new(),
            archive_hash: None,
            onchain_tx_hash: None,
            onchain_block_number: None,
            onchain_at: None,
            creator_account_id: "0x".to_string() + &"9c".repeat(32),
            created_at: chrono::DateTime::from_timestamp(1_779_580_800, 0).unwrap(),
            updater_account_id: None,
            updated_at: chrono::DateTime::from_timestamp(1_779_580_800, 0).unwrap(),
        }
    }

    #[test]
    fn citizen_outputs_only_expose_canonical_account_identity() {
        let mut record = base_record();
        let account_id = format!("0x{}", "11".repeat(32));
        record.account_id = Some(account_id.clone());
        for output in [
            serde_json::to_value(create_output_from_record(record.clone())).unwrap(),
            serde_json::to_value(crate::registry::citizens::model::citizen_row_from_record(
                &record,
            ))
            .unwrap(),
        ] {
            assert_eq!(output["accountId"], account_id);
            assert!(output.get("ss58Address").is_none());
            assert!(output.get("ss58_address").is_none());
        }
        assert_eq!(
            resolve_citizen_account(&account_id).unwrap().account_id,
            account_id
        );
        for invalid in [
            "",
            "0x11",
            "11",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            assert!(resolve_citizen_account(invalid).is_err());
        }
    }

    fn input() -> AdminEditCitizenInput {
        AdminEditCitizenInput {
            family_name: "程".to_string(),
            given_name: "伟".to_string(),
            city_code: "018".to_string(),
            // 唯一 china.sqlite 中 GZ/018 的真实三位镇码（坪乐镇）。
            town_code: "013".to_string(),
            voting_eligible: true,
            citizen_sex: "MALE".to_string(),
            citizen_birth_date: "1988-05-20".to_string(),
            birth_province_code: "GZ".to_string(),
            birth_city_code: "018".to_string(),
            birth_town_code: "013".to_string(),
        }
    }

    #[test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    fn concurrent_edits_lock_identity_and_stale_projection_rolls_back_all_indexes() {
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let owner = format!(
            "GZ018-CREG{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
        );
        let ctx = crate::authentication::login::AdminAuthContext {
            account_id: format!("0x{}", "9c".repeat(32)),
            institution_cid_number: owner.clone(),
            institution_code: "CREG".into(),
            admin_level: Some("CITY".into()),
            family_name: "测".into(),
            given_name: "试".into(),
            scope_province_name: Some("贵州省".into()),
            scope_city_name: Some("绥阳市".into()),
            scope_town_name: None,
            cid_short_name: None,
        };
        let record = base_record();
        let binding = crate::authentication::login::NodeInstitutionBinding {
            binding_id: "edit-binding".into(),
            candidate_id: "edit-candidate".into(),
            institution_code: "CREG".into(),
            institution_cid_number: owner,
            bound_account_id: ctx.account_id.clone(),
            bound_at: record.created_at,
            status: "ACTIVE".into(),
        };
        sandbox
            .db
            .with_client(|conn| {
                crate::authentication::repo::upsert_active_node_binding_conn(conn, &binding)
            })
            .unwrap();
        sandbox.db.upsert_citizen_row(&record).unwrap();
        let handles = ["1988-05-20", "1989-05-20"]
            .into_iter()
            .map(|birth| {
                let db = sandbox.db.clone();
                let ctx = ctx.clone();
                let cid = record.cid_number.clone();
                std::thread::spawn(move || {
                    let mut edit = input();
                    edit.citizen_birth_date = birth.into();
                    db.with_client(|conn| {
                        apply_citizen_edit_conn(conn, &ctx, "edit-binding", &cid, &edit)
                    })
                    .unwrap()
                    .is_ok()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|success| *success)
                .count(),
            1
        );
        let saved = sandbox
            .db
            .find_citizen_by_cid(&record.cid_number)
            .unwrap()
            .unwrap();
        assert!(!saved.passport_no.is_empty());
        assert!(saved.id > 0);
        let mut current = saved.clone();
        current.binding_revision = 2;
        current.account_id = Some(format!("0x{}", "33".repeat(32)));
        sandbox.db.upsert_citizen_row(&current).unwrap();
        let mut stale = saved;
        stale.province_code = "GD".into();
        stale.city_code = "001".into();
        assert!(sandbox.db.upsert_citizen_row(&stale).is_err());
        let reread = sandbox
            .db
            .find_citizen_by_cid(&record.cid_number)
            .unwrap()
            .unwrap();
        assert_eq!(reread.province_code, "GZ");
        assert_eq!(reread.binding_revision, 2);
        sandbox
            .db
            .with_client(|conn| {
                let province: String = conn
                    .query_one(
                        "SELECT province_code FROM identity WHERE cid_number=$1",
                        &[&record.cid_number],
                    )
                    .unwrap()
                    .get(0);
                assert_eq!(province, "GZ");
                let passports: i64 = conn
                    .query_one(
                        "SELECT COUNT(*) FROM passport WHERE cid_number=$1",
                        &[&record.cid_number],
                    )
                    .unwrap()
                    .get(0);
                assert_eq!(passports, 1);
                Ok(())
            })
            .unwrap();
        sandbox
            .db
            .with_client(|conn| {
                crate::authentication::repo::deactivate_active_node_binding_conn(conn).map(|_| ())
            })
            .unwrap();
        assert!(sandbox
            .db
            .with_client(|conn| apply_citizen_edit_conn(
                conn,
                &ctx,
                "edit-binding",
                &record.cid_number,
                &input()
            ))
            .unwrap()
            .is_err());
    }

    #[test]
    fn empty_immutable_fields_are_initialized() {
        let locked = resolve_locked_identity(&base_record(), &input()).expect("ok");
        assert_eq!(locked.citizen_sex, "MALE");
        assert_eq!(locked.citizen_birth_date, "1988-05-20");
        assert_eq!(locked.birth_town_code, "013");
    }

    #[test]
    fn immutable_birth_date_cannot_change() {
        let mut existing = base_record();
        existing.citizen_birth_date = "1988-05-20".to_string(); // 已初始化
        let mut edit = input();
        edit.citizen_birth_date = "1990-01-01".to_string(); // 试图改出生日期
        let err = resolve_locked_identity(&existing, &edit).unwrap_err();
        assert!(err.contains("出生日期"));
    }

    #[test]
    fn name_is_mutable_not_locked() {
        // 姓名可变:即便现存已有姓名,改名也不应触发锁定拒绝(姓名不进 LockedIdentity)。
        let mut existing = base_record();
        existing.citizen_sex = "MALE".to_string();
        let mut edit = input();
        edit.family_name = "王".to_string(); // 改姓
        edit.given_name = "五".to_string(); // 改名
        edit.citizen_sex = "MALE".to_string(); // 不可变一致 → 保持
        let locked = resolve_locked_identity(&existing, &edit).expect("ok");
        assert_eq!(locked.citizen_sex, "MALE"); // 不可变字段仍锁定
                                                // 姓名的实际采用在 handler 直接取 input,不受 lock 影响。
    }

    #[test]
    fn immutable_field_kept_when_input_matches_or_empty() {
        let mut existing = base_record();
        existing.citizen_birth_date = "1988-05-20".to_string();
        existing.citizen_sex = "MALE".to_string();
        let mut edit = input();
        edit.citizen_birth_date = String::new(); // 入参空 → 保持
        edit.citizen_sex = "MALE".to_string(); // 一致 → 保持
        let locked = resolve_locked_identity(&existing, &edit).expect("ok");
        assert_eq!(locked.citizen_birth_date, "1988-05-20");
        assert_eq!(locked.citizen_sex, "MALE");
    }

    #[test]
    fn voting_eligible_requires_birth_date() {
        let existing = base_record(); // 出生日期空
        let mut edit = input();
        edit.citizen_birth_date = String::new();
        // 隔离出生日期校验:清空出生地三段与居住市镇,避免行政区校验先行拦截。
        edit.birth_province_code = String::new();
        edit.birth_city_code = String::new();
        edit.birth_town_code = String::new();
        edit.voting_eligible = true;
        let locked = resolve_locked_identity(&existing, &edit).expect("lock ok");
        let err = validate_locked_identity(&existing, &locked, true, "", "").unwrap_err();
        assert!(err.contains("出生日期"));
    }

    #[test]
    fn illegal_sex_is_rejected() {
        let existing = base_record();
        let mut edit = input();
        edit.citizen_sex = "X".to_string();
        let locked = resolve_locked_identity(&existing, &edit).expect("lock ok");
        let err = validate_locked_identity(&existing, &locked, false, "", "").unwrap_err();
        assert!(err.contains("性别"));
    }
}

// 公民专属数据库操作，原SQL与锁顺序保持。
impl Db {
    pub(crate) fn upsert_citizen_row(&self, record: &CitizenRecord) -> Result<u64, String> {
        let record = record.clone();
        self.with_client(move |conn| {
            let mut tx = conn
                .transaction()
                .map_err(|e| format!("begin citizen write failed: {e}"))?;
            // 固定锁顺序：绑定 -> CID -> ID 计数器，与批量实体投影一致。
            crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
            let id = Self::upsert_target_citizen_rows(&mut tx, &record)?;
            tx.commit()
                .map_err(|e| format!("commit citizen write failed: {e}"))?;
            Ok(id)
        })
    }
    pub(crate) fn upsert_target_citizen_rows(
        conn: &mut impl postgres::GenericClient,
        record: &CitizenRecord,
    ) -> Result<u64, String> {
        let cid_number = record.cid_number.trim().to_string();
        if cid_number.is_empty() {
            return Err("citizen cid_number is required".to_string());
        }
        let province_code = record.province_code.trim().to_string();
        let city_code = record.city_code.trim().to_string();
        // 省码是分区键必填;市码可空(省级 FRG 注册局无市;匿名占号归办理注册局省分区)。
        if province_code.is_empty() {
            return Err("citizen province_code is required".to_string());
        }
        let status = if matches!(record.computed_identity_status(), CitizenStatus::Normal) {
            "ACTIVE"
        } else {
            "REVOKED"
        };
        let citizen_status = citizen_status_text(&record.citizen_status);
        // 所有新增入口共用原子计数器；同一 CID 在同一事务中串行，既有 ID 永不改写。
        Self::lock_citizen_conn(conn, &cid_number)?;
        // 先检查修订号再更新 ids/subjects/护照索引，拒绝旧快照时整个事务均不得留下副作用。
        if let Some(current) = Self::find_citizen_by_cid_conn(conn, &cid_number)? {
            if current.binding_revision > record.binding_revision
                || (current.binding_revision == record.binding_revision
                    && current.account_id != record.account_id)
            {
                return Err("citizen binding revision conflict".to_string());
            }
        }
        let existing_id = conn
            .query_opt(
                "SELECT row_id FROM citizen WHERE cid_number=$1 LIMIT 1",
                &[&cid_number],
            )
            .map_err(|e| format!("read citizen id failed: {e}"))?
            .and_then(|row| row.get::<_, Option<i64>>(0))
            .filter(|id| *id > 0);
        let id = match existing_id {
            Some(id) => id,
            None => Self::allocate_citizen_id_conn(conn)?,
        };
        let binding_revision = i64::try_from(record.binding_revision)
            .map_err(|_| "citizen binding revision exceeds i64".to_string())?;
        Self::upsert_target_id_row(
            conn,
            cid_number.as_str(),
            "CITIZEN",
            province_code.as_str(),
            Some(city_code.as_str()),
        )?;
        Self::delete_target_rows_outside_scope(
            conn,
            "subject",
            cid_number.as_str(),
            province_code.as_str(),
        )?;
        Self::delete_target_rows_outside_scope(
            conn,
            "citizen",
            cid_number.as_str(),
            province_code.as_str(),
        )?;
        conn.execute(
            "INSERT INTO subject (
                cid_number, record_kind, province_code, city_code, state_code, created_at, updated_at
             ) VALUES ($1, 'CITIZEN', $2, $3, $4, $5, now())
             ON CONFLICT (province_code, cid_number) DO UPDATE SET
                city_code = EXCLUDED.city_code,
                state_code = EXCLUDED.state_code,
                updated_at = now()",
            &[
                &cid_number,
                &province_code,
                &city_code,
                &status,
                &record.created_at,
            ],
        )
        .map_err(|e| format!("upsert citizen subject failed: {e}"))?;
        let written = conn
            .execute(
                "INSERT INTO citizen (
                cid_number, passport_no, family_name, given_name,
                citizen_sex, birth_date, province_code, city_code, row_id,
                account_id, binding_revision, binding_block,
                binding_hash, citizen_status, voting_eligible, valid_from,
                valid_until, status_at, town_code, birth_province, birth_city,
                birth_town, archive_hash, tx_hash, block_number, onchain_at,
                creator_account, created_at, updater_account, updated_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19,
                $20, $21, $22, $23, $24, $25, $26, $27, $28,
                $29, $30
             )
             ON CONFLICT (province_code, cid_number) DO UPDATE SET
                passport_no = EXCLUDED.passport_no,
                family_name = EXCLUDED.family_name,
                given_name = EXCLUDED.given_name,
                citizen_sex = EXCLUDED.citizen_sex,
                birth_date = EXCLUDED.birth_date,
                city_code = EXCLUDED.city_code,
                row_id = EXCLUDED.row_id,
                account_id = EXCLUDED.account_id,
                binding_revision = EXCLUDED.binding_revision,
                binding_block = EXCLUDED.binding_block,
                binding_hash = EXCLUDED.binding_hash,
                citizen_status = EXCLUDED.citizen_status,
                voting_eligible = EXCLUDED.voting_eligible,
                valid_from = EXCLUDED.valid_from,
                valid_until = EXCLUDED.valid_until,
                status_at = EXCLUDED.status_at,
                town_code = EXCLUDED.town_code,
                birth_province = EXCLUDED.birth_province,
                birth_city = EXCLUDED.birth_city,
                birth_town = EXCLUDED.birth_town,
                archive_hash = EXCLUDED.archive_hash,
                tx_hash = EXCLUDED.tx_hash,
                block_number = EXCLUDED.block_number,
                onchain_at = EXCLUDED.onchain_at,
                updater_account = EXCLUDED.updater_account,
                updated_at = EXCLUDED.updated_at
             WHERE citizen.binding_revision < EXCLUDED.binding_revision
                OR (
                    citizen.binding_revision = EXCLUDED.binding_revision
                    AND citizen.account_id IS NOT DISTINCT FROM EXCLUDED.account_id
                )",
                &[
                    &cid_number,
                    &record.passport_no,
                    &record.family_name,
                    &record.given_name,
                    &record.citizen_sex,
                    &record.citizen_birth_date,
                    &province_code,
                    &city_code,
                    &id,
                    &record.account_id,
                    &binding_revision,
                    &record.binding_block_number,
                    &record.binding_block_hash,
                    &citizen_status,
                    &record.voting_eligible,
                    &record.passport_valid_from,
                    &record.passport_valid_until,
                    &record.status_updated_at,
                    &record.town_code,
                    &record.birth_province_code,
                    &record.birth_city_code,
                    &record.birth_town_code,
                    &record.archive_hash,
                    &record.onchain_tx_hash,
                    &record.onchain_block_number,
                    &record.onchain_at,
                    &record.creator_account_id,
                    &record.created_at,
                    &record.updater_account_id,
                    &record.updated_at,
                ],
            )
            .map_err(|e| format!("upsert citizens failed: {e}"))?;
        if written != 1 {
            return Err("citizen write did not update exactly one row".to_string());
        }
        // 匿名占号无护照(护照随详情页完善出生日期时一次性签发)→ 空护照号不登记 passport_numbers,
        // 否则多条匿名记录会在 passport_no='' 主键冲突。
        if !record.passport_no.trim().is_empty() {
            conn.execute(
                "INSERT INTO passport (passport_no, cid_number, province_code, city_code, created_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (passport_no) DO UPDATE SET
                    cid_number = EXCLUDED.cid_number,
                    province_code = EXCLUDED.province_code,
                    city_code = EXCLUDED.city_code",
                &[
                    &record.passport_no,
                    &cid_number,
                    &province_code,
                    &city_code,
                    &record.created_at,
                ],
            )
            .map_err(|e| format!("upsert passport number failed: {e}"))?;
        }
        Ok(id as u64)
    }
    pub(crate) fn lock_citizen_conn(
        conn: &mut impl postgres::GenericClient,
        cid: &str,
    ) -> Result<(), String> {
        let key = format!("onchina-citizen:{cid}");
        conn.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&key],
        )
        .map_err(|e| format!("lock citizen failed: {e}"))?;
        Ok(())
    }
    pub(crate) fn allocate_citizen_id_conn(
        conn: &mut impl postgres::GenericClient,
    ) -> Result<i64, String> {
        if conn
            .query_opt(
                "SELECT next_seq FROM sequence WHERE seq_key='citizen_id'",
                &[],
            )
            .map_err(|e| format!("read citizen id counter failed: {e}"))?
            .is_none()
        {
            conn.execute("INSERT INTO sequence(seq_key,next_seq) SELECT 'citizen_id', COALESCE(MAX(row_id),0) FROM citizen ON CONFLICT (seq_key) DO NOTHING", &[])
                .map_err(|e| format!("initialize citizen id counter failed: {e}"))?;
        }
        let row = conn.query_opt("UPDATE sequence SET next_seq=next_seq+1 WHERE seq_key='citizen_id' AND next_seq>=0 AND next_seq<9223372036854775807 RETURNING next_seq", &[])
            .map_err(|e| format!("allocate citizen id failed: {e}"))?
            .ok_or_else(|| "citizen id capacity exhausted or counter invalid".to_string())?;
        Ok(row.get(0))
    }

    pub(crate) fn list_citizens_page(
        &self,
        keyword: &str,
        province_code: Option<&str>,
        city_code: Option<&str>,
        cursor: Option<&str>,
        page_size: usize,
    ) -> Result<PageResult<CitizenRow>, String> {
        let keyword = keyword.trim();
        let cursor = decode_db_page_cursor(cursor)?;
        let keyword = keyword.to_string();
        let province_code = province_code.map(str::to_string);
        let city_code = city_code.map(str::to_string);
        self.with_client(move |conn| {
            let cursor_created_at = cursor.map(|c| c.created_at);
            let cursor_id = cursor.map(|c| c.id).unwrap_or(i64::MAX);
            let fetch_limit = i64::try_from(page_size.saturating_add(1))
                .map_err(|_| "page_size too large".to_string())?;
            let rows = conn
                .query(
                    "SELECT COALESCE(c.row_id, 0), c.cid_number, c.passport_no, c.family_name,
                                    c.given_name, c.citizen_sex, c.birth_date,
                                    c.account_id, c.binding_revision,
                                    c.binding_block,
                                    c.binding_hash,
                                    c.citizen_status, c.voting_eligible,
                                    c.valid_from, c.valid_until, c.status_at,
                                    c.province_code, c.city_code, c.town_code,
                                    c.birth_province, c.birth_city, c.birth_town,
                                    c.archive_hash, c.tx_hash, c.block_number, c.onchain_at,
                                    c.creator_account, c.created_at, c.updater_account, c.updated_at
                             FROM citizen c
                             JOIN subject s
                               ON s.province_code = c.province_code
                              AND s.cid_number = c.cid_number
                              AND s.record_kind = 'CITIZEN'
                             WHERE ($1::text IS NULL OR c.province_code = $1)
                               AND ($2::text IS NULL OR c.city_code = $2)
                               AND (
                                    $3::text = ''
                                    OR
                                    c.cid_number = $3
                                    OR c.passport_no = $3
                                    OR c.family_name || c.given_name = $3
                                    OR c.family_name = $3
                                    OR c.given_name = $3
                                    OR c.account_id = $3
                               )
                               AND (
                                    $4::timestamptz IS NULL
                                    OR c.created_at < $4
                                    OR (c.created_at = $4 AND COALESCE(c.row_id, 0) < $5)
                               )
                             ORDER BY c.created_at DESC, COALESCE(c.row_id, 0) DESC
                             LIMIT $6",
                    &[
                        &province_code,
                        &city_code,
                        &keyword,
                        &cursor_created_at,
                        &cursor_id,
                        &fetch_limit,
                    ],
                )
                .map_err(|e| format!("query citizens failed: {e}"))?;
            let mut output = Vec::with_capacity(rows.len());
            for row in rows {
                let id_i64: i64 = row.get(0);
                let created_at: DateTime<Utc> = row.get(27);
                let record = crate::registry::citizens::admin_entry::citizen_record_from_row(&row);
                output.push((citizen_row_from_record(&record), created_at, id_i64));
            }
            Ok(page_from_rows(output, page_size))
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod citizen_id_tests {
    use super::*;

    #[test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    fn concurrent_ids_are_unique_and_capacity_exhaustion_is_explicit() {
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let handles = (0..8)
            .map(|_| {
                let db = sandbox.db.clone();
                std::thread::spawn(move || {
                    (0..16)
                        .map(|_| db.with_client(Db::allocate_citizen_id_conn).unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let ids = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 128);
        assert_eq!(ids.first(), Some(&1));
        assert_eq!(ids.last(), Some(&128));
        sandbox
            .db
            .with_client(|conn| {
                conn.execute(
                    "UPDATE sequence SET next_seq=9223372036854775807 WHERE seq_key='citizen_id'",
                    &[],
                )
                .unwrap();
                assert!(Db::allocate_citizen_id_conn(conn).is_err());
                Ok(())
            })
            .unwrap();
    }
}

/// 原公民投影只在所属模块执行；回写仍与会话租约及完成标记同一事务。
pub(crate) async fn commit_module_projection(
    db: &Db,
    session: &crate::transactions::session::ChainSignSession,
    lease: &crate::transactions::session::SessionLease,
    finalized: &crate::core::chain_submit::FinalizedChainSubmit,
    verified_snapshot: Option<&crate::core::chain_citizen_identity::FinalizedCitizenIdentity>,
) -> Result<Option<serde_json::Value>, axum::response::Response> {
    use crate::registry::citizens::occupy::{
        PURPOSE_CITIZEN_ADMIN_REBIND, PURPOSE_CITIZEN_IDENTITY_PUSH, PURPOSE_CITIZEN_OCCUPY,
        PURPOSE_CITIZEN_REVOKE,
    };
    let cid_number = session
        .context
        .get("cid_number")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let tx_hash = finalized.tx_hash.clone();
    let extra_snapshot;
    let finalized_cid_snapshot = if verified_snapshot.is_none()
        && matches!(
            session.purpose.as_str(),
            PURPOSE_CITIZEN_REVOKE | PURPOSE_CITIZEN_IDENTITY_PUSH
        ) {
        extra_snapshot = match crate::core::chain_citizen_identity::read_citizen_identity_at(
            &cid_number,
            Some(finalized.block_hash),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(err) => {
                tracing::error!(error = %err, tx_hash = %tx_hash, "read finalized citizen snapshot failed");
                return Err(api_error(
                    StatusCode::BAD_GATEWAY,
                    2004,
                    "交易已 finalized，但公民绑定快照读取失败",
                ));
            }
        };
        Some(&extra_snapshot)
    } else {
        verified_snapshot
    };
    crate::transactions::handler::commit_projection(db, session, lease, &tx_hash, |tx| {
        let mut citizen_output = None;
        match session.purpose.as_str() {
            PURPOSE_CITIZEN_OCCUPY => {
                let snapshot = finalized_cid_snapshot.ok_or("missing finalized CID snapshot")?;
                let validated: ValidatedCitizenInput = serde_json::from_value(
                    session
                        .context
                        .get("validated")
                        .cloned()
                        .ok_or("validated citizen context missing")?,
                )
                .map_err(|e| e.to_string())?;
                let record = persist_citizen_record(
                    tx,
                    &session.account_id,
                    &validated,
                    &cid_number,
                    &tx_hash,
                    snapshot,
                )?;
                citizen_output = Some(
                    serde_json::to_value(create_output_from_record(record))
                        .map_err(|e| e.to_string())?,
                );
            }
            PURPOSE_CITIZEN_REVOKE => {
                let snapshot = finalized_cid_snapshot.ok_or("missing finalized CID snapshot")?;
                Db::mark_citizen_revoked_conn(
                    tx,
                    &cid_number,
                    &session.account_id,
                    &tx_hash,
                    snapshot
                        .binding_revision
                        .ok_or("finalized binding revision missing")?,
                    snapshot.finalized_block_number,
                    &format!("0x{}", hex::encode(snapshot.finalized_block_hash)),
                )?;
            }
            PURPOSE_CITIZEN_IDENTITY_PUSH | PURPOSE_CITIZEN_ADMIN_REBIND => {
                let snapshot = finalized_cid_snapshot.ok_or("missing finalized CID snapshot")?;
                Db::confirm_citizen_identity_onchain_conn(
                    tx,
                    &cid_number,
                    &session.account_id,
                    &tx_hash,
                    snapshot,
                )?;
            }
            crate::registry::admins::governance::PURPOSE_INSTITUTION_REGISTER_ADMINS => {}
            _ => return Err("unknown chain sign purpose".into()),
        }
        Ok(citizen_output)
    })
    .map_err(|error| {
        tracing::error!(error = %error, "finalized business projection retained for recovery");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "链上交易已完成，本地投影尚未完成，会话已保留",
        )
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod chain_projection_tests {
    use super::*;
    use crate::transactions::handler::origin_matches_binding;
    use crate::transactions::handler::tests::chain_session_with_context;
    use crate::transactions::session::SessionLease;
    use chrono::Duration;
    #[tokio::test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    async fn recovery_lease_and_projection_commit_survive_revocation_failure_and_retry() {
        use crate::authentication::{login::NodeInstitutionBinding, repo};
        use crate::core::chain_citizen_identity::{FinalizedCidStatus, FinalizedCitizenIdentity};
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let owner = format!(
            "GZ018-CREG{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
        );
        let citizen = format!(
            "CN000-CTZN{}-100000000-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
        );
        let binding = NodeInstitutionBinding {
            binding_id: "original".into(),
            candidate_id: "one-institution".into(),
            institution_code: "CREG".into(),
            institution_cid_number: owner.clone(),
            bound_account_id: format!("0x{}", "11".repeat(32)),
            bound_at: Utc::now(),
            status: "ACTIVE".into(),
        };
        sandbox
            .db
            .with_client(|conn| repo::upsert_active_node_binding_conn(conn, &binding))
            .unwrap();
        let mut session = chain_session_with_context(serde_json::json!({
            "origin_binding_id":binding.binding_id,"origin_institution_cid":owner,"origin_institution_code":"CREG",
            "cid_number":citizen,"finalized_tx_hash":format!("0x{}","55".repeat(32)),"finalized_block_number":7,"finalized_block_hash":format!("0x{}","22".repeat(32)),
            "validated":{"cidType":"CTZN","familyName":"","givenName":"","citizenSex":"","citizenBirthDate":"","provinceName":"贵州省","cityName":"绥阳市","provinceCode":"GZ","cityCode":"018","townCode":"","birthProvinceCode":"","birthCityCode":"","birthTownCode":"","votingEligible":false}
        }));
        session.purpose = crate::registry::citizens::occupy::PURPOSE_CITIZEN_OCCUPY.into();
        session.call_data = vec![
            crate::registry::citizens::occupy::CITIZEN_IDENTITY_PALLET_INDEX,
            crate::registry::citizens::occupy::OCCUPY_CID_CALL_INDEX,
        ];
        session.expires_at = Utc::now() - Duration::hours(1); // 已提交事务恢复不受签名会话到期阻断。
        sandbox.db.with_client(|conn| {
        conn.execute("INSERT INTO signing(request_id,sign_purpose,account_id,call_data,nonce_value,signing_hash,context_json,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", &[&session.request_id,&session.purpose,&session.account_id,&hex::encode(&session.call_data),&i64::from(session.nonce),&session.signing_hash,&session.context,&session.expires_at]).unwrap();
        // 没有当前管理员行，恢复依然只按已确认会话的原机构完成，不能用于发新交易。
        assert_eq!(conn.query_one("SELECT COUNT(*) FROM admin",&[]).unwrap().get::<_,i64>(0),0);
        Ok(())
    }).unwrap();
        let lease = SessionLease::acquire(&sandbox.db, &session.request_id)
            .unwrap()
            .unwrap();
        assert!(SessionLease::acquire(&sandbox.db, &session.request_id)
            .unwrap()
            .is_none());
        let snapshot = FinalizedCitizenIdentity {
            genesis_hash: [1; 32],
            finalized_block_hash: [0x22; 32],
            finalized_block_number: 7,
            chain_now_seconds: 1_700_000_000,
            cid_status: FinalizedCidStatus::Active,
            registrar_cid_number: Some(binding.institution_cid_number.as_bytes().to_vec()),
            commitment: Some([3; 32]),
            residence_province_code: b"GZ".to_vec(),
            residence_city_code: b"018".to_vec(),
            account_id: Some([0x33; 32]),
            binding_revision: Some(1),
            identity_version: 0,
            voting: None,
            candidate: None,
        };
        let mut finalized = crate::core::chain_submit::FinalizedChainSubmit {
            tx_hash: "different-finalized-hash".into(),
            block_hash: [0x22; 32],
            block_number: 7,
        };
        assert!(super::commit_module_projection(
            &sandbox.db,
            &session,
            &lease,
            &finalized,
            Some(&snapshot)
        )
        .await
        .is_err());
        assert!(sandbox.db.find_citizen_by_cid(&citizen).unwrap().is_none()); // 消费失败须回滚已写的公民/ID。
        let hash = format!("0x{}", "55".repeat(32));
        finalized.tx_hash = hash.clone();
        let output = super::commit_module_projection(
            &sandbox.db,
            &session,
            &lease,
            &finalized,
            Some(&snapshot),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output["id"].as_u64().unwrap() > 0);
        assert!(sandbox
            .db
            .find_chain_sign_session(&session.request_id)
            .unwrap()
            .unwrap()
            .consumed_at
            .is_some());
        assert!(super::commit_module_projection(
            &sandbox.db,
            &session,
            &lease,
            &finalized,
            Some(&snapshot)
        )
        .await
        .is_err());
        assert_eq!(
            sandbox
                .db
                .find_citizen_by_cid(&citizen)
                .unwrap()
                .unwrap()
                .id,
            output["id"].as_u64().unwrap()
        );
        assert!(origin_matches_binding(&session, &binding));
        let other = NodeInstitutionBinding {
            institution_cid_number: "different-institution".into(),
            ..binding.clone()
        };
        assert!(!origin_matches_binding(&session, &other));
        let fresh = chain_session_with_context(serde_json::json!({}));
        assert!(!origin_matches_binding(&fresh, &binding));
    }
}
