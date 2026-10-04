// 治理模块入口：注册 Tauri 命令，聚合机构数据。

// Tauri IPC 命令参数对应既有扫码签名载荷，不能在 Node 内改成新的封装对象。
#![allow(clippy::too_many_arguments)]

pub(crate) mod balance_watch;
pub(crate) mod chain_query;
pub(crate) mod institution;
pub mod proposal;
pub(crate) mod registry;
pub mod runtime_upgrade;
pub mod signing;
pub(crate) mod storage_keys;
pub mod types;

use crate::home;
use registry::InstitutionRef;
use types::{GovernanceOverview, InstitutionBalanceUpdate, InstitutionDetail, InstitutionType};

use tauri::AppHandle;

fn internal_threshold(org_type: InstitutionType) -> u32 {
    // 阈值单一真源 = primitives::count_const，桌面端不再硬编码。
    use primitives::count_const::{
        NRC_INTERNAL_THRESHOLD, PRB_INTERNAL_THRESHOLD, PRC_INTERNAL_THRESHOLD,
    };
    match org_type {
        InstitutionType::Nrc => NRC_INTERNAL_THRESHOLD,
        InstitutionType::Prc => PRC_INTERNAL_THRESHOLD,
        InstitutionType::Prb => PRB_INTERNAL_THRESHOLD,
    }
}

fn joint_vote_weight(org_type: InstitutionType) -> u32 {
    // 联合投票票数单一真源 = primitives::count_const。
    use primitives::count_const::{
        NRC_JOINT_VOTE_WEIGHT, PRB_JOINT_VOTE_WEIGHT, PRC_JOINT_VOTE_WEIGHT,
    };
    match org_type {
        InstitutionType::Nrc => NRC_JOINT_VOTE_WEIGHT,
        InstitutionType::Prc => PRC_JOINT_VOTE_WEIGHT,
        InstitutionType::Prb => PRB_JOINT_VOTE_WEIGHT,
    }
}

#[derive(Default)]
struct InstitutionBalances {
    balance_fen: Option<String>,
    staking_balance_fen: Option<String>,
    fee_balance_fen: Option<String>,
    cb_fee_balance_fen: Option<String>,
    nrc_fee_balance_fen: Option<String>,
    safety_fund_balance_fen: Option<String>,
}

struct ChainQueryContext {
    running: bool,
    block_hash: Option<String>,
    warnings: Vec<String>,
}

fn join_warnings(warnings: Vec<String>) -> Option<String> {
    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("；"))
    }
}

fn build_chain_query_context(app: &AppHandle) -> Result<ChainQueryContext, String> {
    let status = home::current_status(app)?;
    if !status.running {
        return Ok(ChainQueryContext {
            running: false,
            block_hash: None,
            warnings: vec!["节点未运行，无法查询链上数据".to_string()],
        });
    }

    let mut warnings = Vec::new();
    let block_hash = match chain_query::fetch_finalized_head() {
        Ok(hash) => Some(hash),
        Err(e) => {
            warnings.push(format!("查询最新区块失败: {e}"));
            None
        }
    };
    Ok(ChainQueryContext {
        running: true,
        block_hash,
        warnings,
    })
}

fn load_balance_at_block(
    account_id: &str,
    block_hash: Option<&str>,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let hash = block_hash?;

    match institution::fetch_balance_at(account_id, Some(hash)) {
        Ok(balance) => balance.map(|value| value.to_string()),
        Err(e) => {
            warnings.push(format!("查询{label}失败: {e}"));
            None
        }
    }
}

fn collect_admins(
    cid_number: &str,
    block_hash: Option<&str>,
    warnings: &mut Vec<String>,
) -> Vec<types::AdminInfo> {
    let admins = match institution::fetch_institution_admins(cid_number) {
        Ok(items) => items,
        Err(e) => {
            warnings.push(format!("查询管理员失败: {e}"));
            Vec::new()
        }
    };

    admins
        .into_iter()
        .map(|admin| {
            let balance_fen = block_hash
                .and_then(|hash| institution::fetch_balance_at(&admin.account_id, Some(hash)).ok())
                .flatten()
                .map(|value| value.to_string());
            types::AdminInfo {
                account_id: admin.account_id,
                family_name: admin.family_name,
                given_name: admin.given_name,
                assignments: admin.assignments,
                balance_fen,
            }
        })
        .collect()
}

