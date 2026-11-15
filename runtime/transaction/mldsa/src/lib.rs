//! 公民链、公民 SDK 与离线钱包共用唯一实现。
//! 原生消费者不启用 `runtime` 特性，不链接 FRAME 或节点。
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use blake2::{digest::consts::U32, Blake2b, Digest};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use fips204::{
    ml_dsa_65,
    traits::{SerDes, Verifier},
};
use scale_info::TypeInfo;

pub const PUBLIC_KEY_BYTES: usize = 1952;
pub const SIGNATURE_BYTES: usize = 3309;
pub const SR25519_SIGNATURE_BYTES: usize = 64;
pub const SIGNATURE_ENVELOPE_BYTES: usize = PUBLIC_KEY_BYTES + SIGNATURE_BYTES;
pub const SR25519_ALGORITHM: u8 = 1;
pub const MLDSA65_ALGORITHM: u8 = 2;
pub const SIGNING_CONTEXT: &[u8] = b"GMB/ml-dsa-65/v1";
pub const ACCOUNT_DOMAIN: &[u8] = b"GMB/account/ml-dsa-65/v1";
pub const TRANSACTION_DOMAIN: &[u8] = b"GMB/transaction/ml-dsa-65/v1";
pub const EXTENSION_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AccountAlgorithm {
    Sr25519 = 1,
    Mldsa65 = 2,
}
impl Default for AccountAlgorithm {
    fn default() -> Self {
        if legacy_derivation_allowed() {
            Self::Sr25519
        } else {
            Self::Mldsa65
        }
    }
}
pub const fn legacy_derivation_allowed() -> bool {
    !cfg!(any(feature = "operation", feature = "pq-only"))
}
impl AccountAlgorithm {
    /// 零表示使用本次构建的默认算法，不代表第三种签名算法。
    pub fn from_option(value: u32) -> Result<Self, &'static str> {
        let algorithm = match value {
            0 => Self::default(),
            1 => Self::Sr25519,
            2 => Self::Mldsa65,
            _ => return Err("unknown account algorithm"),
        };
        if algorithm == Self::Sr25519 && !legacy_derivation_allowed() {
            return Err("sr25519 derivation is disabled in operation");
        }
        Ok(algorithm)
    }
}

/// 按 SCALE 规范编码 `ImplicationParts { base: (version, call), explicit, implicit }`。
/// 输入均为已编码的字段字节，不附加 SCALE 向量长度；两种交易构建入口共用此实现。
pub fn transaction_implication(
    call: &[u8],
    explicit: &[u8],
    implicit: &[u8],
) -> alloc::vec::Vec<u8> {
    let mut bytes =
        alloc::vec::Vec::with_capacity(1 + call.len() + explicit.len() + implicit.len());
    bytes.push(EXTENSION_VERSION);
    bytes.extend_from_slice(call);
    bytes.extend_from_slice(explicit);
    bytes.extend_from_slice(implicit);
    bytes
}

pub fn review_message(account: &[u8; 32], review: &[u8]) -> [u8; 32] {
    let mut implication = alloc::vec::Vec::with_capacity(review.len() + 1);
    implication.push(EXTENSION_VERSION);
    implication.extend_from_slice(review);
    transaction_message(account, &implication)
}

/// 使用官方通用交易外层编码，签名与证明的 SCALE 定义集中在本模块。
pub fn encode_transaction(proof: &Proof, extensions: &[u8], call: &[u8]) -> alloc::vec::Vec<u8> {
    let mut body = alloc::vec![0x45, EXTENSION_VERSION];
    body.extend(proof.encode());
    body.extend_from_slice(extensions);
    body.extend_from_slice(call);
    let mut encoded = codec::Compact(body.len() as u32).encode();
    encoded.extend(body);
    encoded
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo)]
pub struct PublicKey(pub [u8; PUBLIC_KEY_BYTES]);

/// 客户端使用的真实签名公钥；PQ 公钥以共享引用保存，克隆轨迹不复制完整字节。
#[cfg(feature = "std")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountPublicKey {
    Sr25519([u8; 32]),
    Mldsa65(alloc::sync::Arc<PublicKey>),
}

