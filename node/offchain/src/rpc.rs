use crate::ledger::OffchainLedger;
use crate::packer::BatchSigner;
use codec::Decode;
use codec::DecodeAll;
use codec::Encode;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorCode;
use jsonrpsee::types::ErrorObjectOwned;
use nodeclient::types::FullClient;
use offchain::batch_item::PaymentIntent;
use sc_client_api::StorageProvider;
use serde::Deserialize;
use serde::Serialize;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::SaturatedConversion;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::sync::Arc;

/// 扫码支付 pallet 在 runtime `construct_runtime!` 中的实例名。与
/// `reserve.rs` 保持一致,storage key 前缀计算 `twox_128(PALLET_NAME)` 依赖它。
const PALLET_NAME: &[u8] = b"OffchainTransaction";

/// 清算行节点暴露给 citizenapp 的查询 RPC。
///
/// 命名空间 `offchain`,方法名与扫码支付协议保持一致。
#[rpc(server, namespace = "offchain")]
pub trait OffchainClearingRpc {
    /// 查询 L3 在本清算行的**可用存款余额**(分)。
    ///
    /// 可用余额 = 链上 `DepositBalance` 同步值 - 本地已接受未上链扣款。
    #[method(name = "queryBalance")]
    fn query_balance(&self, user: AccountId32) -> RpcResult<u128>;

    /// 查询 L3 下一个应使用的 `nonce`。
    ///
    /// citizenapp 本地保管 nonce 的同时,每次签名前问一次以防错位。
    #[method(name = "queryNextNonce")]
    fn query_next_nonce(&self, user: AccountId32) -> RpcResult<u64>;

    /// 查询本清算行待上链笔数(运维查看)。
    #[method(name = "queryPendingCount")]
    fn query_pending_count(&self) -> RpcResult<u64>;

    // ─── 扫码支付提交入口 ───

    /// 扫码支付提交入口。citizenapp 本地对 `PaymentIntent` 做 SCALE 编码后
    /// 用 L3 sr25519 私钥签名,把 hex 形式的 intent 和 64 字节签名一起提交。
    ///
    /// 节点侧:
    /// - 反序列化 intent
    /// - 验证 L3 sr25519 签名
    /// - 校验绑定清算行 / 费率 / nonce / 可用余额
    /// - 入账到本地 pending 列表
    /// - 返回 tx_id + 清算行 ACK 签名
    #[method(name = "submitPayment")]
    fn submit_payment(
        &self,
        intent_hex: String,
        payer_sig_hex: String,
    ) -> RpcResult<SubmitPaymentResp>;

    // ─── citizenapp 扫码前置查询 ───

    /// 查询 L3 当前绑定的清算行 CID 文本(对应链上 `UserBank[user]` 的值)。
    ///
    /// citizenapp 在扫码付款前调用,以确定"本人付款方清算行"(`payer_bank_cid`)。
    /// 未绑定返回 `None`,调用方据此提示用户先完成绑定流程。
    #[method(name = "queryUserBank")]
    fn query_user_bank(&self, user: AccountId32) -> RpcResult<Option<String>>;

    /// 查询指定清算行当前生效费率(对应链上 `L2FeeRateBp[bank]`)。
    ///
    /// 返回 `rate_bp`(万分之一)与 `min_fee_fen`(最低手续费,分)。
    /// citizenapp 据此在 UI 上展示费率与预计扣费,并本地预计算 `fee_amount`
    /// 以便构造 `PaymentIntent`。runtime `ValueQuery` 默认 0,本 RPC 同步
    /// 把 0 映射为"费率未设置"提示,调用方应拒绝提交。
    #[method(name = "queryFeeRate")]
    fn query_fee_rate(&self, bank_cid: String) -> RpcResult<FeeRateResp>;
}