fn collect_institution_balances(
    entry: InstitutionRef,
    block_hash: Option<&str>,
    warnings: &mut Vec<String>,
) -> InstitutionBalances {
    let main_account_id = entry.main_account_id();
    let mut balances = InstitutionBalances {
        balance_fen: load_balance_at_block(&main_account_id, block_hash, "主账户余额", warnings),
        ..InstitutionBalances::default()
    };

    match entry {
        InstitutionRef::Nrc(_) => {
            let fee_account_id = entry.fee_account_id();
            let Some(safety_fund_account_id) = entry.safety_fund_account_id() else {
                warnings.push("国家储委会安全基金账户 AccountId 缺失".to_string());
                return balances;
            };
            balances.nrc_fee_balance_fen =
                load_balance_at_block(&fee_account_id, block_hash, "费用账户余额", warnings);
            balances.safety_fund_balance_fen = load_balance_at_block(
                &safety_fund_account_id,
                block_hash,
                "安全基金账户余额",
                warnings,
            );
        }
        InstitutionRef::Prc(_) => {
            let fee_account_id = entry.fee_account_id();
            balances.cb_fee_balance_fen =
                load_balance_at_block(&fee_account_id, block_hash, "费用账户余额", warnings);
        }
        InstitutionRef::Prb(_) => {
            let Some(stake_account_id) = entry.stake_account_id() else {
                warnings.push("省储行永久质押账户 AccountId 缺失".to_string());
                return balances;
            };
            let fee_account_id = entry.fee_account_id();
            balances.staking_balance_fen =
                load_balance_at_block(&stake_account_id, block_hash, "永久质押账户余额", warnings);
            balances.fee_balance_fen =
                load_balance_at_block(&fee_account_id, block_hash, "费用账户余额", warnings);
        }
    }

    balances
}

fn build_institution_detail_sync(
    app: &AppHandle,
    cid_number: &str,
) -> Result<InstitutionDetail, String> {
    let entry = registry::find_institution(cid_number)
        .ok_or_else(|| format!("未知的机构 cidNumber: {cid_number}"))?;
    let org_type = entry.org_type();
    let mut context = build_chain_query_context(app)?;
    let admins = if context.running {
        collect_admins(
            cid_number,
            context.block_hash.as_deref(),
            &mut context.warnings,
        )
    } else {
        Vec::new()
    };
    let balances =
        collect_institution_balances(entry, context.block_hash.as_deref(), &mut context.warnings);
    let (
        stake_account_id,
        fee_account_id,
        cb_fee_account_id,
        nrc_fee_account_id,
        safety_fund_account_id,
    ) = match entry {
        InstitutionRef::Nrc(_) => (
            None,
            None,
            None,
            Some(entry.fee_account_id()),
            entry.safety_fund_account_id(),
        ),
        InstitutionRef::Prc(_) => (None, None, Some(entry.fee_account_id()), None, None),
        InstitutionRef::Prb(_) => (
            entry.stake_account_id(),
            Some(entry.fee_account_id()),
            None,
            None,
            None,
        ),
    };

    Ok(InstitutionDetail {
        cid_full_name: entry.cid_full_name().to_string(),
        cid_short_name: entry.cid_short_name().to_string(),
        cid_full_name_en: entry.cid_full_name_en().to_string(),
        cid_short_name_en: entry.cid_short_name_en().to_string(),
        cid_number: cid_number.to_string(),
        org_type: org_type as u8,
        org_type_label: org_type.label().to_string(),
        main_account_id: entry.main_account_id(),
        balance_fen: balances.balance_fen,
        admins,
        internal_threshold: internal_threshold(org_type),
        joint_vote_weight: joint_vote_weight(org_type),
        stake_account_id,
        staking_balance_fen: balances.staking_balance_fen,
        fee_account_id,
        fee_balance_fen: balances.fee_balance_fen,
        cb_fee_account_id,
        cb_fee_balance_fen: balances.cb_fee_balance_fen,
        nrc_fee_account_id,
        nrc_fee_balance_fen: balances.nrc_fee_balance_fen,
        safety_fund_account_id,
        safety_fund_balance_fen: balances.safety_fund_balance_fen,
        warning: join_warnings(context.warnings),
    })
}

pub(super) fn build_institution_balance_update_sync(
    app: &AppHandle,
    cid_number: &str,
) -> Result<InstitutionBalanceUpdate, String> {
    let entry = registry::find_institution(cid_number)
        .ok_or_else(|| format!("未知的机构 cidNumber: {cid_number}"))?;
    let mut context = build_chain_query_context(app)?;
    let balances =
        collect_institution_balances(entry, context.block_hash.as_deref(), &mut context.warnings);

    Ok(InstitutionBalanceUpdate {
        cid_number: cid_number.to_string(),
        balance_fen: balances.balance_fen,
        staking_balance_fen: balances.staking_balance_fen,
        fee_balance_fen: balances.fee_balance_fen,
        cb_fee_balance_fen: balances.cb_fee_balance_fen,
        nrc_fee_balance_fen: balances.nrc_fee_balance_fen,
        safety_fund_balance_fen: balances.safety_fund_balance_fen,
        warning: join_warnings(context.warnings),
    })
}

/// 获取治理首页机构分类列表（直接读取 runtime 常量，不依赖节点运行）。
#[tauri::command]
pub async fn get_governance_overview() -> Result<GovernanceOverview, String> {
    Ok(registry::governance_overview())
}