#[cfg(feature = "std")]
impl From<PublicKey> for AccountPublicKey {
    fn from(public: PublicKey) -> Self {
        Self::Mldsa65(alloc::sync::Arc::new(public))
    }
}

#[cfg(feature = "std")]
impl AccountPublicKey {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Sr25519(public) => public,
            Self::Mldsa65(public) => &public.as_ref().0,
        }
    }

    /// 账户派生关系只在本模块定义，账户标识与公钥字节分别保留。
    pub fn account_id(&self) -> [u8; 32] {
        match self {
            Self::Sr25519(public) => *public,
            Self::Mldsa65(public) => account_id(public.as_ref()),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo)]
pub struct Signature(pub [u8; SIGNATURE_BYTES]);

impl core::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Mldsa65PublicKey")
            .field(&account_id(self))
            .finish()
    }
}
impl core::fmt::Debug for Signature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Mldsa65Signature")
    }
}

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Blake2b::<U32>::digest(bytes).into()
}

/// 派生固定账户标识，不把 sr25519 账户重新解释为 PQ 账户。
pub fn account_id(public: &PublicKey) -> [u8; 32] {
    let mut digest = Blake2b::<U32>::new();
    digest.update(ACCOUNT_DOMAIN);
    digest.update(public.0);
    digest.finalize().into()
}

/// 签署 SDK 的完整签名约束，覆盖扩展版本、调用和隐式字段。
/// 签名明确覆盖账户标识，避免调用方意外复用密钥时混淆账户。
pub fn transaction_message(account: &[u8; 32], implication: &[u8]) -> [u8; 32] {
    let mut digest = Blake2b::<U32>::new();
    digest.update(TRANSACTION_DOMAIN);
    digest.update(account);
    digest.update(implication);
    digest.finalize().into()
}

pub fn verify(public: &PublicKey, message: &[u8], signature: &Signature) -> bool {
    ml_dsa_65::PublicKey::try_from_bytes(public.0)
        .map(|public| public.verify(message, &signature.0, SIGNING_CONTEXT))
        .unwrap_or(false)
}

/// 使用完整 PQ 公钥验签，并核对该公钥派生的账户标识。
pub fn verify_account_signature(
    account: &[u8; 32],
    public: &PublicKey,
    message: &[u8],
    signature: &Signature,
) -> bool {
    account_id(public) == *account && verify(public, message, signature)
}

/// 两个原生消费者使用同一子账户种子和域分隔规则，私密材料不写入链上。
#[cfg(feature = "signing")]
pub fn seed_from_account(child: &[u8; 32]) -> zeroize::Zeroizing<[u8; 32]> {
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<sha2::Sha512>::new(Some(b"GMB/account/v1"), child)
        .expand(b"GMB/account/ml-dsa-65", seed.as_mut())
        .expect("32 bytes is within the HKDF-SHA512 output bound");
    seed
}

#[cfg(feature = "signing")]
pub fn public_key(child: &[u8; 32]) -> PublicKey {
    use fips204::traits::KeyGen;
    let seed = seed_from_account(child);
    let (public, _secret) = ml_dsa_65::KG::keygen_from_seed(&seed);
    PublicKey(public.into_bytes())
}

/// 按 FIPS 204 执行确定性签名（rnd=0），无需运行时随机数或导出私钥。
#[cfg(feature = "signing")]
pub fn sign(child: &[u8; 32], message: &[u8]) -> Result<Signature, &'static str> {
    sign_with_public(child, message).map(|(_, signature)| signature)
}

#[cfg(feature = "signing")]
pub fn sign_with_public(
    child: &[u8; 32],
    message: &[u8],
) -> Result<(PublicKey, Signature), &'static str> {
    use fips204::traits::{KeyGen, Signer};
    let seed = seed_from_account(child);
    let (public, secret) = ml_dsa_65::KG::keygen_from_seed(&seed);
    let signature = secret.try_sign_with_seed(&[0; 32], message, SIGNING_CONTEXT)?;
    Ok((PublicKey(public.into_bytes()), Signature(signature)))
}

