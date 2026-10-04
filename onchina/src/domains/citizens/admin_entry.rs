//! 注册局直接录入公民 handler。
//!
//! 公民由注册局管理员在办理市先录入本地档案。请求只提交公民档案字段;
//! 身份 CID、护照号、护照有效期由服务端确定性生成并落库。
//! 链账户留到链上身份推送阶段录入，并由该账户签名确认。

// Handler 直接复用统一 Axum Response；日期 expect 只消费本模块已严格校验并规范化的日期。
#![allow(clippy::result_large_err, clippy::expect_used)]

use axum::http::{HeaderMap, StatusCode};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cid::{generate_cid_number, GenerateCidInput};
use crate::core::chain_citizen_identity::FinalizedCitizenIdentity;
use crate::crypto::pubkey::{account_id_to_ss58, normalize_account_id};
use crate::domains::citizens::passport_no::{
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
pub(crate) struct AdminCreateCitizenInput {
    /// 当前注册局内的任职岗位码；与机构 CID、管理员签名钱包共同构成权限主体。
    pub(crate) actor_role_code: String,
    /// 人主体类型:CTZN(公民)/ NATP(居民)。必填,决定发号机构码,区分公民与居民。
    pub(crate) cid_type: String,
}

/// 直接录入公民返回 DTO。
#[derive(Serialize)]
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
    pub(crate) ss58_address: Option<String>,
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
    ctx: &crate::auth::login::AdminAuthContext,
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
    let scope = crate::scope::get_visible_scope(ctx);
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
    state: &AppState,
    headers: &HeaderMap,
    account_id: &str,
    v: &ValidatedCitizenInput,
    cid_number: &str,
    onchain_tx_hash: &str,
    finalized: &FinalizedCitizenIdentity,
) -> Result<CitizenRecord, axum::response::Response> {
    // 落库的账户与 revision 一律取自 finalized 链读快照,不取请求体、也不取本地投影:
    // 本地档案是链上绑定的投影,来源必须唯一。绑定无效(号未占/已吊销/无当前账户)时
    // fail-closed 直接拒,绝不落一条 account_id 为空或指向旧账户的档案。
    let Some((citizen_account_id, binding_revision)) = finalized.active_binding() else {
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            2004,
            "finalized CID 绑定无效",
        ));
    };
    // 账户与块哈希统一按 ADR-040 规范文本落库:小写 0x + 十六进制。
    let citizen_account_id = format!("0x{}", hex::encode(citizen_account_id));
    let binding_finalized_block_hash = format!("0x{}", hex::encode(finalized.finalized_block_hash));
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
    let id = match state.db.next_citizen_id() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "allocate citizen id failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "公民序号分配失败",
            ));
        }
    };

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
        binding_finalized_block_number: Some(i64::from(finalized.finalized_block_number)),
        binding_finalized_block_hash: Some(binding_finalized_block_hash),
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

    if let Err(err) = state.db.upsert_citizen_row(&record) {
        tracing::error!(error = %err, "citizen row upsert failed");
        if err.contains("duplicate key") || err.contains("already belongs") {
            return Err(api_error(
                StatusCode::CONFLICT,
                1005,
                "公民身份或护照号已存在",
            ));
        }
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "公民落库失败",
        ));
    }

    crate::core::runtime_ops::append_audit_log(
        state,
        "CITIZEN_CREATE",
        account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "passport_no": passport_no,
            "family_name": record.family_name,
            "given_name": record.given_name,
            "province_code": province_code,
            "city_code": city_code,
            "town_code": town_code,
            "birth_province_code": birth_province_code,
            "birth_city_code": birth_city_code,
            "birth_town_code": birth_town_code,
            "voting_eligible": record.voting_eligible,
            "onchain_tx_hash": onchain_tx_hash,
            "request_id": request_id_from_headers(headers),
            "actor_ip": actor_ip_from_headers(headers),
        }),
    );
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
        ss58_address: record.account_id.as_deref().and_then(account_id_to_ss58),
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
    pub(crate) ss58_address: String,
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
    let Some(ss58_address) = account_id_to_ss58(&account_id) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id 无法派生 SS58 展示地址",
        ));
    };
    Ok(ResolvedCitizenAccount {
        account_id,
        ss58_address,
    })
}