/// 获取指定机构的详细信息（地址来自 runtime 常量，金额来自链上 finalized 快照）。
#[tauri::command]
pub async fn get_institution_detail(
    app: AppHandle,
    cid_number: String,
) -> Result<InstitutionDetail, String> {
    tauri::async_runtime::spawn_blocking(move || build_institution_detail_sync(&app, &cid_number))
        .await
        .map_err(|e| format!("institution detail task failed: {e}"))?
}

/// 获取提案分页列表（需要节点运行）。
#[tauri::command]
pub async fn get_proposal_page(
    app: AppHandle,
    start_id: u64,
    count: u32,
) -> Result<proposal::ProposalPageResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || proposal::fetch_proposal_page(start_id, count))
        .await
        .map_err(|e| format!("proposal page task failed: {e}"))?
}

/// 获取单个提案完整信息（需要节点运行）。
#[tauri::command]
pub async fn get_proposal_detail(
    app: AppHandle,
    proposal_id: u64,
) -> Result<proposal::ProposalFullInfo, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || proposal::fetch_proposal_full(proposal_id))
        .await
        .map_err(|e| format!("proposal detail task failed: {e}"))?
}

/// 获取 NextProposalId（需要节点运行）。
#[tauri::command]
pub async fn get_next_proposal_id(app: AppHandle) -> Result<u64, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案 ID".to_string());
    }
    tauri::async_runtime::spawn_blocking(proposal::fetch_next_proposal_id)
        .await
        .map_err(|e| format!("next proposal id task failed: {e}"))?
}

/// 获取机构活跃提案 ID 列表（需要节点运行）。
#[tauri::command]
pub async fn get_institution_proposals(
    app: AppHandle,
    cid_number: String,
) -> Result<Vec<proposal::ProposalListItem>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let ids = proposal::fetch_active_proposal_ids(&cid_number)?;
        let mut items = Vec::new();
        for id in ids.iter().rev() {
            if let Ok(page) = proposal::fetch_proposal_page(*id, 1) {
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
) -> Result<proposal::ProposalPageResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        proposal::fetch_institution_proposal_page(&cid_number, start_id, count)
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
) -> Result<Option<proposal::ProposalDisplayMeta>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询展示号".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || proposal::fetch_proposal_display_id(proposal_id))
        .await
        .map_err(|e| format!("proposal display task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByCode[institutionCode]` 下所有 proposal_id。
#[tauri::command]
pub async fn list_proposals_by_institution(
    app: AppHandle,
    institution_code: String,
) -> Result<Vec<u64>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        proposal::fetch_proposals_by_institution_code(&institution_code)
    })
    .await
    .map_err(|e| format!("proposals by org task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByCid[cidNumber]` 下所有 proposal_id。
#[tauri::command]
pub async fn list_proposals_by_cid(app: AppHandle, cid_number: String) -> Result<Vec<u64>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || proposal::fetch_proposals_by_cid(&cid_number))
        .await
        .map_err(|e| format!("proposals by cid task failed: {e}"))?
}

/// 反向索引:列出 `ProposalsByOwner[module_tag]` 下所有 proposal_id。
/// `module_tag` 是 BoundedVec<u8> 的 SCALE 编码字节(Compact<len> + bytes)。
#[tauri::command]
pub async fn list_proposals_by_owner(
    app: AppHandle,
    module_tag_scale_hex: String,
) -> Result<Vec<u64>, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询反向索引".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = hex::decode(module_tag_scale_hex.trim_start_matches("0x"))
            .map_err(|e| format!("module_tag hex 解析失败: {e}"))?;
        proposal::fetch_proposals_by_owner(&bytes)
    })
    .await
    .map_err(|e| format!("proposals by owner task failed: {e}"))?
}

/// 构建投票签名请求 QR JSON（需要节点运行）。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_vote_request(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    voter_role_code: Option<String>,
    approve: bool,
) -> Result<signing::VoteSignRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        signing::build_vote_sign_request(
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
#[tauri::command(rename_all = "snake_case")]
pub async fn build_joint_vote_request(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    cid_number: String,
    voter_role_code: String,
    approve: bool,
) -> Result<signing::VoteSignRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        signing::build_joint_vote_sign_request(
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
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_vote(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    call_data_hex: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<signing::VoteSubmitResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交投票".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data =
            hex::decode(&call_data_hex).map_err(|e| format!("call_data 解码失败: {e}"))?;
        signing::verify_and_submit(
            &request_id,
            &expected_signer_public_key,
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
#[tauri::command(rename_all = "snake_case")]
pub async fn check_vote_status(
    app: AppHandle,
    proposal_id: u64,
    signer_public_key: String,
    cid_number: Option<String>,
    voter_role_code: Option<String>,
) -> Result<proposal::UserVoteStatus, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法查询投票状态".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        proposal::fetch_user_vote_status(
            proposal_id,
            &signer_public_key,
            cid_number.as_deref(),
            voter_role_code.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("check vote status task failed: {e}"))?
}