/// 统一签名证明结构，供 SDK 交易编码与钱包审阅解码共用。
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum Proof {
    #[codec(index = 0)]
    Sr25519 {
        account: [u8; 32],
        signature: [u8; SR25519_SIGNATURE_BYTES],
    },
    #[codec(index = 1)]
    First {
        account: [u8; 32],
        public: PublicKey,
        signature: Signature,
    },
    #[codec(index = 2)]
    Stored {
        account: [u8; 32],
        signature: Signature,
    },
}

/// 统一签名信封边界，返回公钥与签名字节的借用切片。
pub fn signature_parts(wire: &[u8]) -> Result<(Option<&[u8]>, &[u8]), &'static str> {
    match wire.len() {
        SR25519_SIGNATURE_BYTES => Ok((None, wire)),
        SIGNATURE_ENVELOPE_BYTES => {
            let (public, signature) = wire.split_at(PUBLIC_KEY_BYTES);
            Ok((Some(public), signature))
        }
        _ => Err("签名信封长度无效"),
    }
}

/// 二维码等传输层只提供字段，长度约束与信封组装统一由本模块执行。
pub fn join_signature_parts(
    public: Option<&[u8]>,
    signature: &[u8],
) -> Result<alloc::vec::Vec<u8>, &'static str> {
    match public {
        None if signature.len() == SR25519_SIGNATURE_BYTES => Ok(signature.to_vec()),
        Some(public) if public.len() == PUBLIC_KEY_BYTES && signature.len() == SIGNATURE_BYTES => {
            let mut wire = alloc::vec::Vec::with_capacity(SIGNATURE_ENVELOPE_BYTES);
            wire.extend_from_slice(public);
            wire.extend_from_slice(signature);
            Ok(wire)
        }
        _ => Err("公钥与签名字段长度不匹配"),
    }
}

/// 固定类型保证长度有效，客户端无需自行拼接 PQ 信封。
pub fn encode_pq_signature(public: &PublicKey, signature: &Signature) -> alloc::vec::Vec<u8> {
    join_signature_parts(Some(&public.0), &signature.0).expect("固定类型的签名长度必须有效")
}

pub fn decode_pq_signature(wire: &[u8]) -> Result<(PublicKey, Signature), &'static str> {
    let (public, signature) = signature_parts(wire)?;
    let public = public.ok_or("需要 PQ 签名信封")?;
    Ok((
        PublicKey(public.try_into().map_err(|_| "PQ 公钥长度无效")?),
        Signature(signature.try_into().map_err(|_| "PQ 签名长度无效")?),
    ))
}

/// 首次与后续交易的证明选择集中在此，后续交易不复制公钥。
pub fn transaction_proof(
    account: [u8; 32],
    public: &PublicKey,
    signature: &Signature,
    registered: bool,
) -> Proof {
    if registered {
        Proof::Stored {
            account,
            signature: signature.clone(),
        }
    } else {
        Proof::First {
            account,
            public: public.clone(),
            signature: signature.clone(),
        }
    }
}

/// 验证联网端收到的完整信封，再使用统一证明规则组装交易。
pub fn verified_transaction(
    account: [u8; 32],
    wire: &[u8],
    review: &[u8],
    registered: bool,
    extensions: &[u8],
    call: &[u8],
) -> Result<alloc::vec::Vec<u8>, &'static str> {
    let (public, signature) = decode_pq_signature(wire)?;
    if !verify_account_signature(
        &account,
        &public,
        &review_message(&account, review),
        &signature,
    ) {
        return Err("PQ 签名与账户或交易不匹配");
    }
    Ok(encode_transaction(
        &transaction_proof(account, &public, &signature, registered),
        extensions,
        call,
    ))
}

