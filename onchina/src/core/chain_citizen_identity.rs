//! 公民 CID finalized 链上快照唯一读取入口。
//!
//! 所有注册局办理、公开绑定查询和本地投影必须复用本模块，并在同一个 finalized
//! 区块读取登记、正向绑定、绑定版本、反向绑定、投票身份和竞选身份。CID 是唯一
//! 身份主键；`account_id` 只表示该 finalized 版本的签名授权账户。

use codec::Decode;
use subxt::backend::legacy::LegacyRpcMethods;
use subxt::backend::rpc::RpcClient;
use subxt::dynamic;
use subxt::{OnlineClient, PolkadotConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizedCidStatus {
    Missing,
    Active,
    Revoked,
}

impl FinalizedCidStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "MISSING",
            Self::Active => "ACTIVE",
            Self::Revoked => "REVOKED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalizedVotingIdentity {
    pub(crate) passport_valid_from: u32,
    pub(crate) passport_valid_until: u32,
    pub(crate) citizen_status: u8,
    pub(crate) residence_province_code: Vec<u8>,
    pub(crate) residence_city_code: Vec<u8>,
    pub(crate) residence_town_code: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalizedCandidateIdentity {
    pub(crate) birth_province_code: Vec<u8>,
    pub(crate) birth_city_code: Vec<u8>,
    pub(crate) birth_town_code: Vec<u8>,
    pub(crate) family_name: Vec<u8>,
    pub(crate) given_name: Vec<u8>,
    pub(crate) citizen_sex: u8,
    pub(crate) birth_date: u32,
}

/// 同一个 finalized 区块中的完整 CID 公开身份快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalizedCitizenIdentity {
    pub(crate) genesis_hash: [u8; 32],
    pub(crate) finalized_block_hash: [u8; 32],
    pub(crate) finalized_block_number: u32,
    pub(crate) chain_now_seconds: u64,
    pub(crate) cid_status: FinalizedCidStatus,
    pub(crate) registrar_cid_number: Option<Vec<u8>>,
    pub(crate) commitment: Option<[u8; 32]>,
    pub(crate) residence_province_code: Vec<u8>,
    pub(crate) residence_city_code: Vec<u8>,
    pub(crate) account_id: Option<[u8; 32]>,
    pub(crate) binding_revision: Option<u64>,
    /// 该 CID 当前身份版本；每次身份写入单调 +1，尚无身份时为 0。
    pub(crate) identity_version: u64,
    pub(crate) voting: Option<FinalizedVotingIdentity>,
    pub(crate) candidate: Option<FinalizedCandidateIdentity>,
}

impl FinalizedCitizenIdentity {
    pub(crate) fn is_unoccupied(&self) -> bool {
        self.cid_status == FinalizedCidStatus::Missing
    }

    pub(crate) fn active_binding(&self) -> Option<([u8; 32], u64)> {
        match (self.cid_status, self.account_id, self.binding_revision) {
            (FinalizedCidStatus::Active, Some(account_id), Some(revision)) if revision > 0 => {
                Some((account_id, revision))
            }
            _ => None,
        }
    }

    pub(crate) fn registry_rebind_required(&self) -> bool {
        self.voting.is_some() || self.candidate.is_some()
    }
}

#[derive(Decode)]
struct RawCidRecord {
    registrar_cid_number: Vec<u8>,
    commitment: [u8; 32],
    residence_province_code: Vec<u8>,
    residence_city_code: Vec<u8>,
    status: u8,
    _registered_at: u32,
    _revoked_at: Option<u32>,
}

#[derive(Decode)]
struct RawVotingIdentity {
    passport_valid_from: u32,
    passport_valid_until: u32,
    citizen_status: u8,
    residence_province_code: Vec<u8>,
    residence_city_code: Vec<u8>,
    residence_town_code: Vec<u8>,
    _updated_at: u32,
}

