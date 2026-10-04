//! 链交易组装与提交(ADR-031 D7)。
//!
//! OnChina 侧唯一的 extrinsic 组装+提交通路：CitizenWallet 只签名一次并显示响应二维码，
//! OnChina 回扫后由本模块「重建签名材料 → 本地 sr25519 验签 → system_dryRun
//! 拒 Future/Stale → submit-and-watch → finalized + ExtrinsicSuccess」。
//! 签名材料构建统一调用 `chain-signing`,避免 OnChina 和 node 各自拼 payload。

use codec::Encode;
use serde_json::{json, Value};
use sp_core::crypto::Ss58Codec;
use subxt::{tx::SubmittableTransaction, OnlineClient, PolkadotConfig};

use super::chain_url::{chain_http_url, chain_ws_url};

const RPC_TIMEOUT_SECS: u64 = 10;
/// 客户端等待确认的观察窗口，不是 PoW 最晚出块期限；窗口结束后交易仍可能继续等待进块。
const WAIT_CONFIRMATION_OBSERVATION_SECS: u64 = 20 * 60;
/// 单次请求最多扫描的 finalized 块数，防止长期会话把一次 HTTP 请求放大成无界 O(N)。
const FINALIZED_RECOVERY_BATCH_BLOCKS: usize = 128;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FinalizedChainSubmit {
    pub(crate) tx_hash: String,
    pub(crate) block_number: u64,
    pub(crate) block_hash: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SubmissionAttempt {
    pub(crate) tx_hash: String,
    pub(crate) from_block_number: u64,
    pub(crate) from_block_hash: [u8; 32],
    pub(crate) recovery_cursor_block_number: Option<u64>,
    pub(crate) recovery_cursor_block_hash: Option<[u8; 32]>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FinalizedRecoveryScan {
    Found(FinalizedChainSubmit),
    /// 本批尚未抵达提交前锚点；游标指向下一批第一个尚未扫描的块。
    Continue {
        cursor_block_number: u64,
        cursor_block_hash: [u8; 32],
    },
    /// 已完整扫描到提交前 finalized 锚点，目标交易不在该区间内。
    Exhausted,
}

#[derive(Debug)]
pub(crate) struct ChainSubmitError {
    message: String,
    submitted_tx_hash: Option<String>,
}

impl ChainSubmitError {
    fn submitted_unconfirmed(tx_hash: String, message: String) -> Self {
        Self {
            message,
            submitted_tx_hash: Some(tx_hash),
        }
    }

    pub(crate) fn submitted_tx_hash(&self) -> Option<&str> {
        self.submitted_tx_hash.as_deref()
    }
}

impl std::fmt::Display for ChainSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<String> for ChainSubmitError {
    fn from(message: String) -> Self {
        Self {
            message,
            submitted_tx_hash: None,
        }
    }
}

/// prepare 阶段产物:随会话持久化,submit 阶段重建校验。
pub(crate) struct PreparedChainSign {
    pub nonce: u32,
    /// 给 QR `b.d` 的完整审阅载荷字节；不得用 32 字节签名哈希替代。
    pub payload: Vec<u8>,
    /// sha256(签名输入) hex,submit 阶段重建校验防 runtime 漂移。
    pub signing_hash_hex: String,
}

async fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    let url = chain_http_url()?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(RPC_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("build rpc client failed: {e}"))?;
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let resp: Value = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("rpc {method} request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("rpc {method} response decode failed: {e}"))?;
    if let Some(err) = resp.get("error") {
        return Err(format!("rpc {method} error: {err}"));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| format!("rpc {method} missing result"))
}

pub(crate) async fn fetch_runtime_version() -> Result<(u32, u32), String> {
    let result = rpc_post("state_getRuntimeVersion", Value::Array(vec![])).await?;
    let spec = result
        .get("specVersion")
        .and_then(|v| v.as_u64())
        .ok_or("runtime version missing specVersion")?;
    let tx = result
        .get("transactionVersion")
        .and_then(|v| v.as_u64())
        .ok_or("runtime version missing transactionVersion")?;
    Ok((spec as u32, tx as u32))
}

pub(crate) async fn fetch_genesis_hash() -> Result<[u8; 32], String> {
    let result = rpc_post("chain_getBlockHash", Value::Array(vec![Value::from(0)])).await?;
    let text = result.as_str().ok_or("genesis hash malformed")?;
    let bytes = hex::decode(text.strip_prefix("0x").unwrap_or(text))
        .map_err(|e| format!("genesis hash decode failed: {e}"))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "genesis hash must be 32 bytes".to_string())
}

fn public_key_to_ss58(account_id: &str) -> Result<String, String> {
    let public = chain_signing::parse_sr25519_public_key(account_id)?;
    Ok(public.to_ss58check_with_version(sp_core::crypto::Ss58AddressFormat::custom(2027)))
}

/// 实时读链上 nonce(死规则 P-SIGN-001:nonce 只来自链,不缓存不自增)。
pub(crate) async fn fetch_nonce(account_id: &str) -> Result<u32, String> {
    let ss58 = public_key_to_ss58(account_id)?;
    let result = rpc_post(
        "system_accountNextIndex",
        Value::Array(vec![Value::from(ss58)]),
    )
    .await?;
    result
        .as_u64()
        .map(|v| v as u32)
        .ok_or_else(|| "accountNextIndex malformed".to_string())
}

/// prepare:实时取 nonce/版本/创世哈希,构建 QR 审阅载荷与签名校验哈希。
pub(crate) async fn prepare_signing(
    call_data: &[u8],
    account_id: &str,
) -> Result<PreparedChainSign, String> {
    let nonce = fetch_nonce(account_id).await?;
    let (spec_version, tx_version) = fetch_runtime_version().await?;
    let genesis_hash = fetch_genesis_hash().await?;
    let material = chain_signing::build_signing_material(
        call_data,
        &genesis_hash,
        nonce,
        spec_version,
        tx_version,
    )?;
    Ok(PreparedChainSign {
        nonce,
        signing_hash_hex: chain_signing::sha256_hex(&material.signing_bytes),
        payload: material.payload,
    })
}

/// submit:重建材料校验哈希 → 本地验签 → dry-run → 提交,返回交易哈希。
pub(crate) async fn assemble_and_submit<F>(
    call_data: &[u8],
    account_id: &str,
    signature_hex: &str,
    nonce: u32,
    expected_signing_hash_hex: &str,
    before_submit: F,
) -> Result<FinalizedChainSubmit, ChainSubmitError>
where
    F: FnOnce(&SubmissionAttempt) -> Result<(), String>,
{
    let (spec_version, tx_version) = fetch_runtime_version().await?;
    let genesis_hash = fetch_genesis_hash().await?;
    let material = chain_signing::build_signing_material(
        call_data,
        &genesis_hash,
        nonce,
        spec_version,
        tx_version,
    )?;
    // 会话期间 runtime 版本/创世哈希不得漂移,否则签名对不上载荷。
    if chain_signing::sha256_hex(&material.signing_bytes) != expected_signing_hash_hex {
        return Err(
            "签名载荷与会话不一致(runtime 版本或创世哈希已变化),请重新发起"
                .to_string()
                .into(),
        );
    }

    let public = chain_signing::parse_sr25519_public_key(account_id)?;
    let signature = chain_signing::parse_sr25519_signature_hex(signature_hex)?;
    if !chain_signing::verify_signature(&material, &signature, &public) {
        return Err("sr25519 本地验签失败,拒绝提交".to_string().into());
    }

    let extrinsic = chain_signing::assemble_signed_extrinsic(material, public, signature);
    let extrinsic_hex = chain_signing::signed_extrinsic_hex(&extrinsic);
    let extrinsic_bytes = extrinsic.encode();

    // dry-run 是提交前硬预检。任何 RuntimeApi trap、交易无效或 RPC 不可用都必须
    // 停止提交，禁止跳过预检后把 wasm panic 原样推给浏览器。
    match rpc_post(
        "system_dryRun",
        Value::Array(vec![Value::from(extrinsic_hex.clone())]),
    )
    .await
    {
        Ok(v) => {
            let s = v.as_str().unwrap_or("");
            let raw = s.strip_prefix("0x").unwrap_or(s);
            let bytes = hex::decode(raw)
                .map_err(|e| format!("dry-run 结果异常,拒绝提交: {e} (raw: {s})"))?;
            if bytes.is_empty() {
                return Err("dry-run 返回空结果,拒绝提交".to_string().into());
            }
            if bytes[0] != 0x00 {
                return Err(chain_signing::dry_run_reject_message(&bytes, raw).into());
            }
            if bytes.len() > 1 && bytes[1] != 0x00 {
                return Err(format!("交易执行会失败: DispatchError (hex: {s})").into());
            }
        }
        Err(e) => {
            return Err(chain_signing::preflight_reject_message(&e).into());
        }
    }

    let ws_url = chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for finalized submit failed: {e}"))?;
    let transaction = SubmittableTransaction::from_bytes(client.clone(), extrinsic_bytes);
    let tx_hash = format!("{:#x}", transaction.hash());
    let from_block = client
        .blocks()
        .at_latest()
        .await
        .map_err(|e| format!("read pre-submit finalized block failed: {e}"))?;
    let attempt = SubmissionAttempt {
        tx_hash: tx_hash.clone(),
        from_block_number: u64::from(from_block.number()),
        from_block_hash: from_block.hash().0,
        recovery_cursor_block_number: None,
        recovery_cursor_block_hash: None,
    };
    // 在任何网络提交前先持久化 tx_hash 与 finalized 扫描锚点；DB 失败时绝不提交。
    before_submit(&attempt)?;
    // submit-and-watch 精确绑定本次 extrinsic；只有进入 finalized block 且出现
    // System.ExtrinsicSuccess 才返回，Dropped/Invalid/ExtrinsicFailed/超时全部失败。
    let finalized_result = tokio::time::timeout(
        std::time::Duration::from_secs(WAIT_CONFIRMATION_OBSERVATION_SECS),
        async {
            let progress = transaction
                .submit_and_watch()
                .await
                .map_err(|e| format!("submit and watch extrinsic failed: {e}"))?;
            let finalized = progress
                .wait_for_finalized()
                .await
                .map_err(|e| format!("wait extrinsic finalized failed: {e}"))?;
            let block_hash = finalized.block_hash();
            finalized
                .wait_for_success()
                .await
                .map_err(|e| format!("finalized extrinsic execution failed: {e}"))?;
            let block = client
                .blocks()
                .at(block_hash)
                .await
                .map_err(|e| format!("read finalized extrinsic block failed: {e}"))?;
            Ok::<(u64, [u8; 32]), String>((u64::from(block.number()), block_hash.0))
        },
    )
    .await;
    let (block_number, block_hash) = match finalized_result {
        Ok(Ok(finalized)) => finalized,
        Ok(Err(message)) => {
            return Err(ChainSubmitError::submitted_unconfirmed(tx_hash, message));
        }
        Err(_) => {
            return Err(ChainSubmitError::submitted_unconfirmed(
                tx_hash,
                format!(
                    "交易 finalized 观察窗口已结束({WAIT_CONFIRMATION_OBSERVATION_SECS} 秒)，结果未知"
                ),
            ));
        }
    };

    Ok(FinalizedChainSubmit {
        tx_hash,
        block_number,
        block_hash,
    })
}

/// 恢复 submit-and-watch 断线/超时后的结果：从 finalized head（或持久化游标）
/// 分批向提交前锚点回扫，并要求目标 extrinsic 自身存在 System.ExtrinsicSuccess。
/// 只有确实扫描到锚点才返回 Exhausted；中途必须返回并持久化下一批游标。
pub(crate) async fn find_finalized_success(
    tx_hash_hex: &str,
    from_block_number: u64,
    from_block_hash: [u8; 32],
    recovery_cursor_block_number: Option<u64>,
    recovery_cursor_block_hash: Option<[u8; 32]>,
) -> Result<FinalizedRecoveryScan, String> {
    if recovery_cursor_block_number.is_some() != recovery_cursor_block_hash.is_some() {
        return Err("finalized recovery cursor is incomplete".to_string());
    }
    let raw = hex::decode(tx_hash_hex.strip_prefix("0x").unwrap_or(tx_hash_hex))
        .map_err(|e| format!("submitted tx hash decode failed: {e}"))?;
    let target_hash = subxt::utils::H256(
        <[u8; 32]>::try_from(raw.as_slice())
            .map_err(|_| "submitted tx hash must be 32 bytes".to_string())?,
    );
    let ws_url = chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for finalized recovery failed: {e}"))?;
    let mut block = match (recovery_cursor_block_number, recovery_cursor_block_hash) {
        (Some(cursor_number), Some(cursor_hash)) => {
            let block = client
                .blocks()
                .at(subxt::utils::H256(cursor_hash))
                .await
                .map_err(|e| format!("read finalized recovery cursor block failed: {e}"))?;
            if u64::from(block.number()) != cursor_number {
                return Err("finalized recovery cursor number/hash mismatch".to_string());
            }
            block
        }
        (None, None) => client
            .blocks()
            .at_latest()
            .await
            .map_err(|e| format!("read latest finalized block failed: {e}"))?,
        _ => unreachable!("cursor completeness checked above"),
    };

    for _ in 0..FINALIZED_RECOVERY_BATCH_BLOCKS {
        let block_hash = block.hash();
        let block_number = u64::from(block.number());
        if block_number < from_block_number {
            return Err("finalized recovery cursor moved below submission anchor".to_string());
        }
        let extrinsics = block
            .extrinsics()
            .await
            .map_err(|e| format!("read finalized block extrinsics failed: {e}"))?;
        for extrinsic in extrinsics.iter() {
            if extrinsic.hash() != target_hash {
                continue;
            }
            let events = extrinsic
                .events()
                .await
                .map_err(|e| format!("read finalized extrinsic events failed: {e}"))?;
            let mut success = false;
            for event in events.iter() {
                let event =
                    event.map_err(|e| format!("decode finalized extrinsic event failed: {e}"))?;
                if event.pallet_name() == "System" && event.variant_name() == "ExtrinsicFailed" {
                    return Err("submitted extrinsic finalized with ExtrinsicFailed".to_string());
                }
                if event.pallet_name() == "System" && event.variant_name() == "ExtrinsicSuccess" {
                    success = true;
                }
            }
            if !success {
                return Err(
                    "submitted extrinsic finalized without System.ExtrinsicSuccess".to_string(),
                );
            }
            return Ok(FinalizedRecoveryScan::Found(FinalizedChainSubmit {
                tx_hash: tx_hash_hex.to_string(),
                block_number,
                block_hash: block_hash.0,
            }));
        }
        if block_number == from_block_number {
            if block_hash.0 != from_block_hash {
                return Err("finalized recovery anchor hash mismatch".to_string());
            }
            return Ok(FinalizedRecoveryScan::Exhausted);
        }
        let parent_hash = block.header().parent_hash;
        block = client
            .blocks()
            .at(parent_hash)
            .await
            .map_err(|e| format!("read parent finalized block failed: {e}"))?;
    }
    Ok(FinalizedRecoveryScan::Continue {
        cursor_block_number: u64::from(block.number()),
        cursor_block_hash: block.hash().0,
    })
}

#[cfg(test)]
// 交易编码夹具失败即代表链调用契约回归，断言式解包仅限测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use citizenchain as runtime;
    use codec::Encode;

    /// 材料构建离线自洽:同输入同产物,call 解码回等值,>256B 审阅载荷仍完整保留。
    #[test]
    fn signing_material_roundtrip_and_hash_rule() {
        let call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![7u8; 8],
        });
        let call_data = call.encode();
        let genesis = [9u8; 32];
        let m =
            chain_signing::build_signing_material(&call_data, &genesis, 5, 1, 1).expect("material");
        assert_eq!(m.call.encode(), call_data);
        // 小载荷:签名输入 == 审阅 payload 本体。
        assert!(m.payload.len() <= 256);
        assert_eq!(m.signing_bytes, m.payload);

        let big_call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![7u8; 400],
        });
        let big = chain_signing::build_signing_material(&big_call.encode(), &genesis, 5, 1, 1)
            .expect("material");
        // 大载荷:QR 仍必须拿到完整审阅 payload；只有实际签名输入是 32 字节 blake2_256。
        assert!(big.payload.len() > 256);
        assert_eq!(big.signing_bytes.len(), 32);
        assert_ne!(big.payload, big.signing_bytes);
    }

    #[test]
    fn tail_data_in_call_is_rejected() {
        let call = runtime::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
        let mut data = call.encode();
        data.push(0xff);
        assert!(chain_signing::decode_runtime_call(&data).is_err());
    }

    #[test]
    fn only_post_submit_uncertainty_carries_recoverable_tx_hash() {
        let rejected = super::ChainSubmitError::from("dry-run rejected".to_string());
        assert_eq!(rejected.submitted_tx_hash(), None);

        let uncertain = super::ChainSubmitError::submitted_unconfirmed(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            "subscription dropped".to_string(),
        );
        assert_eq!(
            uncertain.submitted_tx_hash(),
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }
}
