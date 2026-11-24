//! 读取准确机构的实体岗位与任职，账户余额仍由原独立入口提供。
use crate::types::{EligibleClearingBankCandidate, InstitutionDetail, InstitutionProposalPage};
use std::collections::BTreeMap;
use tauri::AppHandle;
#[tauri::command]
pub async fn get_institution_roles(
    app: AppHandle,
    cid_number: String,
    expected_institution_code: Option<String>,
) -> Result<Option<crate::roles::InstitutionRolesState>, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，不能核验岗位任职".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = crate::storage::fetch_roles(&cid_number)?;
        if let Some(state) = &state {
            if expected_institution_code.as_deref().is_some_and(|code| {
                primitives::cid::code::institution_code_from_str(code)
                    != Some(state.institution_code)
            }) {
                return Err("请求机构码与链上实体不一致".into());
            }
        }
        Ok(state)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 批量读取账户 finalized free 余额。
///
/// 余额只做展示,余额读取必须钉 finalized 块,并且同一批账户共用
/// 同一个 finalized hash,避免列表内不同卡片落在不同块高。
#[tauri::command]
pub async fn get_account_balances(
    app: AppHandle,
    account_ids: Vec<String>,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询账户余额".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let block_hash = nodeclient::query::fetch_finalized_head()?;
        let mut balances = BTreeMap::new();
        for raw in account_ids {
            let account_id = crate::account::normalize_account_id(raw.as_str())?;
            let value =
                crate::balance::fetch_balance_at(account_id.as_str(), Some(block_hash.as_str()))?
                    .map(|fen| fen.to_string());
            balances.insert(account_id, value);
        }
        Ok(balances)
    })
    .await
    .map_err(|e| format!("account balances task failed: {e}"))?
}

/// 搜索资格白名单内的清算行候选机构(包含未激活,供"添加清算行"页选择)。
#[tauri::command]
pub async fn search_eligible_clearing_banks(
    app: AppHandle,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<EligibleClearingBankCandidate>, String> {
    let limit = limit.unwrap_or(20);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cid::search_eligible_clearing_banks(
            &query,
            limit,
            nodedesktop::cid::deployment(&app)?,
        )
    })
    .await
    .map_err(|e| format!("search_eligible_clearing_banks task failed:{e}"))?
}

/// 链上查询某机构的多签信息。返回 `None` = 该 cid_number 链上尚未创建机构。
#[tauri::command]
pub async fn fetch_clearing_bank_institution_detail(
    app: AppHandle,
    cid_number: String,
) -> Result<Option<InstitutionDetail>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询链上数据".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::chain::fetch_institution_detail(&cid_number)
    })
    .await
    .map_err(|e| format!("fetch_clearing_bank_institution_detail task failed:{e}"))?
}

/// 机构提案分页查询。本阶段返回空列表占位。
#[tauri::command]
pub async fn fetch_clearing_bank_institution_proposals(
    app: AppHandle,
    cid_number: String,
    start_id: String,
    page_size: u32,
) -> Result<InstitutionProposalPage, String> {
    let start_id = start_id
        .parse::<u128>()
        .map_err(|_| "提案 ID 超出 u128 范围")?;
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询链上数据".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::chain::fetch_institution_proposals(&cid_number, start_id, page_size)
    })
    .await
    .map_err(|e| format!("fetch_clearing_bank_institution_proposals task failed:{e}"))?
}
