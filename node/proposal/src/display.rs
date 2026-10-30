use nodeupgrade::summary::format_runtime_upgrade_summary;

use noderesolutionissue::summary::format_issuance_summary;

use nodedestroy::summary::format_destroy_summary;

use crate::query::resolve_proposal_action;
use crate::types::FeeRateProposalDetail;

use crate::types::ProposalDisplayMeta;
use crate::types::ProposalMeta;

pub(crate) struct ProposalDisplayInfo {
    pub(crate) summary: String,
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

/// 列表卡片展示信息:一次解析,按动作变体生成 summary + votebase status。
///
/// 与 [`fetch_proposal_full`] 共用 [`resolve_proposal_action`],保证两条路径对同一提案看到一致内容。
pub(crate) fn fetch_proposal_display(
    proposal_id: u64,
    meta: &ProposalMeta,
) -> Result<ProposalDisplayInfo, String> {
    let action = resolve_proposal_action(proposal_id, meta)?;
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
        status,
        status_label: status_label_s,
    })
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