#[derive(Decode)]
struct RawCandidateIdentity {
    birth_province_code: Vec<u8>,
    birth_city_code: Vec<u8>,
    birth_town_code: Vec<u8>,
    family_name: Vec<u8>,
    given_name: Vec<u8>,
    citizen_sex: u8,
    birth_date: u32,
    _updated_at: u32,
}

fn decode_all<T: Decode>(encoded: &[u8], field: &str) -> Result<T, String> {
    let mut input = encoded;
    let value = T::decode(&mut input).map_err(|e| format!("decode {field} failed: {e}"))?;
    if !input.is_empty() {
        return Err(format!(
            "decode {field} left {} trailing bytes",
            input.len()
        ));
    }
    Ok(value)
}

/// 在调用方已经固定的 finalized storage 上解析 CID 当前绑定账户。
///
/// 本入口只服务管理员签名授权，不另建身份真源：登记状态、正向绑定、绑定版本与
/// `CidByAccountId` 反向闭环仍全部读取 `CitizenIdentity`。返回 `None` 表示 CID 不存在、
/// 已撤销或没有有效当前绑定；闭环损坏则返回错误并拒绝授权。
pub(crate) async fn read_active_cid_account_id_at(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    cid_number: &str,
) -> Result<Option<[u8; 32]>, String> {
    let cid_key = || vec![dynamic::Value::from_bytes(cid_number.as_bytes())];
    let cid_record = storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "CidRegistry",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch CidRegistry {cid_number} failed: {e}"))?
        .map(|value| {
            decode_all::<RawCidRecord>(value.encoded(), &format!("CidRegistry {cid_number}"))
        })
        .transpose()?;
    let account_id = storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "AccountIdByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch AccountIdByCid {cid_number} failed: {e}"))?
        .map(|value| {
            decode_all::<[u8; 32]>(value.encoded(), &format!("AccountIdByCid {cid_number}"))
        })
        .transpose()?;
    let binding_revision = storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "BindingRevisionByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch BindingRevisionByCid {cid_number} failed: {e}"))?
        .map(|value| {
            decode_all::<u64>(
                value.encoded(),
                &format!("BindingRevisionByCid {cid_number}"),
            )
        })
        .transpose()?;
    let cid_status = match cid_record {
        None => FinalizedCidStatus::Missing,
        Some(record) => match record.status {
            0 => FinalizedCidStatus::Active,
            1 => FinalizedCidStatus::Revoked,
            other => {
                return Err(format!(
                    "CidRegistry {cid_number} has unknown status {other}"
                ));
            }
        },
    };
    validate_binding_closure(
        storage,
        cid_number,
        cid_status,
        account_id,
        binding_revision,
        false,
        false,
    )
    .await?;
    Ok(match (cid_status, account_id, binding_revision) {
        (FinalizedCidStatus::Active, Some(account_id), Some(revision)) if revision > 0 => {
            Some(account_id)
        }
        _ => None,
    })
}

/// 由当前签名账户读取其唯一有效 CID，并复核 CID → AccountId 正向绑定闭环。
pub(crate) async fn read_active_cid_number_by_account_id_at(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    account_id: [u8; 32],
) -> Result<Option<Vec<u8>>, String> {
    let Some(value) = storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "CidByAccountId",
            vec![dynamic::Value::from_bytes(account_id)],
        ))
        .await
        .map_err(|e| format!("fetch CidByAccountId failed: {e}"))?
    else {
        return Ok(None);
    };
    let cid_number = decode_all::<Vec<u8>>(value.encoded(), "CidByAccountId")?;
    let cid_number_text = std::str::from_utf8(cid_number.as_slice())
        .map_err(|_| "CidByAccountId is not UTF-8".to_string())?;
    let current_account_id = read_active_cid_account_id_at(storage, cid_number_text)
        .await?
        .ok_or_else(|| "CidByAccountId points to a CID without active binding".to_string())?;
    if current_account_id != account_id {
        return Err("CidByAccountId does not match AccountIdByCid".to_string());
    }
    Ok(Some(cid_number))
}

