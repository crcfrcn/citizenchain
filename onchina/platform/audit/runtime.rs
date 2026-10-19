//! 运行期启动辅助和显式维护动作。
//!
//! 公权机构不在这里生成或对账;其唯一真源是链上 PublicManage,
//! 本地只通过平台 indexer::db::government 同步链投影缓存。

use crate::AppState;

/// 审计日志只存"事实"——detail 是结构化 JSON(键小写蛇形,值存系统原值:
/// 代码/布尔/原始字段),不得写展示文案;人话翻译统一归前端操作记录渲染器
/// (OperationRecords 的键名/值映射),文案改版零后端改动且历史行同样适用。
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_audit_log(
    state: &AppState,
    action: &'static str,
    actor_account_id: &str,
    target_cid: Option<String>,
    detail: serde_json::Value,
) {
    let actor_account_id = crate::crypto::pubkey::normalize_account_id(actor_account_id);
    let action = action.to_string();
    let log_action = action.clone();
    // 审计归属「办理该动作的注册局」= 本节点自身作用域(省/市),与目标 CID 无关。
    // 目标 CID 只作关联列 target_cid:人主体目标 R5 去地域化不载省市,机构目标省市也不代表办理局。
    // 单一真源 resolve_node_scope(与审计读侧管理员作用域同源);节点未绑定机构 = 非法调用,
    // 丢弃不写(禁止兜底/退化,绝不落到错误分区)。
    let (province_code, city_code) = match crate::institutions::catalog::projection::resolve_node_scope(&state.db)
    {
        Ok(Some((_is_federal, scope))) => {
            let city = (scope.city_code != "000").then_some(scope.city_code);
            (scope.province_code, city)
        }
        Ok(None) => {
            tracing::warn!(action = %log_action, "append audit dropped: node unbound, no registrar scope");
            return;
        }
        Err(err) => {
            tracing::warn!(action = %log_action, error = %err, "append audit dropped: resolve node scope failed");
            return;
        }
    };
    if let Err(err) = state.db.with_client(move |conn| {
        conn.execute(
            "INSERT INTO audit(
                province_code, city_code, actor_account_id, action, target_cid, detail
             )
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &province_code,
                &city_code,
                &actor_account_id,
                &action,
                &target_cid,
                &detail,
            ],
        )
        .map_err(|e| format!("insert audit failed: {e}"))?;
        Ok(())
    }) {
        tracing::warn!(action = %log_action, error = %err, "append audit failed");
    }
}

/// 启动公共机构对账，使用原CID真源与原检查条件。
pub(crate) mod catalog {
//! 创世公权机构目录链上验收(ADR-031 D9)。
//!
//! 本文件只做部署/启动验收:用创世派生规则抽样或全量比对链上
//! `PublicManage::Institutions`,确认 runtime/onchina 版本未漂移。
//! OnChina 本地公权机构数据不得由这里生成,只能由平台 indexer::db::government 从链上投影。

use std::collections::{BTreeMap, BTreeSet};

use crate::core::chain_runtime;

/// 启动抽样条数。
pub(crate) const SAMPLE_SIZE: usize = 32;
const STARTUP_RETRIES: usize = 6;
const RETRY_INTERVAL_SECS: u64 = 10;

/// 抽样不一致(数据错,不重试);其余为链暂不可达类错误(可重试)。
pub(crate) enum AuditError {
    Mismatch(String),
    Unreachable(String),
}

/// 常量 296 机构 (号 → (全称, 简称))。
pub(crate) fn constant_institutions() -> BTreeMap<String, (String, String)> {
    use primitives::cid::china::{
        china_cb::CHINA_CB, china_ch::CHINA_CH, china_jc::CHINA_JC, china_jy::CHINA_JY,
        china_lf::CHINA_LF, china_sf::CHINA_SF, china_zf::CHINA_ZF,
    };
    let mut out = BTreeMap::new();
    let mut push = |cid: &str, full: &str, short: &str| {
        out.insert(cid.to_string(), (full.to_string(), short.to_string()));
    };
    for n in CHINA_ZF.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_JC.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_CB.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_SF.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_LF.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_CH.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    for n in CHINA_JY.iter() {
        push(n.cid_number, n.cid_full_name, n.cid_short_name);
    }
    out
}

pub(crate) async fn audit_one(cid: &str, full: &str, short: &str) -> Result<(), AuditError> {
    let on = chain_runtime::institution_lookup(cid)
        .await
        .map_err(AuditError::Unreachable)?
        .ok_or_else(|| AuditError::Mismatch(format!("链上缺创世机构 {cid}")))?;
    if on.cid_full_name != full.as_bytes() || on.cid_short_name != short.as_bytes() {
        return Err(AuditError::Mismatch(format!(
            "机构 {cid} 名称与链上不一致(本地 {full}/{short})"
        )));
    }
    Ok(())
}

/// 单轮抽样：覆盖公权派生全域，并固定核验私权创世公民链基金会。
async fn sample_audit_once() -> Result<usize, AuditError> {
    let total = primitives::cid::official_derive::public_institution_derived_count();
    let salt = (chrono::Utc::now().timestamp().unsigned_abs() as usize) % total;
    let step = (total / SAMPLE_SIZE).max(1);
    let picks: BTreeSet<usize> = (0..SAMPLE_SIZE)
        .map(|i| (salt + i * step) % total)
        .collect();

    let mut expected: Vec<(String, String, String)> = Vec::with_capacity(picks.len() + 1);
    let mut idx = 0usize;
    primitives::cid::official_derive::for_each_public_institution(|cid, full, short| {
        if picks.contains(&idx) {
            expected.push((cid.to_string(), full.to_string(), short.to_string()));
        }
        idx += 1;
    });
    if let Some((cid, (full, short))) = constant_institutions().into_iter().next() {
        expected.push((cid, full, short));
    }
    let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
    expected.push((
        foundation.cid_number.to_string(),
        foundation.cid_full_name.to_string(),
        foundation.cid_short_name.to_string(),
    ));
    for (cid, full, short) in &expected {
        audit_one(cid, full, short).await?;
    }
    Ok(expected.len())
}

/// 启动 fail-closed 抽样对账:数据不一致立即失败;链暂不可达重试后失败。
pub(crate) fn startup_sample_audit_blocking() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("audit runtime: {e}"))?;
    rt.block_on(async {
        let mut last = String::new();
        for attempt in 1..=STARTUP_RETRIES {
            match sample_audit_once().await {
                Ok(sampled) => {
                    tracing::info!(sampled, "创世机构目录链上抽样对账通过");
                    return Ok(());
                }
                Err(AuditError::Mismatch(e)) => return Err(format!("链上目录对账不一致: {e}")),
                Err(AuditError::Unreachable(e)) => {
                    last = e;
                    tracing::warn!(attempt, error = %last, "链上对账暂不可达,重试");
                    tokio::time::sleep(std::time::Duration::from_secs(RETRY_INTERVAL_SECS)).await;
                }
            }
        }
        Err(format!(
            "链上抽样对账在 {STARTUP_RETRIES} 次重试后仍不可达: {last}"
        ))
    })
}


}
