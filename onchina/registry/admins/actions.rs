//! 注册局名册办理的原有校验与写入；平台仅保留统一安全动作调度。
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::authentication::login::AdminAuthContext;
use crate::authentication::repo;
use crate::crypto::pubkey::{normalize_account_id, same_account_id};
use super::city_registry_admins::{
    can_manage_city_registry, city_registry_row_from_user_conn,
    count_city_registry_admins_in_city_conn, ensure_city_in_province,
    find_city_registry_by_id_conn, MAX_ADMIN_PERSON_NAME_BYTES, MAX_CITY_REGISTRY_ADMINS_PER_CITY,
};
use crate::*;
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CityRegistryIdPayload {
    pub(crate) id: u64,
}

pub(crate) fn validate_create_city_registry_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    input: &CreateCityRegistryAdminInput,
) -> Result<(String, String, String, String, String), String> {
    let Some(account_id) = normalize_account_id(input.account_id.as_str()) else {
        return Err("http:bad_request:account_id format invalid".to_string());
    };
    let family_name = validate_person_name(input.family_name.as_str(), "family_name")?;
    let given_name = validate_person_name(input.given_name.as_str(), "given_name")?;
    let creator_account_id = match input.creator_account_id.as_deref().map(str::trim) {
        None | Some("") => ctx.account_id.clone(),
        Some(raw) => {
            let Some(normalized) = normalize_account_id(raw) else {
                return Err("http:bad_request:creator_account_id format invalid".to_string());
            };
            if !same_account_id(normalized.as_str(), ctx.account_id.as_str()) {
                return Err(
                    "http:forbidden:FederalRegistry can only create city registry admins under itself"
                        .to_string(),
                );
            }
            normalized
        }
    };
    if let Some(existing) = repo::get_admin_by_account_id_conn(conn, account_id.as_str())? {
        return Err(duplicate_admin_account_error(&existing.institution_code));
    }
    let province_name = ctx
        .scope_province_name
        .as_deref()
        .ok_or_else(|| "http:forbidden:admin province scope missing".to_string())?;
    let (province, city) = ensure_city_in_province(province_name, input.city_name.as_str())
        .map_err(response_to_string)?;
    if count_city_registry_admins_in_city_conn(conn, province.as_str(), city.as_str())?
        >= MAX_CITY_REGISTRY_ADMINS_PER_CITY
    {
        return Err("http:conflict:city admin city limit reached".to_string());
    }
    Ok((
        account_id,
        family_name,
        given_name,
        city,
        creator_account_id,
    ))
}

fn validate_person_name(name: &str, field: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("http:bad_request:{field} is required"));
    }
    if name.len() > MAX_ADMIN_PERSON_NAME_BYTES {
        return Err(format!("http:bad_request:{field} too long"));
    }
    Ok(name.to_string())
}

pub(crate) fn require_manageable_city_registry_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    id: u64,
) -> Result<AdminUser, String> {
    let city_registry = find_city_registry_by_id_conn(conn, id)?
        .ok_or_else(|| "http:not_found:city admin not found".to_string())?;
    if !can_manage_city_registry(ctx.scope_province_name.as_deref(), &city_registry) {
        return Err("http:forbidden:cannot manage other province city registry admins".to_string());
    }
    Ok(city_registry)
}

pub(crate) fn apply_create_city_registry_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    input: &CreateCityRegistryAdminInput,
) -> Result<serde_json::Value, String> {
    let (account_id, family_name, given_name, city, creator_account_id) =
        validate_create_city_registry_conn(conn, ctx, input)?;
    let province = ctx.scope_province_name.as_deref().ok_or("http:forbidden:admin province scope missing")?;
    let owner = super::repo::city_registry_cid_conn(conn, province, &city)?;
    let now = Utc::now();
    let row = AdminUser {
        id: repo::next_admin_id_conn(conn)?,
        account_id: account_id.clone(),
        family_name,
        given_name,
        institution_code: "CREG".to_string(),
        institution_cid_number: Some(owner),
        built_in: false,
        creator_account_id,
        created_at: now,
        updated_at: Some(now),
        city_name: city,
    };
    repo::upsert_admin_conn(conn, &row)?;
    serde_json::to_value(city_registry_row_from_user_conn(conn, &row)?)
        .map_err(|e| format!("encode city admin failed: {e}"))
}

pub(crate) fn apply_delete_city_registry_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    input: &CityRegistryIdPayload,
) -> Result<serde_json::Value, String> {
    let city_registry = require_manageable_city_registry_conn(conn, ctx, input.id)?;
    let account_id = city_registry.account_id.clone();
    repo::delete_admin_runtime_state_conn(conn, account_id.as_str())?;
    conn.execute("DELETE FROM admins WHERE account_id = $1", &[&account_id])
        .map_err(|e| format!("delete city admin failed: {e}"))?;
    Ok(json!({ "deleted": true, "account_id": account_id }))
}

fn duplicate_admin_account_error(institution_code: &str) -> String {
    if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        "http:conflict:admin account_id already exists as federal admin"
    } else {
        "http:conflict:admin account_id already exists as city admin"
    }
    .to_string()
}

fn response_to_string(_resp: axum::response::Response) -> String {
    "http:bad_request:invalid request".to_string()
}