/// 本地签名仅派生一次密钥；签名约束、证明选择与交易编码均由本模块处理。
#[cfg(feature = "signing")]
pub fn sign_transaction(
    child: &[u8; 32],
    review: &[u8],
    registered: bool,
    extensions: &[u8],
    call: &[u8],
) -> Result<alloc::vec::Vec<u8>, &'static str> {
    use fips204::traits::{KeyGen, Signer};
    let seed = seed_from_account(child);
    let (public, secret) = ml_dsa_65::KG::keygen_from_seed(&seed);
    let public = PublicKey(public.into_bytes());
    let account = account_id(&public);
    let message = review_message(&account, review);
    let signature = Signature(secret.try_sign_with_seed(&[0; 32], &message, SIGNING_CONTEXT)?);
    Ok(encode_transaction(
        &transaction_proof(account, &public, &signature, registered),
        extensions,
        call,
    ))
}

/// 二维码传输保持有界、无状态，帧合并与身份认证分别处理。
pub const QR_SINGLE_CODE_BYTES: usize = 2331;
pub const QR_FRAME_BYTES: usize = 1200;
pub const QR_MAX_BYTES: usize = 65_536;
pub const QR_MAX_FRAMES: usize = 64;

pub fn qr_frames(text: &str) -> Result<alloc::vec::Vec<alloc::string::String>, &'static str> {
    use alloc::{format, string::ToString};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    if text.is_empty() || text.len() > QR_MAX_BYTES {
        return Err("QR payload exceeds bound");
    }
    if text.len() <= QR_SINGLE_CODE_BYTES && !text.contains('\n') {
        return Ok(alloc::vec![text.to_string()]);
    }
    let digest = hash(text.as_bytes());
    let id: alloc::string::String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let count = text.len().div_ceil(QR_FRAME_BYTES);
    Ok(text
        .as_bytes()
        .chunks(QR_FRAME_BYTES)
        .enumerate()
        .map(|(index, bytes)| {
            format!(
                "GMBQR1:{id}:{index}:{count}:{}",
                URL_SAFE_NO_PAD.encode(bytes)
            )
        })
        .collect())
}

/// 无状态、有界的帧合并实现，供原生 SDK 与离线钱包 FFI 共用。
/// 允许重复帧，拒绝冲突帧与不同消息混合。
pub fn qr_join(input: &str) -> Result<Option<alloc::string::String>, &'static str> {
    use alloc::{string::ToString, vec::Vec};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    if !input.starts_with("GMBQR1:") {
        return if !input.is_empty() && input.len() <= QR_MAX_BYTES {
            Ok(Some(input.to_string()))
        } else {
            Err("QR payload exceeds bound")
        };
    }
    if input.len() > QR_MAX_BYTES * 3 {
        return Err("QR frame collection exceeds bound");
    }
    let mut id = None;
    let mut parts: Vec<Option<Vec<u8>>> = Vec::new();
    for (seen, frame) in input.lines().enumerate() {
        if seen >= QR_MAX_FRAMES * 2 {
            return Err("too many QR frames");
        }
        let values: Vec<_> = frame.splitn(5, ':').collect();
        if values.len() != 5 || values[0] != "GMBQR1" || values[1].len() != 64 {
            return Err("invalid QR frame");
        }
        let index: usize = values[2].parse().map_err(|_| "invalid QR index")?;
        let count: usize = values[3].parse().map_err(|_| "invalid QR count")?;
        if count == 0 || count > QR_MAX_FRAMES || index >= count {
            return Err("invalid QR frame bounds");
        }
        match id {
            None => {
                id = Some(values[1]);
                parts.resize(count, None);
            }
            Some(previous) if previous != values[1] || parts.len() != count => {
                return Err("mixed QR messages")
            }
            _ => {}
        }
        if values[4].len() > QR_FRAME_BYTES * 4 / 3
            || index.to_string() != values[2]
            || count.to_string() != values[3]
        {
            return Err("noncanonical QR frame");
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(values[4])
            .map_err(|_| "invalid QR base64")?;
        if bytes.is_empty()
            || bytes.len() > QR_FRAME_BYTES
            || URL_SAFE_NO_PAD.encode(&bytes) != values[4]
        {
            return Err("noncanonical QR frame");
        }
        if parts[index].as_ref().is_some_and(|old| old != &bytes) {
            return Err("conflicting QR frame");
        }
        parts[index] = Some(bytes);
    }
    if parts.iter().any(Option::is_none) {
        return Ok(None);
    }
    let size: usize = parts.iter().flatten().map(Vec::len).sum();
    if size > QR_MAX_BYTES {
        return Err("QR payload exceeds bound");
    }
    let bytes: Vec<_> = parts.into_iter().flatten().flatten().collect();
    let digest = hash(&bytes);
    let expected: alloc::string::String = digest
        .iter()
        .map(|byte| alloc::format!("{byte:02x}"))
        .collect();
    if id != Some(expected.as_str()) {
        return Err("QR digest mismatch");
    }
    alloc::string::String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "invalid QR UTF-8")
}

