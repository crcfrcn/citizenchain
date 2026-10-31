//! 平台统一冷签会话、原机构恢复锚点与互斥租约；有效期不变。
use crate::Db;
use chrono::{DateTime, Utc};
/// 冷签会话有效期(秒)，沿用现有值。
pub const SESSION_TTL_SECS: i64 = 600;

/// 链冷签会话:prepare 只保存短期签名 payload。
///
/// 这不是公民或机构的业务草稿状态。提交前失败可删除；一旦已经尝试网络提交，
/// 必须保留恢复锚点，直到确认 finalized+ExtrinsicSuccess 并完成本地投影。
/// 公民/机构正式数据只能在链上确认成功后写入正式投影表。
pub struct ChainSignSession {
    pub request_id: String,
    pub purpose: String,
    /// 发起管理员的钱包账户 account_id（提交时签名者账户必须与之一致）。
    pub account_id: String,
    pub call_data: Vec<u8>,
    pub nonce: u32,
    /// sha256(签名输入) hex,submit 阶段重建校验防 runtime 漂移。
    pub signing_hash: String,
    pub context: serde_json::Value,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl Db {
    /// 创建冷签会话 = 发起一次链上写。
    ///
    /// `passkey` 是 [`PasskeyProof`](crate::authentication::passkey::PasskeyProof),
    /// 只能由 `require_passkey_assertion` 产出。三档鉴权铁律要求链上写 =
    /// 会话 + passkey + 冷签,这里用类型把 passkey 这一半钉死:
    /// 没做 passkey 就调本函数会编译失败,不再依赖各 handler 自觉。
    pub fn insert_chain_sign_session(
        &self,
        s: &ChainSignSession,
        passkey: &crate::authentication::passkey::PasskeyProof,
        expected_institution_cid: &str,
    ) -> Result<(), String> {
        let mut s = ChainSignSession {
            request_id: s.request_id.clone(),
            purpose: s.purpose.clone(),
            account_id: s.account_id.clone(),
            call_data: s.call_data.clone(),
            nonce: s.nonce,
            signing_hash: s.signing_hash.clone(),
            context: s.context.clone(),
            expires_at: s.expires_at,
            consumed_at: s.consumed_at,
        };
        self.with_client(move |conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
            let binding = crate::authentication::repo::get_active_node_binding_conn(&mut tx)?
                .ok_or("node binding required for chain session")?;
            if binding.institution_cid_number != expected_institution_cid
                || !passkey.matches_binding(&s.account_id, &binding)
            {
                return Err("Passkey proof belongs to another account or binding".into());
            }
            let context = s
                .context
                .as_object_mut()
                .ok_or("chain session context must be an object")?;
            context.insert(
                "origin_binding_id".into(),
                serde_json::json!(binding.binding_id),
            );
            context.insert(
                "origin_institution_cid".into(),
                serde_json::json!(binding.institution_cid_number),
            );
            context.insert(
                "origin_institution_code".into(),
                serde_json::json!(binding.institution_code),
            );
            tx.execute(
                "INSERT INTO signing
                    (request_id, sign_purpose, account_id, call_data, nonce_value, signing_hash,
                     context_json, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &s.request_id,
                    &s.purpose,
                    &s.account_id,
                    &hex::encode(&s.call_data),
                    &(s.nonce as i64),
                    &s.signing_hash,
                    &s.context,
                    &s.expires_at,
                ],
            )
            .map_err(|e| format!("insert chain sign session failed: {e}"))?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    pub fn find_chain_sign_session(
        &self,
        request_id: &str,
    ) -> Result<Option<ChainSignSession>, String> {
        let request_id = request_id.trim().to_string();
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT request_id, sign_purpose, account_id, call_data, nonce_value, signing_hash,
                            context_json, expires_at, consumed_at
                     FROM signing WHERE request_id = $1",
                    &[&request_id],
                )
                .map_err(|e| format!("query chain sign session failed: {e}"))?;
            row.map(|r| -> Result<ChainSignSession, String> { Ok(ChainSignSession {
                request_id: r.get(0),
                purpose: r.get(1),
                account_id: r.get(2),
                call_data: hex::decode(r.get::<_, String>(3)).map_err(|_| "invalid chain session call data")?,
                nonce: u32::try_from(r.get::<_, i64>(4)).map_err(|_| "invalid chain session nonce")?,
                signing_hash: r.get(5),
                context: r.get(6),
                expires_at: r.get(7),
                consumed_at: r.get(8),
            }) }).transpose()
        })
    }

