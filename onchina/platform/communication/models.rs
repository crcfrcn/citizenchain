// 平台通用 DTO 与现有行映射；本次仅按功能移动定义。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganizationCaCertificateInfoView {
    filename: &'static str,
    sha256: String,
    subject: &'static str,
    valid_until: String,
}

#[derive(Clone, Copy)]
pub struct DbPageCursor {
    pub created_at: DateTime<Utc>,
    pub id: i64,
}

fn encode_db_page_cursor(created_at: DateTime<Utc>, id: i64) -> String {
    let raw = format!("{}|{}", created_at.timestamp_micros(), id);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

pub fn decode_db_page_cursor(cursor: Option<&str>) -> Result<Option<DbPageCursor>, String> {
    let Some(raw_cursor) = cursor.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw_cursor)
        .map_err(|_| "invalid page cursor".to_string())?;
    let text = String::from_utf8(decoded).map_err(|_| "invalid page cursor".to_string())?;
    let mut parts = text.splitn(2, '|');
    let ts_micros = parts
        .next()
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| "invalid page cursor".to_string())?;
    let id = parts
        .next()
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| "invalid page cursor".to_string())?;
    let created_at = DateTime::<Utc>::from_timestamp_micros(ts_micros)
        .ok_or_else(|| "invalid page cursor".to_string())?;
    Ok(Some(DbPageCursor { created_at, id }))
}

fn institution_category_text(category: crate::codes::InstitutionCategory) -> &'static str {
    match category {
        crate::codes::InstitutionCategory::GovInstitution => "GOV_INSTITUTION",
        crate::codes::InstitutionCategory::PrivateInstitution => "PRIVATE_INSTITUTION",
    }
}

fn institution_category_from_text(category: &str) -> Option<crate::codes::InstitutionCategory> {
    match category {
        "GOV_INSTITUTION" => Some(crate::codes::InstitutionCategory::GovInstitution),
        "PRIVATE_INSTITUTION" => Some(crate::codes::InstitutionCategory::PrivateInstitution),
        _ => None,
    }
}

pub fn page_from_rows<T: Serialize>(
    mut rows: Vec<(T, DateTime<Utc>, i64)>,
    page_size: usize,
) -> PageResult<T> {
    let has_more = rows.len() > page_size;
    if has_more {
        rows.truncate(page_size);
    }
    let next_cursor = if has_more {
        rows.last()
            .map(|(_, created_at, id)| encode_db_page_cursor(*created_at, *id))
    } else {
        None
    };
    PageResult {
        items: rows.into_iter().map(|(row, _, _)| row).collect(),
        page_size,
        next_cursor,
        has_more,
        manifest_version: None,
        catalog_status: None,
    }
}

pub fn citizen_region_names(
    province_code: Option<&str>,
    city_code: Option<&str>,
    town_code: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(province_code) = province_code.map(str::trim).filter(|code| !code.is_empty()) else {
        return (None, None, None);
    };
    crate::codes::china::area_name_by_codes(province_code, city_code, town_code)
        .map(|(province, city, town)| {
            (
                Some(province.to_string()),
                city.map(str::to_string),
                town.map(str::to_string),
            )
        })
        .unwrap_or((None, None, None))
}

fn stable_institution_cursor_id(cid_number: &str) -> i64 {
    cid_number
        .as_bytes()
        .iter()
        .fold(0i64, |acc, byte| {
            acc.wrapping_mul(131).wrapping_add(*byte as i64)
        })
        .wrapping_abs()
}

fn institution_row_from_record(
    inst: &crate::institutions::subjects::Institution,
    account_count: usize,
    creator_family_name: Option<String>,
    creator_given_name: Option<String>,
    creator_institution_code: Option<String>,
) -> crate::institutions::subjects::InstitutionListRow {
    crate::institutions::subjects::InstitutionListRow {
        status: inst.status.clone(),
        subject_property: inst.subject_property.clone(),
        cid_number: inst.cid_number.clone(),
        cid_full_name: inst.cid_full_name.clone(),
        cid_short_name: inst.cid_short_name.clone(),
        category: inst.category,
        p1: inst.p1.clone(),
        province_name: inst.province_name.clone(),
        city_name: inst.city_name.clone(),
        town_name: inst.town_name.clone(),
        institution_code: inst.institution_code.clone(),
        education_type: inst.education_type.clone(),
        private_type: inst.private_type.clone(),
        partnership_kind: inst.partnership_kind.clone(),
        has_legal_personality: inst.has_legal_personality,
        parent_cid_number: inst.parent_cid_number.clone(),
        account_count,
        created_at: inst.created_at,
        creator_family_name,
        creator_given_name,
        creator_institution_code,
    }
}