fn citizen_archive_hash(record: &CitizenRecord) -> String {
    let value = serde_json::json!({
        "cid_number": record.cid_number,
        "passport_no": record.passport_no,
        "family_name": record.family_name,
        "given_name": record.given_name,
        "citizen_sex": record.citizen_sex,
        "citizen_birth_date": record.citizen_birth_date,
        "province_code": record.province_code,
        "city_code": record.city_code,
        "town_code": record.town_code,
        "birth_province_code": record.birth_province_code,
        "birth_city_code": record.birth_city_code,
        "birth_town_code": record.birth_town_code,
        "passport_valid_from": record.passport_valid_from,
        "passport_valid_until": record.passport_valid_until,
        "voting_eligible": record.voting_eligible,
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
                    "SELECT COALESCE(id, 0), cid_number, passport_no, family_name,
                            given_name, citizen_sex, citizen_birth_date, account_id,
                            binding_revision, binding_finalized_block_number,
                            binding_finalized_block_hash, citizen_status, voting_eligible,
                            passport_valid_from, passport_valid_until, status_updated_at,
                            province_code, city_code, town_code,
                            birth_province_code, birth_city_code, birth_town_code,
                            archive_hash, onchain_tx_hash, onchain_block_number, onchain_at,
                            creator_account_id, created_at, updater_account_id, updated_at
                     FROM citizens
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

    /// 分配下一个公民自增序号。
    pub(crate) fn next_citizen_id(&self) -> Result<u64, String> {
        self.with_client(|conn| {
            let row = conn
                .query_one("SELECT COALESCE(MAX(id), 0) + 1 FROM citizens", &[])
                .map_err(|e| format!("allocate citizen id failed: {e}"))?;
            let id: i64 = row.get(0);
            Ok(u64::try_from(id).unwrap_or(1))
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
        binding_finalized_block_number: row.get(9),
        binding_finalized_block_hash: row.get(10),
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
        crate::domains::citizens::chain_identity::ensure_record_in_admin_scope(&ctx, &existing)
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

    // 护照签发:现存已签发则原样保留(锁定);现存为空且出生日期就绪时确定性签发一次。
    // 号段以最终居住省市为命名空间(与建档一致)。
    let now = Utc::now();
    let (passport_no, passport_valid_from_val, passport_valid_until_val) = if !existing
        .passport_no
        .trim()
        .is_empty()
    {
        (
            existing.passport_no.clone(),
            existing.passport_valid_from.clone(),
            existing.passport_valid_until.clone(),
        )
    } else if locked.citizen_birth_date.is_empty() {
        (String::new(), String::new(), String::new())
    } else {
        let birth = NaiveDate::parse_from_str(locked.citizen_birth_date.as_str(), "%Y-%m-%d")
            .expect("birth date validated above");
        let passport_no = match state.db.allocate_passport_no(
            existing.province_code.as_str(),
            residence_city.as_str(),
            existing.cid_number.as_str(),
        ) {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(error = %err, "allocate passport no on edit failed");
                return crate::api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "护照号签发失败");
            }
        };
        (
            passport_no,
            passport_valid_from(now),
            passport_valid_until(now, passport_validity_years(now, birth)),
        )
    };

    // ── 组装:可变字段 + 锁定身份字段 + 服务端护照;正本/链上承诺/账户/状态保留 ──
    let mut updated = existing.clone();
    updated.family_name = family_name;
    updated.given_name = given_name;
    updated.city_code = residence_city;
    updated.town_code = residence_town;
    updated.voting_eligible = input.voting_eligible;
    updated.citizen_sex = locked.citizen_sex;
    updated.citizen_birth_date = locked.citizen_birth_date;
    updated.birth_province_code = locked.birth_province_code;
    updated.birth_city_code = locked.birth_city_code;
    updated.birth_town_code = locked.birth_town_code;
    updated.passport_no = passport_no;
    updated.passport_valid_from = passport_valid_from_val;
    updated.passport_valid_until = passport_valid_until_val;
    updated.status_updated_at = Some(now.timestamp());
    updated.updated_at = now;
    updated.archive_hash = Some(citizen_archive_hash(&updated));

    if let Err(err) = state.db.upsert_citizen_row(&updated) {
        tracing::error!(error = %err, "citizen edit upsert failed");
        return crate::api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案更新失败");
    }
    axum::Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: crate::citizen_row_from_record(&updated),
    })
    .into_response()
}

#[cfg(test)]
// 公民资料编辑契约测试使用断言式解包定位不可变字段回归。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod edit_tests {
    use super::*;

    fn base_record() -> CitizenRecord {
        CitizenRecord {
            id: 1,
            cid_number: "CN220-CTZN2-198805200-2026".to_string(),
            passport_no: String::new(),
            family_name: String::new(),
            given_name: String::new(),
            citizen_sex: String::new(),
            citizen_birth_date: String::new(),
            account_id: None,
            binding_revision: 0,
            binding_finalized_block_number: None,
            binding_finalized_block_hash: None,
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
            created_at: Utc::now(),
            updater_account_id: None,
            updated_at: Utc::now(),
        }
    }

    fn input() -> AdminEditCitizenInput {
        AdminEditCitizenInput {
            family_name: "程".to_string(),
            given_name: "伟".to_string(),
            city_code: "018".to_string(),
            town_code: "018001".to_string(),
            voting_eligible: true,
            citizen_sex: "MALE".to_string(),
            citizen_birth_date: "1988-05-20".to_string(),
            birth_province_code: "GZ".to_string(),
            birth_city_code: "018".to_string(),
            birth_town_code: "018001".to_string(),
        }
    }

    #[test]
    fn empty_immutable_fields_are_initialized() {
        let locked = resolve_locked_identity(&base_record(), &input()).expect("ok");
        assert_eq!(locked.citizen_sex, "MALE");
        assert_eq!(locked.citizen_birth_date, "1988-05-20");
        assert_eq!(locked.birth_town_code, "018001");
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
