// 治理模块入口：注册 Tauri 命令，聚合机构数据。

// Tauri IPC 命令参数对应既有扫码签名载荷，不能在 Node 内改成新的封装对象。
#![allow(clippy::too_many_arguments)]
use tauri::AppHandle;

/// 获取提案分页列表（需要节点运行）。
#[tauri::command]
pub async fn get_proposal_page(
    app: AppHandle,
    start_id: u64,
    count: u32,
) -> Result<crate::types::ProposalPageResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || crate::query::fetch_proposal_page(start_id, count))
        .await
        .map_err(|e| format!("proposal page task failed: {e}"))?
}

/// 获取单个提案完整信息（需要节点运行）。
#[tauri::command]
pub async fn get_proposal_detail(
    app: AppHandle,
    proposal_id: u64,
) -> Result<crate::types::ProposalFullInfo, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || crate::query::fetch_proposal_full(proposal_id))
        .await
        .map_err(|e| format!("proposal detail task failed: {e}"))?
}

/// 获取 NextProposalId（需要节点运行）。
#[tauri::command]
pub async fn get_next_proposal_id(app: AppHandle) -> Result<u64, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案 ID".to_string());
    }
    tauri::async_runtime::spawn_blocking(crate::query::fetch_next_proposal_id)
        .await
        .map_err(|e| format!("next proposal id task failed: {e}"))?
}

/// 获取机构活跃提案 ID 列表（需要节点运行）。
#[tauri::command]
pub async fn get_institution_proposals(
    app: AppHandle,
    cid_number: String,
) -> Result<Vec<crate::types::ProposalListItem>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let ids = crate::query::fetch_active_proposal_ids(&cid_number)?;
        let mut items = Vec::new();
        for id in ids.iter().rev() {
            if let Ok(page) = crate::query::fetch_proposal_page(*id, 1) {
                items.extend(page.items);
            }
        }
        Ok(items)
    })
    .await
    .map_err(|e| format!("institution proposals task failed: {e}"))?
}

/// 分页查询指定机构的所有提案（需要节点运行）。
///
/// 从 start_id 倒序遍历，过滤属于该机构的提案，返回分页结果。
#[tauri::command]
pub async fn get_institution_proposal_page(
    app: AppHandle,
    cid_number: String,
    start_id: u64,
    count: u32,
) -> Result<crate::types::ProposalPageResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::query::fetch_institution_proposal_page(&cid_number, start_id, count)
    })
    .await
    .map_err(|e| format!("institution proposal page task failed: {e}"))?
}

// ──── 双层 ID 与反向索引 ────

/// 查询提案展示号 `ProposalDisplayId[id] = ProposalDisplayMeta { year, seq_in_year }`。
#[tauri::command]
pub async fn get_proposal_display(
    app: AppHandle,
    proposal_id: u64,
) -> Result<Option<crate::types::ProposalDisplayMeta>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询展示号".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::query::fetch_proposal_display_id(proposal_id)
    })
    .await
    .map_err(|e| format!("proposal display task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByCode[institutionCode]` 下所有 proposal_id。
#[tauri::command]
pub async fn list_proposals_by_institution(
    app: AppHandle,
    institution_code: String,
) -> Result<Vec<u64>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::query::fetch_proposals_by_institution_code(&institution_code)
    })
    .await
    .map_err(|e| format!("proposals by org task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByCid[cidNumber]` 下所有 proposal_id。
#[tauri::command]
pub async fn list_proposals_by_cid(app: AppHandle, cid_number: String) -> Result<Vec<u64>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || crate::query::fetch_proposals_by_cid(&cid_number))
        .await
        .map_err(|e| format!("proposals by cid task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByOwner[module_tag]` 下所有 proposal_id。
/// `module_tag` 是 BoundedVec<u8> 的 SCALE 编码字节(Compact<len> + bytes)。
#[tauri::command]
pub async fn list_proposals_by_owner(
    app: AppHandle,
    module_tag_hex: String,
) -> Result<Vec<u64>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = hex::decode(module_tag_hex.trim_start_matches("0x"))
            .map_err(|e| format!("module_tag hex 解析失败: {e}"))?;
        crate::query::fetch_proposals_by_owner(&bytes)
    })
    .await
    .map_err(|e| format!("proposals by owner task failed: {e}"))?
}

/// 构建投票签名请求 QR JSON（需要节点运行）。
#[tauri::command]
pub async fn build_vote_request(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    voter_role_code: Option<String>,
    approve: bool,
) -> Result<nodeclient::signing::VoteSignRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::vote::build_vote_sign_request(
            proposal_id,
            &signer_public_key,
            voter_role_code.as_deref(),
            approve,
        )
    })
    .await
    .map_err(|e| format!("build vote request task failed: {e}"))?
}

/// 构建联合投票签名请求 QR JSON（需要节点运行）。
#[tauri::command]
pub async fn build_joint_vote_request(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    cid_number: String,
    voter_role_code: String,
    approve: bool,
) -> Result<nodeclient::signing::VoteSignRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::vote::build_joint_vote_sign_request(
            proposal_id,
            &signer_public_key,
            &cid_number,
            &voter_role_code,
            approve,
        )
    })
    .await
    .map_err(|e| format!("build joint vote request task failed: {e}"))?
}

/// 验证签名响应并提交投票（通用，支持内部和联合投票）。
///
/// call_data_hex 为完整的 SCALE call data hex（不含 0x 前缀）。
#[tauri::command]
pub async fn submit_vote(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    call_data_hex: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<nodeclient::signing::VoteSubmitResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交投票".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data =
            hex::decode(&call_data_hex).map_err(|e| format!("call_data 解码失败: {e}"))?;
        nodeclient::signing::verify_and_submit(
            &request_id,
            &expected_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit vote task failed: {e}"))?
}

/// 查询用户投票状态（需要节点运行）。
#[tauri::command]
pub async fn check_vote_status(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    cid_number: Option<String>,
    voter_role_code: Option<String>,
) -> Result<crate::types::UserVoteStatus, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询投票状态".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::tally::fetch_user_vote_status(
            proposal_id,
            &signer_public_key,
            cid_number.as_deref(),
            voter_role_code.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("check vote status task failed: {e}"))?
}