    pub(crate) fn delete_chain_sign_session(&self, request_id: &str) -> Result<(), String> {
        let request_id = request_id.trim().to_string();
        self.with_client(move |conn| {
            conn.execute("DELETE FROM signing WHERE request_id = $1", &[&request_id])
                .map_err(|e| format!("delete chain sign session failed: {e}"))?;
            Ok(())
        })
    }

    /// 把占号 pending 会话(用户签名收集前的占位)升级为可提交的冷签会话:
    /// 用户签名回来后回填 call_data/nonce/signing_hash + 转正 purpose + 追加 account_id。
    pub fn promote_chain_sign_session(
        &self,
        request_id: &str,
        purpose: &str,
        call_data: &[u8],
        nonce: u32,
        signing_hash: &str,
        context: &serde_json::Value,
    ) -> Result<u64, String> {
        let request_id = request_id.trim().to_string();
        let purpose = purpose.to_string();
        let call_data = hex::encode(call_data);
        let signing_hash = signing_hash.to_string();
        let context = context.clone();
        self.with_client(move |conn| {
            conn.execute(
                "UPDATE signing
                 SET sign_purpose = $2, call_data = $3, nonce_value = $4, signing_hash = $5, context_json = $6 || jsonb_build_object(
                     'origin_binding_id',context_json->'origin_binding_id',
                     'origin_institution_cid',context_json->'origin_institution_cid',
                     'origin_institution_code',context_json->'origin_institution_code')
                 WHERE request_id = $1 AND consumed_at IS NULL",
                &[
                    &request_id,
                    &purpose,
                    &call_data,
                    &(nonce as i64),
                    &signing_hash,
                    &context,
                ],
            )
            .map_err(|e| format!("promote chain sign session failed: {e}"))
        })
    }
}

/// HTTP 提交与后台恢复共用数据库租约；网络等待期间只持有租约，不占用 SQL 连接/事务。
pub struct SessionLease {
    db: Db,
    request_id: String,
    token: String,
    renew: tokio::task::JoinHandle<()>,
}
impl SessionLease {
    pub fn acquire(db: &Db, request_id: &str) -> Result<Option<Self>, String> {
        let request_id = request_id.to_string();
        let token = uuid::Uuid::new_v4().to_string();
        let claimed = db.with_client(|conn| conn.execute("UPDATE signing SET processing_token=$2,processing_until=now()+interval '90 seconds',checked_at=now() WHERE request_id=$1 AND consumed_at IS NULL AND (processing_until IS NULL OR processing_until<now())", &[&request_id,&token]).map_err(|e| e.to_string()))?;
        if claimed != 1 {
            return Ok(None);
        }
        let renew_db = db.clone();
        let renew_id = request_id.clone();
        let renew_token = token.clone();
        let renew = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                let renewed = renew_db.with_client(|conn| conn.execute("UPDATE signing SET processing_until=now()+interval '90 seconds' WHERE request_id=$1 AND processing_token=$2 AND consumed_at IS NULL AND processing_until>now()", &[&renew_id,&renew_token]).map_err(|e| e.to_string()));
                if !matches!(renewed, Ok(1)) {
                    break;
                }
            }
        });
        Ok(Some(Self {
            db: db.clone(),
            request_id,
            token,
            renew,
        }))
    }

    pub(crate) fn update_context(&self, context: &serde_json::Value) -> Result<(), String> {
        let updated = self.db.with_client(|conn| conn.execute("UPDATE signing SET context_json=$3 || jsonb_build_object('origin_binding_id',context_json->'origin_binding_id','origin_institution_cid',context_json->'origin_institution_cid','origin_institution_code',context_json->'origin_institution_code') WHERE request_id=$1 AND processing_token=$2 AND processing_until>now() AND consumed_at IS NULL", &[&self.request_id,&self.token,&context]).map_err(|e| e.to_string()))?;
        if updated != 1 {
            return Err("chain session lease lost while saving recovery context".into());
        }
        Ok(())
    }

    /// 在正式写事务中核对租约并锁住会话，防止超时请求覆盖另一个恢复者。
    pub(crate) fn verify_conn(
        &self,
        conn: &mut impl postgres::GenericClient,
    ) -> Result<(), String> {
        let row = conn.query_opt("SELECT request_id FROM signing WHERE request_id=$1 AND processing_token=$2 AND processing_until>now() AND consumed_at IS NULL FOR UPDATE", &[&self.request_id,&self.token]).map_err(|e| e.to_string())?;
        if row.is_none() {
            return Err("chain session lease lost or already consumed".into());
        }
        Ok(())
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        self.renew.abort();
        let _ = self.db.with_client(|conn| conn.execute("UPDATE signing SET processing_token=NULL,processing_until=NULL WHERE request_id=$1 AND processing_token=$2", &[&self.request_id,&self.token]).map(|_| ()).map_err(|e| e.to_string()));
    }
}