/// 扫码支付提交响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitPaymentResp {
    /// 本笔支付 tx_id 的 hex(含 `0x` 前缀)。
    pub tx_id: String,
    /// 清算行 ACK 签名的 hex(64 字节 = 128 hex,含 `0x` 前缀)。
    pub l2_ack_sig: String,
    /// 节点接受本笔的 UNIX 秒时间戳。
    pub accepted_at: u64,
}

/// 清算行费率查询响应(`offchain_queryFeeRate`)。
///
/// 与 runtime `fee_config::calc_fee` 口径对齐:fee = `max(amount * rate_bp / 10_000, min_fee_fen)`,
/// 四舍五入规则同 runtime。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRateResp {
    /// 清算行当前生效费率(万分之一)。runtime 未配置时为 0,调用方应拒绝提交。
    pub rate_bp: u32,
    /// 最低手续费(分),= `primitives::fee_policy::OFFCHAIN_MIN_FEE`(当前 1 分 = 0.01 元)。
    pub min_fee_fen: u128,
}

/// 清算行 RPC 的具体实现。`derive(Clone)` 让 service.rs 能把 `Arc<Self>` 里
/// 的内容 owned 地传给 `into_rpc`;内部字段只有 Arc,clone 廉价。
#[derive(Clone)]
pub struct OffchainClearingRpcImpl {
    ledger: Arc<OffchainLedger>,
    /// 用于读取链上 storage(`UserBank` / `DepositBalance` / `L3PaymentNonce` / `L2FeeRateBp`)。
    client: Arc<FullClient>,
    /// 当前节点归属的清算行 **CID**(机构唯一永久主键),用于拒绝错路由支付。
    bank_cid: Vec<u8>,
    /// 复用清算行管理员签名器生成 L2 ACK。
    ack_signer: Arc<dyn BatchSigner>,
}

impl OffchainClearingRpcImpl {
    pub fn new(
        ledger: Arc<OffchainLedger>,
        client: Arc<FullClient>,
        bank_cid: Vec<u8>,
        ack_signer: Arc<dyn BatchSigner>,
    ) -> Self {
        Self {
            ledger,
            client,
            bank_cid,
            ack_signer,
        }
    }

    fn read_user_bank(&self, user: &AccountId32) -> RpcResult<Option<Vec<u8>>> {
        let best = self.client.info().best_hash;
        let key = user_bank_storage_key(user);
        let raw = self
            .client
            .storage(best, &key)
            .map_err(|e| rpc_err(ErrorCode::InternalError, format!("storage 读取失败:{e}")))?;
        match raw {
            None => Ok(None),
            // 值为 InstitutionCidNumber(BoundedVec<u8>),SCALE 与 Vec<u8> 等价。
            Some(data) => Vec::<u8>::decode(&mut &data.0[..])
                .map(Some)
                .map_err(|e| rpc_err(ErrorCode::InternalError, format!("CID 解码失败:{e}"))),
        }
    }

    fn read_fee_rate(&self, bank_cid: &[u8]) -> RpcResult<u32> {
        let best = self.client.info().best_hash;
        let key = l2_fee_rate_bp_storage_key(bank_cid);
        let raw = self
            .client
            .storage(best, &key)
            .map_err(|e| rpc_err(ErrorCode::InternalError, format!("storage 读取失败:{e}")))?;
        match raw {
            None => Ok(0),
            Some(data) => u32::decode(&mut &data.0[..])
                .map_err(|e| rpc_err(ErrorCode::InternalError, format!("u32 解码失败:{e}"))),
        }
    }

    fn read_l3_payment_nonce(&self, user: &AccountId32) -> RpcResult<u64> {
        let best = self.client.info().best_hash;
        let key = l3_payment_nonce_storage_key(user);
        let raw = self
            .client
            .storage(best, &key)
            .map_err(|e| rpc_err(ErrorCode::InternalError, format!("storage 读取失败:{e}")))?;
        match raw {
            None => Ok(0),
            Some(data) => u64::decode(&mut &data.0[..])
                .map_err(|e| rpc_err(ErrorCode::InternalError, format!("u64 解码失败:{e}"))),
        }
    }

