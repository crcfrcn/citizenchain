use nodeupgrade::summary::format_runtime_upgrade_summary;

use noderesolutionissue::summary::format_issuance_summary;

use nodedestroy::summary::format_destroy_summary;

use crate::query::resolve_proposal_action;
use crate::types::{FeeRateProposalDetail, ProposalAction, ProposalCardContent};

use crate::types::ProposalDisplayMeta;
use crate::types::ProposalMeta;

pub(crate) struct ProposalDisplayInfo {
    pub(crate) summary: String,
    pub(crate) card: Option<ProposalCardContent>,
    pub(crate) status: u8,
    pub(crate) status_label: String,
}

// ──── 工具函数 ────

/// 提案展示号格式化（双层 ID）：
///   主键 `proposal_id` 是单调 u64,与展示号解耦。展示号通过链上
///   `ProposalDisplayId[id] = ProposalDisplayMeta { year, seq_in_year }` 反查。
///
/// 渲染格式:`2026000123`(年份 + 6 位补零序号);seq 突破 6 位时自动扩展。
/// `display_meta=None` 时(理论不该发生)fallback 到 `#<u64>` 形式避免空字符串。
pub(crate) fn format_proposal_id(id: u64, display_meta: Option<&ProposalDisplayMeta>) -> String {
    match display_meta {
        Some(meta) => format!("{}{:06}", meta.year, meta.seq_in_year),
        None => format!("#{id}"),
    }
}

pub(crate) fn kind_label(kind: u8) -> &'static str {
    match kind {
        0 => "内部投票",
        1 => "联合投票",
        _ => "未知",
    }
}

pub(crate) fn stage_label(stage: u8) -> &'static str {
    match stage {
        0 => "内部阶段",
        1 => "联合阶段",
        2 => "联合公投阶段",
        _ => "未知",
    }
}

pub(crate) fn status_label(status: u8) -> &'static str {
    match status {
        0 => "投票中",
        1 => "已通过",
        2 => "已否决",
        3 => "已执行",
        4 => "执行失败",
        _ => "未知",
    }
}

/// 从机构 CID 列表反查第一个可识别的机构名称。
pub(crate) fn resolve_cid_full_name_by_subjects(subject_cid_numbers: &[String]) -> Option<String> {
    subject_cid_numbers.iter().find_map(|cid| {
        nodeinstitution::registry::find_institution(cid)
            .map(|item| item.cid_full_name().to_string())
    })
}

/// 一次解析业务动作，同时生成既有摘要和机构简介卡片字段。
///
/// 与 [`fetch_proposal_full`] 共用 [`resolve_proposal_action`],保证两条路径对同一提案看到一致内容。
pub(crate) fn fetch_proposal_display(
    proposal_id: u64,
    meta: &ProposalMeta,
) -> Result<ProposalDisplayInfo, String> {
    let action = resolve_proposal_action(proposal_id, meta)?;
    let card = proposal_card_content(&action);
    let (summary, status, status_label_s) = match action {
        crate::types::ProposalAction::Business(action) => (
            crate::business::format_summary(&action, |actor_cid_number| {
                nodeinstitution::registry::find_institution(actor_cid_number)
                    .map(|item| item.cid_full_name().to_string())
            }),
            meta.status,
            status_label(meta.status).to_string(),
        ),
        crate::types::ProposalAction::RuntimeUpgrade(d) => (
            format_runtime_upgrade_summary(&d),
            meta.status,
            status_label(meta.status).to_string(),
        ),
        crate::types::ProposalAction::ResolutionIssuance(d) => (
            format_issuance_summary(&d),
            meta.status,
            status_label(meta.status).to_string(),
        ),
        crate::types::ProposalAction::ResolutionDestroy(d) => (
            format_destroy_summary(&d),
            meta.status,
            status_label(meta.status).to_string(),
        ),
        crate::types::ProposalAction::FeeRate(d) => (
            format_fee_rate_summary(&d),
            meta.status,
            status_label(meta.status).to_string(),
        ),
        crate::types::ProposalAction::Unknown => (
            "（无详情数据）".to_string(),
            meta.status,
            status_label(meta.status).to_string(),
        ),
    };
    Ok(ProposalDisplayInfo {
        summary,
        card,
        status,
        status_label: status_label_s,
    })
}

fn proposal_card_content(action: &ProposalAction) -> Option<ProposalCardContent> {
    use crate::business::ProposalAction::TransferModule;
    use nodemultisig::types::ProposalAction as TransferAction;

    let (action_label, amount, remark) = match action {
        ProposalAction::Business(action) => match action.as_ref() {
            TransferModule(transfer) => match transfer.as_ref() {
                TransferAction::Transfer(detail) => {
                    ("转账", Some(&detail.amount_fen), Some(&detail.remark))
                }
                TransferAction::SafetyFund(detail) => (
                    "安全基金转账",
                    Some(&detail.amount_fen),
                    Some(&detail.remark),
                ),
                TransferAction::Sweep(detail) => ("手续费划转", Some(&detail.amount_fen), None),
            },
        },
        ProposalAction::RuntimeUpgrade(_) => ("协议升级", None, None),
        ProposalAction::ResolutionIssuance(detail) => {
            ("决议发行", Some(&detail.total_amount_fen), None)
        }
        ProposalAction::ResolutionDestroy(detail) => ("决议销毁", Some(&detail.amount_fen), None),
        ProposalAction::FeeRate(_) => ("费率设置", None, None),
        ProposalAction::Unknown => return None,
    };

    Some(ProposalCardContent {
        action_label,
        amount_fen: amount.cloned(),
        remark: remark.cloned(),
    })
}

#[cfg(test)]
#[test]
fn transfer_card_preserves_the_complete_remark() {
    use nodemultisig::types::{ProposalAction as TransferAction, TransferProposalDetail};

    let remark = "这是备注中的机构名称，由提案人填写。\n".repeat(4);
    let action =
        ProposalAction::Business(Box::new(crate::business::ProposalAction::TransferModule(
            Box::new(TransferAction::Transfer(Box::new(TransferProposalDetail {
                proposal_id: 0,
                actor_cid_number: Some("FIXTURE-CID".into()),
                funding_account_id: String::new(),
                beneficiary_account_id: String::new(),
                amount_fen: "100000".into(),
                remark: remark.clone(),
                proposer_account_id: String::new(),
            }))),
        )));
    let card = proposal_card_content(&action).unwrap();
    assert_eq!(
        serde_json::to_value(card).unwrap(),
        serde_json::json!({"actionLabel": "转账", "amountFen": "100000", "remark": remark})
    );
}

pub(crate) fn format_fee_rate_summary(d: &FeeRateProposalDetail) -> String {
    let rate_percent = format!("{:.2}%", d.new_rate_bp as f64 / 100.0);
    let inst_name = nodeinstitution::registry::find_institution(&d.actor_cid_number)
        .map(|item| item.cid_full_name().to_string())
        .unwrap_or_else(|| {
            if d.actor_cid_number.is_empty() {
                "未知机构".to_string()
            } else {
                d.actor_cid_number.clone()
            }
        });
    format!("费率设置 {rate_percent}：{inst_name}")
}
