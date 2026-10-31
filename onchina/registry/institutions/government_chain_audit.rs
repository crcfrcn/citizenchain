//! 注册局显式全量目录验收，启动抽样由平台负责。
use onchina::audit_runtime::catalog::{audit_one, constant_institutions, AuditError};
use onchina::chain_runtime;
use std::collections::BTreeMap;
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
