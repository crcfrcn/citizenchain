//! 创世公权机构目录链上验收(ADR-031 D9)。
//!
//! 本文件只做部署/启动验收:用创世派生规则抽样或全量比对链上
//! `PublicManage::Institutions`,确认 runtime/onchina 版本未漂移。
//! OnChina 本地公权机构数据不得由这里生成,只能由 gov::service 从链上投影。

use std::collections::{BTreeMap, BTreeSet};

use crate::core::chain_runtime;

/// 启动抽样条数。
pub(crate) const SAMPLE_SIZE: usize = 32;
const STARTUP_RETRIES: usize = 6;
const RETRY_INTERVAL_SECS: u64 = 10;

/// 抽样不一致(数据错,不重试);其余为链暂不可达类错误(可重试)。
enum AuditError {
    Mismatch(String),
    Unreachable(String),
}

/// 常量 296 机构 (号 → (全称, 简称))。
fn constant_institutions() -> BTreeMap<String, (String, String)> {
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

async fn audit_one(cid: &str, full: &str, short: &str) -> Result<(), AuditError> {
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

/// 全量双向比对(`audit-chain-catalog` 子命令,部署验收用):
/// 本地创世目录(常量 296 + 省/市派生 49,297)与链上
/// `Institutions` 逐字节互查。
pub(crate) fn full_audit_blocking() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("audit runtime: {e}"))?;
    rt.block_on(async {
        let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
        audit_one(
            foundation.cid_number,
            foundation.cid_full_name,
            foundation.cid_short_name,
        )
        .await
        .map_err(|error| match error {
            AuditError::Mismatch(message) | AuditError::Unreachable(message) => message,
        })?;
        let mut local: BTreeMap<Vec<u8>, (Vec<u8>, Vec<u8>)> = constant_institutions()
            .into_iter()
            .map(|(cid, (full, short))| (cid.into_bytes(), (full.into_bytes(), short.into_bytes())))
            .collect();
        primitives::cid::official_derive::for_each_public_institution(|cid, full, short| {
            local.insert(
                cid.as_bytes().to_vec(),
                (full.as_bytes().to_vec(), short.as_bytes().to_vec()),
            );
        });
        let local_total = local.len();

        let mut mismatched = 0usize;
        let mut extra_on_chain = 0usize;
        let chain_total = chain_runtime::for_each_chain_institution(|cid, info| {
            match local.remove(&cid) {
                Some((full, short)) => {
                    if info.cid_full_name != full
                        || info.cid_short_name != short
                    {
                        mismatched += 1;
                        tracing::error!(
                            cid = %String::from_utf8_lossy(&cid),
                            "链上机构与本地派生不一致"
                        );
                    }
                }
                None => {
                    extra_on_chain += 1;
                    tracing::error!(
                        cid = %String::from_utf8_lossy(&cid),
                        "链上存在本地派生之外的创世机构"
                    );
                }
            }
        })
        .await?;

        let missing_on_chain = local.len();
        for cid in local.keys().take(10) {
            tracing::error!(cid = %String::from_utf8_lossy(cid), "本地派生机构在链上缺失");
        }
        println!(
            "audit-chain-catalog: 本地 {local_total} / 链上 {chain_total} / 不一致 {mismatched} / 链上多出 {extra_on_chain} / 链上缺失 {missing_on_chain}"
        );
        if mismatched == 0 && extra_on_chain == 0 && missing_on_chain == 0 {
            println!("audit-chain-catalog: 全量双向比对通过 ✓");
            Ok(())
        } else {
            Err("链上机构目录与本地派生不一致,见日志".to_string())
        }
    })
}