/// SDK 与钱包共用原生接口，不依赖 FRAME，不创建句柄或由原生端持有的缓冲区。
#[cfg(feature = "ffi")]
pub mod ffi {
    use super::*;
    use core::{ptr, slice};
    #[no_mangle]
    pub extern "C" fn gmb_mldsa_default_algorithm() -> u8 {
        AccountAlgorithm::default() as u8
    }
    fn guarded(body: impl FnOnce() -> Result<(), &'static str>) -> i32 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).map_or(-1, |result| {
            if result.is_ok() {
                0
            } else {
                -1
            }
        })
    }
    unsafe fn input<'a>(
        bytes: *const u8,
        length: usize,
        bound: usize,
    ) -> Result<&'a [u8], &'static str> {
        if length > bound || (length > 0 && bytes.is_null()) {
            return Err("invalid input");
        }
        Ok(if length == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(bytes, length) }
        })
    }
    unsafe fn output(
        bytes: &[u8],
        out: *mut u8,
        capacity: usize,
        required: *mut usize,
    ) -> Result<(), &'static str> {
        if required.is_null() {
            return Err("missing output length");
        }
        unsafe { ptr::write(required, bytes.len()) };
        if out.is_null() && capacity == 0 {
            return Ok(());
        }
        if out.is_null() || capacity < bytes.len() {
            return Err("output too small");
        }
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
        Ok(())
    }
    /// 调用方须提供可读的 32 字节种子及可写的 1952 字节输出缓冲区。
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_public_key(child: *const u8, out: *mut u8) -> i32 {
        guarded(|| {
            if out.is_null() {
                return Err("missing output");
            }
            let child: &[u8; 32] = unsafe { input(child, 32, 32)? }
                .try_into()
                .map_err(|_| "seed length")?;
            let public = public_key(child);
            unsafe { ptr::copy_nonoverlapping(public.0.as_ptr(), out, PUBLIC_KEY_BYTES) };
            Ok(())
        })
    }
    /// 调用方须提供容纳 1952 字节公钥与 3309 字节签名的输出缓冲区。
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_sign(
        child: *const u8,
        message: *const u8,
        length: usize,
        public_out: *mut u8,
        signature_out: *mut u8,
    ) -> i32 {
        guarded(|| {
            if public_out.is_null() || signature_out.is_null() {
                return Err("missing output");
            }
            let child: &[u8; 32] = unsafe { input(child, 32, 32)? }
                .try_into()
                .map_err(|_| "seed length")?;
            let message = unsafe { input(message, length, 16 * 1024 * 1024)? };
            let (public, signature) = sign_with_public(child, message)?;
            unsafe {
                ptr::copy_nonoverlapping(public.0.as_ptr(), public_out, PUBLIC_KEY_BYTES);
                ptr::copy_nonoverlapping(signature.0.as_ptr(), signature_out, SIGNATURE_BYTES);
            }
            Ok(())
        })
    }
    /// 调用方提供固定长度公钥、签名及可写的完整信封缓冲区。
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_signature_encode(
        public: *const u8,
        signature: *const u8,
        out: *mut u8,
    ) -> i32 {
        guarded(|| {
            if out.is_null() {
                return Err("缺少签名信封输出");
            }
            let public = PublicKey(
                unsafe { input(public, PUBLIC_KEY_BYTES, PUBLIC_KEY_BYTES)? }
                    .try_into()
                    .map_err(|_| "公钥长度无效")?,
            );
            let signature = Signature(
                unsafe { input(signature, SIGNATURE_BYTES, SIGNATURE_BYTES)? }
                    .try_into()
                    .map_err(|_| "签名长度无效")?,
            );
            let wire = encode_pq_signature(&public, &signature);
            unsafe { ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len()) };
            Ok(())
        })
    }

    /// 输出缓冲区分别容纳最大公钥和签名，算法编号决定各输出的有效长度。
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_signature_decode(
        wire: *const u8,
        length: usize,
        public_out: *mut u8,
        signature_out: *mut u8,
        algorithm_out: *mut u8,
    ) -> i32 {
        guarded(|| {
            if public_out.is_null() || signature_out.is_null() || algorithm_out.is_null() {
                return Err("缺少签名解码输出");
            }
            let (public, signature) =
                signature_parts(unsafe { input(wire, length, SIGNATURE_ENVELOPE_BYTES)? })?;
            if let Some(public) = public {
                unsafe { ptr::copy_nonoverlapping(public.as_ptr(), public_out, public.len()) };
            }
            unsafe {
                ptr::copy_nonoverlapping(signature.as_ptr(), signature_out, signature.len());
                ptr::write(
                    algorithm_out,
                    if public.is_some() {
                        MLDSA65_ALGORITHM
                    } else {
                        SR25519_ALGORITHM
                    },
                );
            }
            Ok(())
        })
    }

    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_account_id(public: *const u8, out: *mut u8) -> i32 {
        guarded(|| {
            if out.is_null() {
                return Err("missing output");
            }
            let public = PublicKey(
                unsafe { input(public, PUBLIC_KEY_BYTES, PUBLIC_KEY_BYTES)? }
                    .try_into()
                    .map_err(|_| "public length")?,
            );
            unsafe { ptr::copy_nonoverlapping(account_id(&public).as_ptr(), out, 32) };
            Ok(())
        })
    }
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_verify(
        public: *const u8,
        signature: *const u8,
        message: *const u8,
        length: usize,
    ) -> i32 {
        guarded(|| {
            let public = PublicKey(
                unsafe { input(public, PUBLIC_KEY_BYTES, PUBLIC_KEY_BYTES)? }
                    .try_into()
                    .map_err(|_| "public length")?,
            );
            let signature = Signature(
                unsafe { input(signature, SIGNATURE_BYTES, SIGNATURE_BYTES)? }
                    .try_into()
                    .map_err(|_| "signature length")?,
            );
            if !verify(
                &public,
                unsafe { input(message, length, 16 * 1024 * 1024)? },
                &signature,
            ) {
                return Err("bad signature");
            }
            Ok(())
        })
    }
    #[no_mangle]
    pub unsafe extern "C" fn gmb_mldsa_review_message(
        account: *const u8,
        review: *const u8,
        length: usize,
        out: *mut u8,
    ) -> i32 {
        guarded(|| {
            if out.is_null() {
                return Err("missing output");
            }
            let account = unsafe { input(account, 32, 32)? }
                .try_into()
                .map_err(|_| "account length")?;
            let message =
                review_message(account, unsafe { input(review, length, 16 * 1024 * 1024)? });
            unsafe { ptr::copy_nonoverlapping(message.as_ptr(), out, 32) };
            Ok(())
        })
    }
    #[no_mangle]
    pub unsafe extern "C" fn gmb_qr_encode(
        text: *const u8,
        length: usize,
        out: *mut u8,
        capacity: usize,
        required: *mut usize,
    ) -> i32 {
        guarded(|| {
            let text = core::str::from_utf8(unsafe { input(text, length, QR_MAX_BYTES)? })
                .map_err(|_| "UTF-8")?;
            let encoded = qr_frames(text)?.join("\n");
            unsafe { output(encoded.as_bytes(), out, capacity, required) }
        })
    }
    #[no_mangle]
    pub unsafe extern "C" fn gmb_qr_join(
        text: *const u8,
        length: usize,
        out: *mut u8,
        capacity: usize,
        required: *mut usize,
    ) -> i32 {
        let mut pending = false;
        let status = guarded(|| {
            let text = core::str::from_utf8(unsafe { input(text, length, QR_MAX_BYTES * 3)? })
                .map_err(|_| "UTF-8")?;
            if let Some(text) = qr_join(text)? {
                unsafe { output(text.as_bytes(), out, capacity, required) }
            } else {
                pending = true;
                if !required.is_null() {
                    unsafe { ptr::write(required, 0) }
                }
                Ok(())
            }
        });
        if status == 0 && pending {
            1
        } else {
            status
        }
    }
}

