// 治理模块入口：注册 Tauri 命令，聚合机构数据。

// Tauri IPC 命令参数对应既有扫码签名载荷，不能在 Node 内改成新的封装对象。
#![allow(clippy::too_many_arguments)]
use crate::registry::InstitutionRef;
use serde::Serialize;
use tauri::AppHandle;

fn internal_threshold(org_type: InstitutionType) -> u32 {
    // 阈值单一真源 = primitives::count_const，桌面端不再硬编码。
    use primitives::count_const::{
        NRC_INTERNAL_THRESHOLD, PRB_INTERNAL_THRESHOLD, PRC_INTERNAL_THRESHOLD,
    };
    match org_type {
        crate::council::InstitutionType::Nrc => NRC_INTERNAL_THRESHOLD,
        crate::council::InstitutionType::Prc => PRC_INTERNAL_THRESHOLD,
        crate::council::InstitutionType::Prb => PRB_INTERNAL_THRESHOLD,
    }
}

fn joint_vote_weight(org_type: InstitutionType) -> u32 {
    // 联合投票票数单一真源 = primitives::count_const。
    use primitives::count_const::{
        NRC_JOINT_VOTE_WEIGHT, PRB_JOINT_VOTE_WEIGHT, PRC_JOINT_VOTE_WEIGHT,
    };
    match org_type {
        crate::council::InstitutionType::Nrc => NRC_JOINT_VOTE_WEIGHT,
        crate::council::InstitutionType::Prc => PRC_JOINT_VOTE_WEIGHT,
        crate::council::InstitutionType::Prb => PRB_JOINT_VOTE_WEIGHT,
    }
}

#[derive(Default)]
pub(crate) struct InstitutionBalances {
    balance_fen: Option<String>,
    staking_balance_fen: Option<String>,
    fee_balance_fen: Option<String>,
    cb_fee_fen: Option<String>,
    nrc_fee_fen: Option<String>,
    fund_balance_fen: Option<String>,
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
    let status = nodecontrol::identity::current_status(app)?;
    if !status.running {
        return Ok(ChainQueryContext {
            running: false,
            block_hash: None,
            warnings: vec!["节点未运行，无法查询链上数据".to_string()],
        });
    }

