use primitives::cid::code::{code_bytes, is_personal_code, InstitutionCode};
use std::collections::BTreeMap;
use tauri::AppHandle;

use crate::{
    governance::{chain_query, institution},
    home,
};

use super::{
    account_id, storage,
    types::{institution_code_label, is_valid_institution_code, InstitutionAdminsState},
};

/// 把前端传入的机构码字符串(如 "NRC"/"CGOV")转成链上 [u8;4]。空串/缺省 → None。
fn parse_expected_code(expected: Option<&str>) -> Option<InstitutionCode> {
    expected
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(code_bytes)
}

fn validate_cid_lookup(
    expected_code: Option<InstitutionCode>,
    cid_number: Option<&str>,
) -> Result<(), String> {
    let has_cid = cid_number
        .map(|item| !item.trim().is_empty())
        .unwrap_or(false);
    if let Some(code) = expected_code {
        if !is_valid_institution_code(&code) {
            return Err("机构码非法".to_string());
        }
        if is_personal_code(&code) {
            return Err("Node 桌面端不管理个人多签管理员".to_string());
        }
    }
    if !has_cid {
        return Err("必须提供 cidNumber".to_string());
    }
    Ok(())
}

fn ensure_expected_code(
    state: InstitutionAdminsState,
    expected_code: Option<InstitutionCode>,
) -> Result<InstitutionAdminsState, String> {
    if let Some(code) = expected_code {
        if state.institution_code != code {
            return Err(format!(
                "管理员账户机构码不匹配：请求 {}，链上 {}",
                institution_code_label(&code),
                institution_code_label(&state.institution_code)
            ));
        }
    }
    Ok(state)
}

/// 获取机构管理员集合状态。
#[tauri::command(rename_all = "snake_case")]
pub async fn get_institution_admins_state(
    app: AppHandle,
    cid_number: String,
    expected_institution_code: Option<String>,
) -> Result<Option<InstitutionAdminsState>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询机构管理员".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let expected_code = parse_expected_code(expected_institution_code.as_deref());
        validate_cid_lookup(expected_code, Some(&cid_number))?;
        let state = storage::fetch_institution_admins_state_by_cid_number(&cid_number)?;
        match state {
            Some(state) => ensure_expected_code(state, expected_code).map(Some),
            None => Ok(None),
        }
    })
    .await
    .map_err(|e| format!("institution admins task failed: {e}"))?
}

/// 批量读取管理员账户 finalized free 余额。
///
/// 管理员卡片只做展示,余额读取必须钉 finalized 块,并且同一批账户共用
/// 同一个 finalized hash,避免列表内不同卡片落在不同块高。
#[tauri::command(rename_all = "snake_case")]
pub async fn get_account_balances(
    app: AppHandle,
    account_ids: Vec<String>,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询管理员余额".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let block_hash = chain_query::fetch_finalized_head()?;
        let mut balances = BTreeMap::new();
        for raw in account_ids {
            let account_id = account_id::normalize_account_id(raw.as_str())?;
            let value =
                institution::fetch_balance_at(account_id.as_str(), Some(block_hash.as_str()))?
                    .map(|fen| fen.to_string());
            balances.insert(account_id, value);
        }
        Ok(balances)
    })
    .await
    .map_err(|e| format!("admin balances task failed: {e}"))?
}