#[cfg(feature = "runtime")]
pub mod weights;

#[cfg(feature = "runtime")]
pub use runtime::*;

#[cfg(feature = "runtime")]
mod runtime {
    use super::*;
    use core::marker::PhantomData;
    use frame_support::{
        pallet_prelude::*,
        traits::{Get, OriginTrait},
        CloneNoBound, EqNoBound, PartialEqNoBound,
    };
    use sp_runtime::{
        traits::{
            AsTransactionAuthorizedOrigin, DispatchInfoOf, Dispatchable, PostDispatchInfoOf,
            TransactionExtension, ValidateResult,
        },
        transaction_validity::{InvalidTransaction, TransactionValidityError},
        AccountId32, DispatchResult,
    };

    pub trait ChainPhase {
        fn is_operation() -> bool;
        fn enter_operation();
    }

    /// 在本模块适配既有阶段存储，创世模块不依赖签名模块。
    pub struct GenesisChainPhase<T>(PhantomData<T>);

    impl<T: genesis::Config> ChainPhase for GenesisChainPhase<T> {
        fn is_operation() -> bool {
            genesis::Phase::<T>::get() == genesis::ChainPhase::Operation
        }

        fn enter_operation() {
            if !Self::is_operation() {
                genesis::Phase::<T>::put(genesis::ChainPhase::Operation);
                genesis::DeveloperUpgradeEnabled::<T>::put(false);
            }
        }
    }

