//! 清算行机构身份只读查询 Tauri 命令。
//!
//!
//! - 本文件只面向清算行流程需要的机构身份只读:候选搜索、机构详情、提案分页。
//! - 机构创建归 onchina 控制台,节点不再承接 propose_create_institution 构建/提交。

use tauri::AppHandle;

use super::types::{EligibleClearingBankCandidate, InstitutionDetail, InstitutionProposalPage};
use crate::home;

/// 搜索资格白名单内的清算行候选机构(包含未激活,供"添加清算行"页选择)。
#[tauri::command]
pub async fn search_eligible_clearing_banks(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<EligibleClearingBankCandidate>, String> {
    let limit = limit.unwrap_or(20);
    tauri::async_runtime::spawn_blocking(move || {
        super::cid::search_eligible_clearing_banks(&query, limit)
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
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询链上数据".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::chain::fetch_institution_detail(&cid_number)
    })
    .await
    .map_err(|e| format!("fetch_clearing_bank_institution_detail task failed:{e}"))?
}

/// 机构提案分页查询。本阶段返回空列表占位。
#[tauri::command]
pub async fn fetch_clearing_bank_institution_proposals(
    app: AppHandle,
    cid_number: String,
    start_id: u64,
    page_size: u32,
) -> Result<InstitutionProposalPage, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询链上数据".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::chain::fetch_institution_proposals(&cid_number, start_id, page_size)
    })
    .await
    .map_err(|e| format!("fetch_clearing_bank_institution_proposals task failed:{e}"))?
}