fn institution_row_from_pg_row(
    row: &postgres::Row,
) -> Result<crate::institutions::subjects::InstitutionListRow, String> {
    let category_text: String = row.get(2);
    let category = institution_category_from_text(category_text.as_str())
        .ok_or_else(|| format!("invalid institution category: {category_text}"))?;
    let account_count_i64: i64 = row.get(15);
    let creator_family_name: Option<String> = row.get(16);
    let creator_given_name: Option<String> = row.get(17);
    let creator_institution_code: Option<String> = row.get(18);
    let cid_full_name: Option<String> = row.get(19);
    let cid_short_name: Option<String> = row.get(20);
    let town_code: Option<String> = row.get(22);
    let education_type: Option<String> = row.get(23);
    // 省/市/镇名字按 code 现场从 china.sqlite 派生,库里不存名字副本(ADR-021)。
    let province_code: String = row.get(6);
    let city_code: Option<String> = row.get(7);
    let town_code_value = town_code.clone().unwrap_or_default();
    let (province_name, city_name, town_name) = crate::codes::china::area_display_names(
        province_code.as_str(),
        city_code.as_deref(),
        Some(town_code_value.as_str()),
    );
    let inst = crate::institutions::subjects::Institution {
        status: row
            .get::<_, Option<String>>(24)
            .ok_or("institution status missing")?,
        chain_status: row.get(25),
        subject_property: crate::institutions::subjects::Institution::subject_property_for_code(
            &row.get::<_, String>(8),
        )?,
        cid_number: row.get(0),
        cid_full_name,
        cid_short_name,
        category,
        p1: row.get(3),
        province_name,
        city_name,
        town_name,
        province_code,
        city_code: city_code.unwrap_or_default(),
        town_code: town_code.unwrap_or_default(),
        institution_code: row.get(8),
        education_type,
        private_type: row.get(9),
        partnership_kind: row.get(10),
        has_legal_personality: row.get(11),
        parent_cid_number: row.get(12),
        legal_representative: None,
        photo_path: None,
        photo_name: None,
        photo_mime: None,
        photo_size: None,
        creator_account_id: row.get(13),
        created_at: row.get(14),
    };
    let item = institution_row_from_record(
        &inst,
        usize::try_from(account_count_i64).unwrap_or(0),
        creator_family_name,
        creator_given_name,
        creator_institution_code,
    );
    Ok(item)
}

fn institution_from_subject_row(
    row: &postgres::Row,
) -> Result<crate::institutions::subjects::Institution, String> {
    let category_text: String = row.get(2);
    let category = institution_category_from_text(category_text.as_str())
        .ok_or_else(|| format!("invalid institution category: {category_text}"))?;
    let cid_full_name: Option<String> = row.get(15);
    let cid_short_name: Option<String> = row.get(16);
    let town_code: Option<String> = row.get(18);
    let education_type: Option<String> = row.get(19);
    // 字段顺序必须与 get_institution_with_accounts 的 SELECT 保持一致;
    // legal_representative_photo_size 是第 27 列,下标为 26,越界会在持有数据库锁时 panic。
    let legal_representative_photo_size_i64: Option<i64> = row.get(26);
    // 省/市/镇名字按 code 现场从 china.sqlite 派生,DTO 仍带名字,库里不存名字副本(ADR-021)。
    let province_code: String = row.get(6);
    let city_code: Option<String> = row.get(7);
    let town_code_value = town_code.clone().unwrap_or_default();
    let (province_name, city_name, town_name) = crate::codes::china::area_display_names(
        province_code.as_str(),
        city_code.as_deref(),
        Some(town_code_value.as_str()),
    );
    Ok(crate::institutions::subjects::Institution {
        status: row
            .get::<_, Option<String>>(28)
            .ok_or("institution status missing")?,
        chain_status: row.get(29),
        subject_property: crate::institutions::subjects::Institution::subject_property_for_code(
            &row.get::<_, String>(8),
        )?,
        cid_number: row.get(0),
        cid_full_name,
        cid_short_name,
        category,
        p1: row.get(3),
        province_name,
        city_name,
        town_name,
        province_code,
        city_code: city_code.unwrap_or_default(),
        town_code: town_code.unwrap_or_default(),
        institution_code: row.get(8),
        education_type,
        private_type: row.get(9),
        partnership_kind: row.get(10),
        has_legal_personality: row.get(11),
        parent_cid_number: row.get(12),
        legal_representative: match (
            row.get::<_, Option<String>>(20),
            row.get::<_, Option<String>>(21),
            row.get::<_, Option<String>>(22),
            row.get::<_, Option<String>>(27),
        ) {
            (Some(family_name), Some(given_name), Some(cid_number), Some(account_id)) => {
                Some(crate::institutions::subjects::model::LegalRepresentative {
                    family_name,
                    given_name,
                    cid_number,
                    account_id,
                })
            }
            (None, None, None, None) => None,
            _ => return Err("incomplete legal representative projection".to_string()),
        },
        photo_path: row.get(23),
        photo_name: row.get(24),
        photo_mime: row.get(25),
        photo_size: legal_representative_photo_size_i64.and_then(|v| u64::try_from(v).ok()),
        creator_account_id: row.get(13),
        created_at: row.get(14),
    })
}

fn offset_page_from_window<T: Serialize>(
    mut rows: Vec<T>,
    offset: usize,
    page_size: usize,
) -> PageResult<T> {
    let has_more = rows.len() > page_size;
    if has_more {
        rows.truncate(page_size);
    }
    let next_offset = offset.saturating_add(rows.len());
    PageResult {
        items: rows,
        page_size,
        next_cursor: has_more.then(|| next_offset.to_string()),
        has_more,
        manifest_version: None,
        catalog_status: None,
    }
}