/// 读取最新 finalized CID 身份快照。
pub(crate) async fn read_finalized_citizen_identity(
    cid_number: &str,
) -> Result<FinalizedCitizenIdentity, String> {
    read_citizen_identity_at(cid_number, None).await
}

/// 在指定 finalized 区块读取 CID 身份快照；`None` 时先解析最新 finalized head。
pub(crate) async fn read_citizen_identity_at(
    cid_number: &str,
    block_hash: Option<[u8; 32]>,
) -> Result<FinalizedCitizenIdentity, String> {
    let ws_url = super::chain_url::chain_ws_url()?;
    let rpc_client = RpcClient::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain rpc for CID identity failed: {e}"))?;
    let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
    let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client)
        .await
        .map_err(|e| format!("connect chain client for CID identity failed: {e}"))?;
    let finalized_hash = match block_hash {
        Some(hash) => subxt::utils::H256(hash),
        None => rpc
            .chain_get_finalized_head()
            .await
            .map_err(|e| format!("fetch finalized head for CID identity failed: {e}"))?,
    };
    let finalized_header = rpc
        .chain_get_header(Some(finalized_hash))
        .await
        .map_err(|e| format!("fetch finalized CID identity header failed: {e}"))?
        .ok_or_else(|| "finalized CID identity header missing".to_string())?;
    let storage = client.storage().at(finalized_hash);
    let cid_key = || vec![dynamic::Value::from_bytes(cid_number.as_bytes())];

    let chain_now_millis = storage
        .fetch(&dynamic::storage("Timestamp", "Now", Vec::new()))
        .await
        .map_err(|e| format!("fetch finalized Timestamp.Now failed: {e}"))?
        .map(|value| decode_all::<u64>(value.encoded(), "finalized Timestamp.Now"))
        .transpose()?;
    // 创世块没有 timestamp inherent，Timestamp.Now 缺失是 Substrate 的正常状态；
    // 非创世 finalized 区块必须存在时间戳，否则授权有效期无法安全判定。
    let chain_now_millis = finalized_chain_now_millis(finalized_header.number, chain_now_millis)?;

    let cid_record = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "CidRegistry",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch CidRegistry {cid_number} failed: {e}"))?
    {
        Some(value) => Some(decode_all::<RawCidRecord>(
            value.encoded(),
            &format!("CidRegistry {cid_number}"),
        )?),
        None => None,
    };
    let account_id = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "AccountIdByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch AccountIdByCid {cid_number} failed: {e}"))?
    {
        Some(value) => Some(decode_all::<[u8; 32]>(
            value.encoded(),
            &format!("AccountIdByCid {cid_number}"),
        )?),
        None => None,
    };
    let binding_revision = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "BindingRevisionByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch BindingRevisionByCid {cid_number} failed: {e}"))?
    {
        Some(value) => Some(decode_all::<u64>(
            value.encoded(),
            &format!("BindingRevisionByCid {cid_number}"),
        )?),
        None => None,
    };
    // 身份版本：ValueQuery，键不存在即 0（尚未建立任何身份）。
    let identity_version = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "VotingEligibilityVersionCount",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch VotingEligibilityVersionCount {cid_number} failed: {e}"))?
    {
        Some(value) => decode_all::<u64>(
            value.encoded(),
            &format!("VotingEligibilityVersionCount {cid_number}"),
        )?,
        None => 0,
    };
    let voting = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "VotingIdentityByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch VotingIdentityByCid {cid_number} failed: {e}"))?
    {
        Some(value) => {
            let raw: RawVotingIdentity = decode_all(
                value.encoded(),
                &format!("VotingIdentityByCid {cid_number}"),
            )?;
            Some(FinalizedVotingIdentity {
                passport_valid_from: raw.passport_valid_from,
                passport_valid_until: raw.passport_valid_until,
                citizen_status: raw.citizen_status,
                residence_province_code: raw.residence_province_code,
                residence_city_code: raw.residence_city_code,
                residence_town_code: raw.residence_town_code,
            })
        }
        None => None,
    };
    let candidate = match storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "CandidateIdentityByCid",
            cid_key(),
        ))
        .await
        .map_err(|e| format!("fetch CandidateIdentityByCid {cid_number} failed: {e}"))?
    {
        Some(value) => {
            let raw: RawCandidateIdentity = decode_all(
                value.encoded(),
                &format!("CandidateIdentityByCid {cid_number}"),
            )?;
            Some(FinalizedCandidateIdentity {
                birth_province_code: raw.birth_province_code,
                birth_city_code: raw.birth_city_code,
                birth_town_code: raw.birth_town_code,
                family_name: raw.family_name,
                given_name: raw.given_name,
                citizen_sex: raw.citizen_sex,
                birth_date: raw.birth_date,
            })
        }
        None => None,
    };

    let (cid_status, registrar_cid_number, commitment, province, city) = match cid_record {
        None => (
            FinalizedCidStatus::Missing,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        Some(record) => {
            let status = match record.status {
                0 => FinalizedCidStatus::Active,
                1 => FinalizedCidStatus::Revoked,
                other => {
                    return Err(format!(
                        "CidRegistry {cid_number} has unknown status {other}"
                    ));
                }
            };
            (
                status,
                Some(record.registrar_cid_number),
                Some(record.commitment),
                record.residence_province_code,
                record.residence_city_code,
            )
        }
    };

    validate_binding_closure(
        &storage,
        cid_number,
        cid_status,
        account_id,
        binding_revision,
        voting.is_some(),
        candidate.is_some(),
    )
    .await?;

    Ok(FinalizedCitizenIdentity {
        genesis_hash: client.genesis_hash().0,
        finalized_block_hash: finalized_hash.0,
        finalized_block_number: finalized_header.number,
        chain_now_seconds: chain_now_millis / 1000,
        cid_status,
        registrar_cid_number,
        commitment,
        identity_version,
        residence_province_code: province,
        residence_city_code: city,
        account_id,
        binding_revision,
        voting,
        candidate,
    })
}