    #[frame_support::pallet]
    pub mod pallet {
        use super::*;
        use frame_system::pallet_prelude::BlockNumberFor;
        #[pallet::pallet]
        pub struct Pallet<T>(_);
        #[pallet::config]
        pub trait Config: frame_system::Config<AccountId = AccountId32> {
            type ChainPhase: ChainPhase;
            type WeightInfo: crate::weights::WeightInfo;
        }
        #[pallet::storage]
        pub type PqPublicKey<T: Config> =
            StorageMap<_, Identity, AccountId32, PublicKey, OptionQuery>;

        #[pallet::hooks]
        impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
            fn on_runtime_upgrade() -> Weight {
                if cfg!(feature = "operation") && !T::ChainPhase::is_operation() {
                    T::ChainPhase::enter_operation();
                    T::DbWeight::get().reads_writes(2, 2)
                } else {
                    T::DbWeight::get().reads(1)
                }
            }
        }
    }
    pub use pallet::*;

    /// 交易外层证明与既有身份凭证钩子共用账户验签实现。
    pub fn verify_account<T: Config>(
        account: &AccountId32,
        message: &[u8],
        signature: &[u8],
    ) -> bool {
        if let Some(public) = PqPublicKey::<T>::get(account) {
            let Ok(raw) = <[u8; SIGNATURE_BYTES]>::try_from(signature) else {
                return false;
            };
            return crate::verify(&public, message, &Signature(raw));
        }
        if T::ChainPhase::is_operation() {
            return false;
        }
        let Ok(raw) = <[u8; 64]>::try_from(signature) else {
            return false;
        };
        let raw_account: [u8; 32] = *account.as_ref();
        sp_io::crypto::sr25519_verify(&raw.into(), message, &raw_account.into())
    }

    #[derive(
        Encode, Decode, DecodeWithMemTracking, CloneNoBound, EqNoBound, PartialEqNoBound, TypeInfo,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Authenticate<T>(pub Proof, #[codec(skip)] PhantomData<fn() -> T>);
    impl<T> core::fmt::Debug for Authenticate<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            self.0.fmt(f)
        }
    }
    impl<T> From<Proof> for Authenticate<T> {
        fn from(proof: Proof) -> Self {
            Self(proof, PhantomData)
        }
    }

    impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for Authenticate<T>
    where
        <T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
    {
        const IDENTIFIER: &'static str = "AccountAuth";
        type Implicit = ();
        type Val = Option<(AccountId32, PublicKey)>;
        type Pre = ();
        fn weight(&self, _: &T::RuntimeCall) -> Weight {
            use crate::weights::WeightInfo;
            match &self.0 {
                Proof::Sr25519 { .. } => T::WeightInfo::sr(),
                Proof::First { .. } => T::WeightInfo::first(),
                Proof::Stored { .. } => T::WeightInfo::stored(),
            }
        }
        fn validate(
            &self,
            mut origin: T::RuntimeOrigin,
            _: &T::RuntimeCall,
            _: &DispatchInfoOf<T::RuntimeCall>,
            _: usize,
            _: (),
            implication: &impl Encode,
            _: TransactionSource,
        ) -> ValidateResult<Self::Val, T::RuntimeCall> {
            if origin.is_transaction_authorized() {
                return Err(InvalidTransaction::BadSigner.into());
            }
            let (account, signature, public, pending) = match &self.0 {
                Proof::Sr25519 { account, signature } => {
                    let id = AccountId32::new(*account);
                    if T::ChainPhase::is_operation() || PqPublicKey::<T>::contains_key(&id) {
                        return Err(InvalidTransaction::BadSigner.into());
                    }
                    let message = crate::transaction_message(account, &implication.encode());
                    if !sp_io::crypto::sr25519_verify(
                        &sp_core::sr25519::Signature::from_raw(*signature),
                        &message,
                        &sp_core::sr25519::Public::from_raw(*account),
                    ) {
                        return Err(InvalidTransaction::BadProof.into());
                    }
                    origin.set_caller_from_signed(id);
                    return Ok((Default::default(), None, origin));
                }
                Proof::First {
                    account,
                    public,
                    signature,
                } => {
                    if crate::account_id(public) != *account {
                        return Err(InvalidTransaction::BadSigner.into());
                    }
                    let id = AccountId32::new(*account);
                    let existing = PqPublicKey::<T>::get(&id);
                    if existing.as_ref().is_some_and(|key| key != public) {
                        return Err(InvalidTransaction::BadSigner.into());
                    }
                    (
                        *account,
                        signature,
                        public.clone(),
                        if existing.is_none() {
                            Some((id, public.clone()))
                        } else {
                            None
                        },
                    )
                }
                Proof::Stored { account, signature } => {
                    let public = PqPublicKey::<T>::get(AccountId32::new(*account))
                        .ok_or(InvalidTransaction::BadSigner)?;
                    (*account, signature, public, None)
                }
            };
            let message = crate::transaction_message(&account, &implication.encode());
            if !crate::verify(&public, &message, signature) {
                return Err(InvalidTransaction::BadProof.into());
            }
            origin.set_caller_from_signed(AccountId32::new(account));
            Ok((Default::default(), pending, origin))
        }
        fn prepare(
            self,
            pending: Self::Val,
            _: &T::RuntimeOrigin,
            _: &T::RuntimeCall,
            _: &DispatchInfoOf<T::RuntimeCall>,
            _: usize,
        ) -> Result<(), TransactionValidityError> {
            if let Some((account, public)) = pending {
                match PqPublicKey::<T>::get(&account) {
                    Some(existing) if existing != public => {
                        return Err(InvalidTransaction::BadSigner.into())
                    }
                    Some(_) => {}
                    None => PqPublicKey::<T>::insert(account, public),
                }
            }
            Ok(())
        }
        fn post_dispatch_details(
            _: (),
            _: &DispatchInfoOf<T::RuntimeCall>,
            _: &PostDispatchInfoOf<T::RuntimeCall>,
            _: usize,
            _: &DispatchResult,
        ) -> Result<Weight, TransactionValidityError> {
            Ok(Weight::zero())
        }
    }
}
