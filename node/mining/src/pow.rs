//! PoW哈希、seal检查与父块难度读取。

// Substrate service API 固定返回 sc_service::Error，Node 必须保持框架函数签名。
#![allow(clippy::result_large_err)]
use runtime;

use codec::Decode;

use nodeclient::types::FullClient;

use runtime::opaque::Block;

use sc_client_api::StorageProvider;

use sc_consensus_pow::PowAlgorithm;

use sc_network::NetworkBackend as _;

use sp_core::crypto::KeyTypeId;
use sp_core::sr25519;
use sp_core::Pair as _;
use sp_core::U256;
use sp_crypto_hashing::blake2_256;
use sp_crypto_hashing::twox_128;

use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;

use std::sync::Arc;

// PoW 作者密钥类型：纯 PoW 链使用独立 key type，避免与 Aura 语义混用。
pub const POW_AUTHOR_KEY_TYPE: KeyTypeId = KeyTypeId(*b"powr");

#[derive(Clone)]
pub struct SimplePow {
    /// 持有 client 引用，直接读取父块 RAW 难度，避免信任可升级 Runtime API。
    client: Arc<FullClient>,
}

impl SimplePow {
    /// 核心服务与真实区块验收共用同一难度读取和 seal 校验实现。
    pub fn new(client: Arc<FullClient>) -> Self {
        Self { client }
    }
}

impl PowAlgorithm<Block> for SimplePow {
    type Difficulty = U256;

    /// 从父块状态读取当前 PoW 难度；缺失、畸形、尾随字节或零值全部 fail-closed。
    fn difficulty(
        &self,
        parent: <Block as BlockT>::Hash,
    ) -> Result<Self::Difficulty, sc_consensus_pow::Error<Block>> {
        let key = [
            twox_128(b"PowDifficulty").as_slice(),
            twox_128(b"CurrentDifficulty").as_slice(),
        ]
        .concat();
        let raw = self
            .client
            .storage(parent, &StorageKey(key))
            .map_err(|e| sc_consensus_pow::Error::BlockProposingError(e.to_string()))?
            .ok_or_else(|| {
                sc_consensus_pow::Error::BlockProposingError(
                    "父块缺少 PowDifficulty::CurrentDifficulty".into(),
                )
            })?;
        let mut input = raw.0.as_slice();
        let difficulty = u64::decode(&mut input).map_err(|_| {
            sc_consensus_pow::Error::BlockProposingError(
                "PowDifficulty::CurrentDifficulty SCALE 解码失败".into(),
            )
        })?;
        if !input.is_empty() || difficulty == 0 {
            return Err(sc_consensus_pow::Error::BlockProposingError(
                "PowDifficulty::CurrentDifficulty 非规范或为零".into(),
            ));
        }
        Ok(U256::from(difficulty))
    }

    fn verify(
        &self,
        _parent: &sp_runtime::generic::BlockId<Block>,
        pre_hash: &<Block as BlockT>::Hash,
        pre_digest: Option<&[u8]>,
        seal: &sp_consensus_pow::Seal,
        difficulty: Self::Difficulty,
    ) -> Result<bool, sc_consensus_pow::Error<Block>> {
        // pre_digest 包含矿工 sr25519 公钥，seal 包含 (nonce, 签名)。
        // 验证：1) PoW 难度满足  2) 签名证明矿工确实拥有该公钥的私钥。
        let Some(pre_digest) = pre_digest else {
            return Ok(false);
        };
        let public = match sr25519::Public::decode(&mut &pre_digest[..]) {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };

        let (nonce, signature): (u64, sr25519::Signature) =
            Decode::decode(&mut &seal[..]).map_err(sc_consensus_pow::Error::<Block>::Codec)?;

        let hash = pow_hash(pre_hash.as_ref(), nonce);
        if !hash_meets_difficulty(&hash, difficulty) {
            return Ok(false);
        }

        // 验证矿工对 pre_hash 的 sr25519 签名，防止冒充他人公钥。
        Ok(sr25519::Pair::verify(
            &signature,
            pre_hash.as_ref(),
            &public,
        ))
    }
}

// RPC 运行态验收复用实际矿工的 PoW 计算，不另建测试算法。
pub fn pow_hash(pre_hash: &[u8], nonce: u64) -> [u8; 32] {
    let mut payload = Vec::with_capacity(pre_hash.len() + std::mem::size_of::<u64>());
    payload.extend_from_slice(pre_hash);
    payload.extend_from_slice(&nonce.to_le_bytes());
    blake2_256(&payload)
}

pub fn hash_meets_difficulty(hash: &[u8; 32], difficulty: U256) -> bool {
    if difficulty.is_zero() {
        return false;
    }
    let target = U256::MAX / difficulty;
    U256::from_big_endian(hash) <= target
}

#[cfg(test)]
#[path = "../tests/pow.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/performance.rs"]
mod performance_tests;