fn finalized_chain_now_millis(
    finalized_block_number: u32,
    chain_now_millis: Option<u64>,
) -> Result<u64, String> {
    match (finalized_block_number, chain_now_millis) {
        (_, Some(value)) => Ok(value),
        (0, None) => Ok(0),
        (_, None) => Err("finalized Timestamp.Now missing outside genesis".to_string()),
    }
}

async fn validate_binding_closure(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    cid_number: &str,
    cid_status: FinalizedCidStatus,
    account_id: Option<[u8; 32]>,
    binding_revision: Option<u64>,
    has_voting: bool,
    has_candidate: bool,
) -> Result<(), String> {
    if cid_status == FinalizedCidStatus::Missing {
        if account_id.is_some() || binding_revision.is_some() || has_voting || has_candidate {
            return Err(format!(
                "CID {cid_number} missing registry record but identity storage still exists"
            ));
        }
        return Ok(());
    }
    let revision = binding_revision
        .filter(|revision| *revision > 0)
        .ok_or_else(|| format!("CID {cid_number} binding revision missing or zero"))?;
    let _ = revision;
    let account_id =
        account_id.ok_or_else(|| format!("CID {cid_number} account binding missing"))?;
    let reverse = storage
        .fetch(&dynamic::storage(
            "CitizenIdentity",
            "CidByAccountId",
            vec![dynamic::Value::from_bytes(account_id)],
        ))
        .await
        .map_err(|e| format!("fetch CidByAccountId for {cid_number} failed: {e}"))?
        .ok_or_else(|| format!("CID {cid_number} reverse account binding missing"))
        .and_then(|value| {
            decode_all::<Vec<u8>>(value.encoded(), &format!("CidByAccountId for {cid_number}"))
        })?;
    if reverse.as_slice() != cid_number.as_bytes() {
        return Err(format!("CID {cid_number} reverse account binding mismatch"));
    }
    if has_candidate && !has_voting {
        return Err(format!(
            "CID {cid_number} candidate identity exists without voting identity"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;

    #[test]
    fn active_binding_requires_positive_revision() {
        let snapshot = FinalizedCitizenIdentity {
            genesis_hash: [0; 32],
            identity_version: 0,
            finalized_block_hash: [1; 32],
            finalized_block_number: 7,
            chain_now_seconds: 100,
            cid_status: FinalizedCidStatus::Active,
            registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
            commitment: Some([2; 32]),
            residence_province_code: b"GD".to_vec(),
            residence_city_code: b"001".to_vec(),
            account_id: Some([3; 32]),
            binding_revision: Some(0),
            voting: None,
            candidate: None,
        };
        assert_eq!(snapshot.active_binding(), None);
    }

    #[test]
    fn revoked_cid_has_no_active_binding_even_when_audit_mapping_remains() {
        let snapshot = FinalizedCitizenIdentity {
            genesis_hash: [0; 32],
            identity_version: 0,
            finalized_block_hash: [1; 32],
            finalized_block_number: 7,
            chain_now_seconds: 100,
            cid_status: FinalizedCidStatus::Revoked,
            registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
            commitment: Some([2; 32]),
            residence_province_code: b"GD".to_vec(),
            residence_city_code: b"001".to_vec(),
            account_id: Some([3; 32]),
            binding_revision: Some(3),
            voting: None,
            candidate: None,
        };
        assert_eq!(snapshot.active_binding(), None);
    }

    #[test]
    fn voting_or_candidate_requires_registry_rebind() {
        let snapshot = FinalizedCitizenIdentity {
            genesis_hash: [0; 32],
            identity_version: 0,
            finalized_block_hash: [1; 32],
            finalized_block_number: 7,
            chain_now_seconds: 100,
            cid_status: FinalizedCidStatus::Active,
            registrar_cid_number: Some(b"CN001-CREG-0001".to_vec()),
            commitment: Some([2; 32]),
            residence_province_code: b"GD".to_vec(),
            residence_city_code: b"001".to_vec(),
            account_id: Some([3; 32]),
            binding_revision: Some(2),
            voting: Some(FinalizedVotingIdentity {
                passport_valid_from: 20260101,
                passport_valid_until: 20360101,
                citizen_status: 0,
                residence_province_code: b"GD".to_vec(),
                residence_city_code: b"001".to_vec(),
                residence_town_code: b"001001".to_vec(),
            }),
            candidate: None,
        };
        assert!(snapshot.registry_rebind_required());
    }

    #[test]
    fn finalized_storage_decode_rejects_trailing_layout_bytes() {
        let mut encoded_record = (
            b"CN001-CREG-0001".to_vec(),
            [0x44u8; 32],
            Vec::<u8>::new(),
            Vec::<u8>::new(),
            0u8,
            42u32,
            Option::<u32>::None,
        )
            .encode();
        let decoded: RawCidRecord = match decode_all(&encoded_record, "CidRegistry fixture") {
            Ok(value) => value,
            Err(error) => panic!("exact record should decode: {error}"),
        };
        assert_eq!(decoded.status, 0);
        encoded_record.push(0xff);
        assert!(decode_all::<RawCidRecord>(&encoded_record, "CidRegistry fixture").is_err());
    }

    #[test]
    fn timestamp_may_only_be_missing_at_genesis() {
        assert_eq!(finalized_chain_now_millis(0, None), Ok(0));
        assert_eq!(finalized_chain_now_millis(7, Some(12_000)), Ok(12_000));
        assert!(finalized_chain_now_millis(7, None).is_err());
    }
}