    fn read_deposit_balance(&self, bank_cid: &[u8], user: &AccountId32) -> RpcResult<u128> {
        let best = self.client.info().best_hash;
        let key = deposit_balance_storage_key(bank_cid, user);
        let raw = self
            .client
            .storage(best, &key)
            .map_err(|e| rpc_err(ErrorCode::InternalError, format!("storage 读取失败:{e}")))?;
        match raw {
            None => Ok(0),
            Some(data) => u128::decode(&mut &data.0[..])
                .map_err(|e| rpc_err(ErrorCode::InternalError, format!("u128 解码失败:{e}"))),
        }
    }
}

impl OffchainClearingRpcServer for OffchainClearingRpcImpl {
    fn query_balance(&self, user: AccountId32) -> RpcResult<u128> {
        Ok(self.ledger.available_balance(&user))
    }

    fn query_next_nonce(&self, user: AccountId32) -> RpcResult<u64> {
        let chain_next = self.read_l3_payment_nonce(&user)?.saturating_add(1);
        Ok(chain_next.max(self.ledger.next_nonce(&user)))
    }

    fn query_pending_count(&self) -> RpcResult<u64> {
        Ok(self.ledger.pending_count() as u64)
    }

    fn submit_payment(
        &self,
        intent_hex: String,
        payer_sig_hex: String,
    ) -> RpcResult<SubmitPaymentResp> {
        // 1. 解析 intent SCALE hex → PaymentIntent
        let intent_bytes = decode_hex(&intent_hex)
            .map_err(|e| rpc_err(ErrorCode::InvalidParams, format!("intent_hex 解析失败:{e}")))?;
        let intent = PaymentIntent::<AccountId32, u32>::decode_all(&mut &intent_bytes[..])
            .map_err(|e| {
                rpc_err(
                    ErrorCode::InvalidParams,
                    format!("PaymentIntent SCALE 反序列化失败:{e}"),
                )
            })?;

        // 2. 解析签名 hex → 64 字节
        let sig_bytes = decode_hex(&payer_sig_hex)
            .map_err(|e| rpc_err(ErrorCode::InvalidParams, format!("sig_hex 解析失败:{e}")))?;
        if sig_bytes.len() != 64 {
            return Err(rpc_err(
                ErrorCode::InvalidParams,
                format!("payer_sig 必须 64 字节,实际 {}", sig_bytes.len()),
            ));
        }
        let mut payer_sig = [0u8; 64];
        payer_sig.copy_from_slice(&sig_bytes);

        // 3. 链上状态早拒:错路由、绑定漂移、费率不一致都不进入 pending。
        if intent.recipient_bank_cid != self.bank_cid {
            return Err(rpc_err(
                ErrorCode::InvalidParams,
                "recipient_bank_cid 不属于当前清算行节点",
            ));
        }
        let payer_bank = self.read_user_bank(&intent.payer_account_id)?;
        if payer_bank.as_ref() != Some(&intent.payer_bank_cid) {
            return Err(rpc_err(
                ErrorCode::InvalidParams,
                "payer_bank_cid 与链上 UserBank[payer] 不一致",
            ));
        }
        let recipient_bank = self.read_user_bank(&intent.recipient_account_id)?;
        if recipient_bank.as_ref() != Some(&intent.recipient_bank_cid) {
            return Err(rpc_err(
                ErrorCode::InvalidParams,
                "recipient_bank_cid 与链上 UserBank[recipient] 不一致",
            ));
        }
        let rate_bp = self.read_fee_rate(&intent.recipient_bank_cid)?;
        if rate_bp == 0 {
            return Err(rpc_err(ErrorCode::InvalidParams, "收款方清算行费率未配置"));
        }
        let expected_fee = calc_fee(intent.amount, rate_bp)
            .map_err(|e| rpc_err(ErrorCode::InvalidParams, format!("手续费计算失败:{e}")))?;
        if intent.fee != expected_fee {
            return Err(rpc_err(
                ErrorCode::InvalidParams,
                format!(
                    "手续费不匹配:expected={expected_fee}, actual={}",
                    intent.fee
                ),
            ));
        }

        let current_block: u32 = self.client.info().best_number.saturated_into();
        let accepted_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ack_message = l2_ack_signing_message(&self.bank_cid, &intent, &payer_sig, accepted_at);
        let l2_ack = self
            .ack_signer
            .sign_batch(&ack_message)
            .map_err(|e| rpc_err(ErrorCode::InternalError, format!("L2 ACK 签名失败:{e}")))?;

        // 4. 调 ledger 执行 L3 签名、nonce、余额校验并写入 pending。
        //
        // 收款方主导清算后,跨行支付会提交到收款方清算节点。此时付款方
        // 不属于本地 ledger,必须读取链上权威 `DepositBalance/L3PaymentNonce`
        // 做校验,不能把付款方写成本地 ghost 账户。
        let cross_bank = intent.payer_bank_cid != self.bank_cid;
        let (chain_confirmed, chain_nonce) = if cross_bank {
            (
                Some(self.read_deposit_balance(&intent.payer_bank_cid, &intent.payer_account_id)?),
                Some(self.read_l3_payment_nonce(&intent.payer_account_id)?),
            )
        } else {
            (None, None)
        };
        let (tx_id, l2_ack) = self
            .ledger
            .accept_payment_with_chain_state(
                intent,
                payer_sig,
                Some(current_block),
                l2_ack,
                accepted_at,
                &self.bank_cid,
                chain_confirmed,
                chain_nonce,
            )
            .map_err(|e| rpc_err(ErrorCode::InvalidParams, e))?;

        Ok(SubmitPaymentResp {
            tx_id: format!("0x{}", encode_hex(tx_id.as_bytes())),
            l2_ack_sig: format!("0x{}", encode_hex(&l2_ack)),
            accepted_at,
        })
    }