    let mut warnings = Vec::new();
    let block_hash = match nodeclient::query::fetch_finalized_head() {
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

    match crate::balance::fetch_balance_at(account_id, Some(hash)) {
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
) -> Vec<crate::council::AdminInfo> {
    let admins = match crate::balance::fetch_institution_admins(cid_number) {
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
                .and_then(|hash| {
                    crate::balance::fetch_balance_at(&admin.account_id, Some(hash)).ok()
                })
                .flatten()
                .map(|value| value.to_string());
            crate::council::AdminInfo {
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
        ..crate::council::InstitutionBalances::default()
    };

    match entry {
        crate::registry::InstitutionRef::Nrc(_) => {
            let fee_account_id = entry.fee_account_id();
            let Some(fund_account_id) = entry.fund_account_id() else {
                warnings.push("国家储委会安全基金账户 AccountId 缺失".to_string());
                return balances;
            };
            balances.nrc_fee_fen =
                load_balance_at_block(&fee_account_id, block_hash, "费用账户余额", warnings);
            balances.fund_balance_fen =
                load_balance_at_block(&fund_account_id, block_hash, "安全基金账户余额", warnings);
        }
        crate::registry::InstitutionRef::Prc(_) => {
            let fee_account_id = entry.fee_account_id();
            balances.cb_fee_fen =
                load_balance_at_block(&fee_account_id, block_hash, "费用账户余额", warnings);
        }
        crate::registry::InstitutionRef::Prb(_) => {
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
    let entry = crate::registry::find_institution(cid_number)
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
    let (stake_account_id, fee_account_id, cb_account_id, nrc_account_id, fund_account_id) =
        match entry {
            crate::registry::InstitutionRef::Nrc(_) => (
                None,
                None,
                None,
                Some(entry.fee_account_id()),
                entry.fund_account_id(),
            ),
            crate::registry::InstitutionRef::Prc(_) => {
                (None, None, Some(entry.fee_account_id()), None, None)
            }
            crate::registry::InstitutionRef::Prb(_) => (
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
        full_name_en: entry.full_name_en().to_string(),
        short_name_en: entry.short_name_en().to_string(),
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
        cb_account_id,
        cb_fee_fen: balances.cb_fee_fen,
        nrc_account_id,
        nrc_fee_fen: balances.nrc_fee_fen,
        fund_account_id,
        fund_balance_fen: balances.fund_balance_fen,
        warning: join_warnings(context.warnings),
    })
}

pub(crate) fn build_institution_balance_update_sync(
    app: &AppHandle,
    cid_number: &str,
) -> Result<InstitutionBalanceUpdate, String> {
    let entry = crate::registry::find_institution(cid_number)
        .ok_or_else(|| format!("未知的机构 cidNumber: {cid_number}"))?;
    let mut context = build_chain_query_context(app)?;
    let balances =
        collect_institution_balances(entry, context.block_hash.as_deref(), &mut context.warnings);

    Ok(InstitutionBalanceUpdate {
        cid_number: cid_number.to_string(),
        balance_fen: balances.balance_fen,
        staking_balance_fen: balances.staking_balance_fen,
        fee_balance_fen: balances.fee_balance_fen,
        cb_fee_fen: balances.cb_fee_fen,
        nrc_fee_fen: balances.nrc_fee_fen,
        fund_balance_fen: balances.fund_balance_fen,
        warning: join_warnings(context.warnings),
    })
}

/// 获取治理首页机构分类列表（直接读取 runtime 常量，不依赖节点运行）。
#[tauri::command]
pub async fn get_governance_overview() -> Result<GovernanceOverview, String> {
    Ok(crate::registry::governance_overview())
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

/// 机构类型枚举，数值与链上 `org` 编码一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum InstitutionType {
    /// 国家储委会 National Reserve Committee
    Nrc = 0,
    /// 省储委会 Provincial Reserve Committee
    Prc = 1,
    /// 省储行 Provincial Reserve Bank
    Prb = 2,
}

impl InstitutionType {
    pub fn label(&self) -> &'static str {
        match self {
            crate::council::InstitutionType::Nrc => "国家储委会",
            crate::council::InstitutionType::Prc => "省储委会",
            crate::council::InstitutionType::Prb => "省储行",
        }
    }
}

/// 机构管理员人员记录、全部有效岗位任职及可选链上余额。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminInfo {
    pub account_id: String,
    pub family_name: String,
    pub given_name: String,
    pub assignments: Vec<crate::admins::InstitutionRoleAssignmentInfo>,
    /// 链上余额（分），节点未运行或余额查询失败时为 null。
    pub balance_fen: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionDetail {
    /// 机构中文全称,字段唯一对齐 runtime/CID 的 `cid_full_name`。
    pub cid_full_name: String,
    /// 机构中文简称,字段唯一对齐 runtime/CID 的 `cid_short_name`。
    pub cid_short_name: String,
    /// 机构英文全称,读取当前固定 runtime/CID 接口 `full_name_en`。
    pub full_name_en: String,
    /// 机构英文简称,读取当前固定 runtime/CID 接口 `short_name_en`。
    pub short_name_en: String,
    /// 链上身份标识。
    pub cid_number: String,
    /// 机构类型：0=NRC, 1=PRC, 2=PRB。
    pub org_type: u8,
    /// 机构类型显示标签。
    pub org_type_label: String,
    /// 主账户 ID，固定为小写 `0x` + 64 位十六进制。
    pub main_account_id: String,
    /// 主账户链上余额（分），节点未运行时为 null。
    pub balance_fen: Option<String>,
    /// 管理员账户列表；每个账户携带其全部有效岗位任职。
    pub admins: Vec<AdminInfo>,
    /// 内部投票通过阈值。
    pub internal_threshold: u32,
    /// 联合投票权重。
    pub joint_vote_weight: u32,
    /// 永久质押账户 AccountId hex（仅 PRB）。
    pub stake_account_id: Option<String>,
    /// 永久质押账户余额（分，仅 PRB）。
    pub staking_balance_fen: Option<String>,
    /// 手续费账户 AccountId hex（仅 PRB）。
    pub fee_account_id: Option<String>,
    /// 手续费账户余额（分，仅 PRB）。
    pub fee_balance_fen: Option<String>,
    /// 储委会费用账户 AccountId hex（省储委会，仅 PRC）。
    pub cb_account_id: Option<String>,
    /// 储委会费用账户余额（分，仅 PRC）。
    pub cb_fee_fen: Option<String>,
    /// 国家储委会费用账户 AccountId hex（仅 NRC）。
    pub nrc_account_id: Option<String>,
    /// 国家储委会手续费账户余额（分，仅 NRC）。
    pub nrc_fee_fen: Option<String>,
    /// 国家储委会安全基金账户 AccountId hex（仅 NRC）。
    pub fund_account_id: Option<String>,
    /// 国家储委会安全基金账户余额（分，仅 NRC）。
    pub fund_balance_fen: Option<String>,
    /// 告警信息。
    pub warning: Option<String>,
}

/// 治理详情页余额更新事件，仅覆盖链上金额和告警，不改页面结构。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionBalanceUpdate {
    /// 链上身份标识，用于前端过滤当前页面事件。
    pub cid_number: String,
    /// 主账户链上余额（分）。
    pub balance_fen: Option<String>,
    /// 永久质押账户链上余额（分，仅 PRB）。
    pub staking_balance_fen: Option<String>,
    /// 费用账户链上余额（分，仅 PRB）。
    pub fee_balance_fen: Option<String>,
    /// 省储委会费用账户链上余额（分，仅 PRC）。
    pub cb_fee_fen: Option<String>,
    /// 国家储委会费用账户链上余额（分，仅 NRC）。
    pub nrc_fee_fen: Option<String>,
    /// 国家储委会安全基金账户链上余额（分，仅 NRC）。
    pub fund_balance_fen: Option<String>,
    /// 链上查询告警。
    pub warning: Option<String>,
}

/// 机构列表项，用于治理首页机构列表。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionListItem {
    /// 机构中文全称,字段唯一对齐 runtime/CID 的 `cid_full_name`。
    pub cid_full_name: String,
    /// 机构中文简称,字段唯一对齐 runtime/CID 的 `cid_short_name`。
    pub cid_short_name: String,
    /// 机构英文全称,读取当前固定 runtime/CID 接口 `full_name_en`。
    pub full_name_en: String,
    /// 机构英文简称,读取当前固定 runtime/CID 接口 `short_name_en`。
    pub short_name_en: String,
    pub cid_number: String,
    pub org_type: u8,
    pub org_type_label: String,
    /// 主账户 ID，固定为小写 `0x` + 64 位十六进制。
    pub main_account_id: String,
}

/// 治理首页聚合数据。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceOverview {
    /// 国家储委会列表。
    pub national_councils: Vec<InstitutionListItem>,
    /// 省储委会列表。
    pub provincial_councils: Vec<InstitutionListItem>,
    /// 省储行列表。
    pub provincial_banks: Vec<InstitutionListItem>,
    pub warning: Option<String>,
}