    fn query_user_bank(&self, user: AccountId32) -> RpcResult<Option<String>> {
        // 返回该用户绑定清算行的 CID 文本(CID 为 ASCII);未绑定返回 None。
        Ok(self
            .read_user_bank(&user)?
            .map(|c| String::from_utf8_lossy(&c).into_owned()))
    }

    fn query_fee_rate(&self, bank_cid: String) -> RpcResult<FeeRateResp> {
        let rate_bp = self.read_fee_rate(bank_cid.as_bytes())?;
        Ok(FeeRateResp {
            rate_bp,
            min_fee_fen: primitives::fee_policy::OFFCHAIN_MIN_FEE,
        })
    }
}

/// 与 runtime `settlement::calc_fee` 保持一致:按万分比四舍五入,最低取 `primitives::fee_policy::OFFCHAIN_MIN_FEE`。
fn calc_fee(transfer_amount: u128, rate_bp: u32) -> Result<u128, &'static str> {
    let numerator = transfer_amount
        .checked_mul(rate_bp as u128)
        .ok_or("fee overflow")?;
    let quotient = numerator / 10_000;
    let remainder = numerator % 10_000;
    let rounded = if remainder >= 5_000 {
        quotient + 1
    } else {
        quotient
    };
    Ok(std::cmp::max(
        rounded,
        primitives::fee_policy::OFFCHAIN_MIN_FEE,
    ))
}

/// 构造 L2 ACK 签名消息(唯一原语):
/// `signing_message(OP_SIGN_L2_ACK, SCALE(bank_cid) || SCALE(intent) || payer_sig || accepted_at_le)`。
/// 变长 `bank_cid` 用 `Compact(len)||bytes` 自定界(与 `batch_signing_hash` 一致),
/// 与后随的 `SCALE(intent)` 之间无边界歧义。
/// 注:citizenapp 侧 ACK 验签为 Step 3 待启用;启用时须按同一布局补跨语言金标。
fn l2_ack_signing_message(
    bank_cid: &[u8],
    intent: &PaymentIntent<AccountId32, u32>,
    payer_sig: &[u8; 64],
    accepted_at: u64,
) -> [u8; 32] {
    let bank_cid_scale = bank_cid.to_vec().encode();
    let intent_bytes = intent.encode();
    let mut scale_payload = Vec::with_capacity(bank_cid_scale.len() + intent_bytes.len() + 64 + 8);
    scale_payload.extend_from_slice(&bank_cid_scale);
    scale_payload.extend_from_slice(&intent_bytes);
    scale_payload.extend_from_slice(payer_sig);
    scale_payload.extend_from_slice(&accepted_at.to_le_bytes());
    primitives::sign::signing_message(primitives::sign::OP_SIGN_L2_ACK, &scale_payload)
}

/// 构造 `UserBank[user]` 的 storage key(键 = AccountId,值 = InstitutionCidNumber CID)。
fn user_bank_storage_key(user: &AccountId32) -> StorageKey {
    let encoded = user.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"UserBank"));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    k.extend_from_slice(&encoded);
    StorageKey(k)
}

/// 构造 `L2FeeRateBp[bank]` 的 storage key(键 = InstitutionCidNumber CID,值 = u32)。
fn l2_fee_rate_bp_storage_key(bank_cid: &[u8]) -> StorageKey {
    // CID 键 = InstitutionCidNumber(BoundedVec<u8>),SCALE = Compact(len) || bytes。
    let encoded = bank_cid.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"L2FeeRateBp"));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    k.extend_from_slice(&encoded);
    StorageKey(k)
}

/// 构造 `L3PaymentNonce[user]` 的 storage key(`StorageMap<_, Blake2_128Concat, AccountId, u64, ValueQuery>`)。
fn l3_payment_nonce_storage_key(user: &AccountId32) -> StorageKey {
    let encoded = user.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"L3PaymentNonce"));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
    k.extend_from_slice(&encoded);
    StorageKey(k)
}

/// 构造 `DepositBalance[bank][user]` 的 storage key。
///
/// runtime 定义为 `StorageDoubleMap<_, Blake2_128Concat, InstitutionCidNumber,
/// Blake2_128Concat, AccountId, u128>`:一级键 = CID(Compact(len)||bytes 变长),
/// 二级键 = AccountId(定长 32B),各自 `blake2_128(编码) || 编码`。
fn deposit_balance_storage_key(bank_cid: &[u8], user: &AccountId32) -> StorageKey {
    // 一级键 CID = BoundedVec<u8> SCALE = Compact(len) || bytes;二级键 user = AccountId32(32B)。
    let bank_encoded = bank_cid.encode();
    let user_encoded = user.encode();
    let mut k = Vec::with_capacity(16 + 16 + 16 + bank_encoded.len() + 16 + user_encoded.len());
    k.extend_from_slice(&sp_io::hashing::twox_128(PALLET_NAME));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"DepositBalance"));
    k.extend_from_slice(&sp_io::hashing::blake2_128(&bank_encoded));
    k.extend_from_slice(&bank_encoded);
    k.extend_from_slice(&sp_io::hashing::blake2_128(&user_encoded));
    k.extend_from_slice(&user_encoded);
    StorageKey(k)
}

// ─── 内部工具 ───

/// 解析 hex(支持 `0x` 前缀),与现有 citizenapp / offchain 客户端风格一致。
fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    let text = input.strip_prefix("0x").unwrap_or(input);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !text.len().is_multiple_of(2) {
        return Err("hex 长度必须偶数".to_string());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for i in (0..text.len()).step_by(2) {
        let byte =
            u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| format!("非法 hex 字节: {e}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// 通用错误构造器。
pub(crate) fn rpc_err<T: Into<String>>(code: ErrorCode, msg: T) -> ErrorObjectOwned {
    ErrorObjectOwned::owned::<()>(code.code(), msg.into(), None)
}
#[cfg(test)]
#[path = "../tests/rpc.rs"]
mod tests;
