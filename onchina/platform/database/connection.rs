//! OnChina 结构化数据库入口。
//!
//! 本模块只负责 PostgreSQL 连接池、创世前最终 schema 初始化和短事务封装。
//! 空库按唯一目标结构事务初始化；已有结构只核验，不迁移、修补或重建。

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
};

/// 展开 PostgreSQL 的 SQLSTATE、message、detail 和 hint，保留真实启动错误。
pub(crate) fn postgres_error_text(err: &postgres::Error) -> String {
    let Some(db_error) = err.as_db_error() else {
        return err.to_string();
    };
    let mut parts = vec![format!("{} {}", db_error.code().code(), db_error.message())];
    if let Some(detail) = db_error.detail() {
        parts.push(format!("detail: {detail}"));
    }
    if let Some(hint) = db_error.hint() {
        parts.push(format!("hint: {hint}"));
    }
    parts.join("; ")
}

// PostgreSQL原生schema注释只记录格式版本；不是业务数据副本。
const SCHEMA_FORMAT: &str = "onchina:2";

#[derive(Clone)]
pub(crate) struct Db {
    clients: Arc<Vec<Mutex<postgres::Client>>>,
    next_client_idx: Arc<AtomicUsize>,
}

impl Db {
    /// 路由单测不创建数据库；任何误入持久化的请求都会明确失败。
    #[cfg(test)]
    pub(crate) fn empty_for_route_test() -> Self {
        Self {
            clients: Arc::new(Vec::new()),
            next_client_idx: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) fn from_database_url(database_url: &str) -> Result<Self, String> {
        Self::connect_database(database_url, true)
    }
    /// 模块只连接宿主已初始化的数据库，不能重复初始化或启动PostgreSQL。
    pub(crate) fn connect_existing(database_url: &str) -> Result<Self, String> {
        Self::connect_database(database_url, false)
    }
    fn connect_database(database_url: &str, initialize: bool) -> Result<Self, String> {
        let db_url = database_url.to_string();
        let pool_size = std::env::var("ONCHINA_PG_POOL_SIZE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(4);
        let handle = thread::spawn(move || {
            let mut bootstrap = postgres::Client::connect(db_url.as_str(), postgres::NoTls)
                .map_err(|err| format!("connect postgres failed: {}", postgres_error_text(&err)))?;
            if initialize {
                Self::init_current_schema(&mut bootstrap)?;
            } else if !Self::schema_ready(&mut bootstrap)? {
                return Err("onchina database is not initialized".into());
            }
            let mut clients = Vec::with_capacity(pool_size);
            clients.push(Mutex::new(bootstrap));
            for _ in 1..pool_size {
                let client =
                    postgres::Client::connect(db_url.as_str(), postgres::NoTls).map_err(|err| {
                        format!(
                            "connect postgres pool client failed: {}",
                            postgres_error_text(&err)
                        )
                    })?;
                clients.push(Mutex::new(client));
            }
            Ok::<Vec<Mutex<postgres::Client>>, String>(clients)
        });
        let clients = match handle.join() {
            Ok(result) => result?,
            Err(_) => return Err("postgres init thread panicked".to_string()),
        };
        Ok(Self {
            clients: Arc::new(clients),
            next_client_idx: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub(crate) fn with_client<R>(
        &self,
        op: impl FnOnce(&mut postgres::Client) -> Result<R, String> + Send,
    ) -> Result<R, String>
    where
        R: Send,
    {
        if self.clients.is_empty() {
            return Err("postgres client pool is empty".to_string());
        }
        let index = self.next_client_idx.fetch_add(1, Ordering::Relaxed) % self.clients.len();
        let clients = Arc::clone(&self.clients);
        thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let mut client = clients[index]
                    .lock()
                    .map_err(|_| "postgres client lock poisoned".to_string())?;
                op(&mut client)
            });
            match handle.join() {
                Ok(result) => result,
                Err(_) => Err("postgres worker thread panicked".to_string()),
            }
        })
    }

    /// 既有库只读核验，禁止在旧库旁创建新表或补修损坏结构。
    fn schema_ready(conn: &mut impl postgres::GenericClient) -> Result<bool, String> {
        let row=conn.query_one("SELECT obj_description(n.oid,'pg_namespace'),
            (SELECT count(*) FROM pg_catalog.pg_class c WHERE c.relnamespace=n.oid AND c.relkind IN ('r','p') AND NOT c.relispartition),
            (SELECT count(*) FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid WHERE c.relnamespace=n.oid AND c.relkind IN ('r','p') AND NOT c.relispartition AND a.attnum>0 AND NOT a.attisdropped),
            NOT EXISTS(SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid WHERE c.relnamespace=n.oid AND c.relkind IN ('r','p') AND a.attnum>0 AND NOT a.attisdropped AND (c.relname !~ '^[a-z]+$' OR a.attname !~ '^[a-z]+_[a-z]+$')),
            (SELECT count(*) FROM pg_catalog.pg_class c WHERE c.relnamespace=n.oid AND c.relkind IN ('r','p','v','m','f','S'))
            FROM pg_catalog.pg_namespace n WHERE n.nspname=current_schema()", &[]).map_err(|e| format!("inspect onchina schema failed: {e}"))?;
        let format: Option<String> = row.get(0);
        let tables: i64 = row.get(1);
        let columns: i64 = row.get(2);
        let names: bool = row.get(3);
        let objects: i64 = row.get(4);
        // 新建命名空间或PostgreSQL官方空public空间，才允许第一次初始化。
        if objects == 0 && matches!(format.as_deref(), None | Some("standard public schema")) {
            return Ok(false);
        }
        if format.as_deref() == Some(SCHEMA_FORMAT) && tables == 32 && columns == 250 && names {
            return Ok(true);
        }
        Err("onchina database format mismatch; explicit data operation required".into())
    }

    fn init_current_schema(conn: &mut postgres::Client) -> Result<(), String> {
        if Self::schema_ready(conn)? {
            return Ok(());
        }
        let mut tx = conn
            .transaction()
            .map_err(|e| format!("begin schema initialization failed: {e}"))?;

        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS operation (
                operation_id TEXT PRIMARY KEY,
                registrar_account TEXT NOT NULL
                    CHECK (registrar_account ~ '^0x[0-9a-f]{64}$'),
                institution_code TEXT NOT NULL,
                actor_role TEXT NOT NULL,
                cid_number TEXT NOT NULL,
                citizen_account TEXT NOT NULL
                    CHECK (citizen_account ~ '^0x[0-9a-f]{64}$'),
                identity_level TEXT NOT NULL,
                payload_hex TEXT NOT NULL,
                identity_version BIGINT NOT NULL,
                authorization_until BIGINT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                signed_at TIMESTAMPTZ
             );
             CREATE INDEX IF NOT EXISTS idx_citizen_onchain_operations_expiry
                ON operation(expires_at);

             CREATE TABLE IF NOT EXISTS signing (
                request_id TEXT PRIMARY KEY,
                sign_purpose TEXT NOT NULL,
                account_id TEXT NOT NULL
                    CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                call_data TEXT NOT NULL,
                nonce_value BIGINT NOT NULL,
                signing_hash TEXT NOT NULL,
                context_json JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ,
                processing_token TEXT,
                processing_until TIMESTAMPTZ,
                checked_at TIMESTAMPTZ
             );
             CREATE INDEX IF NOT EXISTS idx_chain_sign_sessions_expiry
                ON signing(expires_at) WHERE consumed_at IS NULL;

             CREATE TABLE IF NOT EXISTS admin (
                admin_id BIGINT PRIMARY KEY,
                account_id TEXT NOT NULL UNIQUE CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                family_name TEXT NOT NULL,
                given_name TEXT NOT NULL,
                institution_code TEXT NOT NULL,
                institution_cid TEXT,
                built_in BOOLEAN NOT NULL DEFAULT FALSE,
                creator_account TEXT NOT NULL
                    CHECK (creator_account ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ,
                city_name TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS idx_admins_institution_code
                ON admin(institution_code);
             CREATE INDEX IF NOT EXISTS idx_admins_institution_code_city_name
                ON admin(institution_code, city_name);
             CREATE INDEX IF NOT EXISTS idx_admins_institution_cid ON admin(institution_cid);
             CREATE INDEX IF NOT EXISTS idx_admins_account_id ON admin(account_id);
             CREATE INDEX IF NOT EXISTS idx_admins_creator_account_id
                ON admin(creator_account);

             CREATE TABLE IF NOT EXISTS challenge (
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['actionId','actorAccountId','actionType','expiresAt','consumed'] AND jsonb_typeof(payload_json->'actionId') = 'string' AND jsonb_typeof(payload_json->'actorAccountId') = 'string' AND jsonb_typeof(payload_json->'actionType') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'consumed') = 'boolean' AND payload_json->>'actorAccountId' ~ '^0x[0-9a-f]{64}$')
             );
             CREATE UNIQUE INDEX IF NOT EXISTS challenge_identity ON challenge((payload_json->>'actionId'));
             CREATE INDEX IF NOT EXISTS challenge_expiry ON challenge(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS challenge_account ON challenge((payload_json->>'actorAccountId'));

             CREATE TABLE IF NOT EXISTS \"grant\" (
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['grantId','actorAccountId','actionType','expiresAt','consumed'] AND jsonb_typeof(payload_json->'grantId') = 'string' AND jsonb_typeof(payload_json->'actorAccountId') = 'string' AND jsonb_typeof(payload_json->'actionType') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'consumed') = 'boolean' AND payload_json->>'actorAccountId' ~ '^0x[0-9a-f]{64}$')
             );
             CREATE UNIQUE INDEX IF NOT EXISTS grant_identity ON \"grant\"((payload_json->>'grantId'));
             CREATE INDEX IF NOT EXISTS grant_expiry ON \"grant\"(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS grant_account ON \"grant\"((payload_json->>'actorAccountId'));

             CREATE TABLE IF NOT EXISTS login (
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['challengeId','sessionId','accountId','expiresAt','consumed'] AND jsonb_typeof(payload_json->'challengeId') = 'string' AND jsonb_typeof(payload_json->'sessionId') = 'string' AND jsonb_typeof(payload_json->'accountId') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'consumed') = 'boolean' AND payload_json->>'accountId' ~ '^0x[0-9a-f]{64}$')
             );
             CREATE UNIQUE INDEX IF NOT EXISTS login_identity ON login((payload_json->>'challengeId'));
             CREATE INDEX IF NOT EXISTS login_expiry ON login(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS login_account ON login((payload_json->>'accountId'));

             CREATE TABLE IF NOT EXISTS binding (
                binding_id TEXT PRIMARY KEY,
                candidate_id TEXT NOT NULL,
                institution_code TEXT NOT NULL,
                institution_cid TEXT NOT NULL,
                bound_account TEXT NOT NULL
                    CHECK (bound_account ~ '^0x[0-9a-f]{64}$'),
                bound_at TIMESTAMPTZ NOT NULL,
                state_code TEXT NOT NULL CHECK (state_code IN ('ACTIVE', 'INACTIVE'))
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_node_binding_one_active
                ON binding ((state_code)) WHERE state_code = 'ACTIVE';

             CREATE TABLE IF NOT EXISTS enrollment (
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['bindingChallengeId','accountId','expiresAt','consumed'] AND jsonb_typeof(payload_json->'bindingChallengeId') = 'string' AND jsonb_typeof(payload_json->'accountId') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'consumed') = 'boolean' AND payload_json->>'accountId' ~ '^0x[0-9a-f]{64}$')
             );
             CREATE UNIQUE INDEX IF NOT EXISTS enrollment_identity ON enrollment((payload_json->>'bindingChallengeId'));
             CREATE INDEX IF NOT EXISTS enrollment_expiry ON enrollment(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS enrollment_account ON enrollment((payload_json->>'accountId'));

             CREATE TABLE IF NOT EXISTS result (
                challenge_id TEXT PRIMARY KEY,
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['sessionId','accessToken','accountId','institutionCode','expiresAt','createdAt'] AND jsonb_typeof(payload_json->'sessionId') = 'string' AND jsonb_typeof(payload_json->'accessToken') = 'string' AND jsonb_typeof(payload_json->'accountId') = 'string' AND jsonb_typeof(payload_json->'institutionCode') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'createdAt') = 'string' AND payload_json->>'accountId' ~ '^0x[0-9a-f]{64}$')
             );
             CREATE INDEX IF NOT EXISTS result_expiry ON result(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS result_account ON result((payload_json->>'accountId'));
             CREATE INDEX IF NOT EXISTS result_session ON result((payload_json->>'sessionId'));

             CREATE TABLE IF NOT EXISTS session (
                token_hash TEXT PRIMARY KEY CHECK(token_hash ~ '^[0-9a-f]{64}$'),
                payload_json JSONB NOT NULL CHECK (jsonb_typeof(payload_json) = 'object' AND payload_json ?& ARRAY['accountId','institutionCode','candidateId','expiresAt','lastActiveAt'] AND jsonb_typeof(payload_json->'accountId') = 'string' AND jsonb_typeof(payload_json->'institutionCode') = 'string' AND jsonb_typeof(payload_json->'candidateId') = 'string' AND jsonb_typeof(payload_json->'expiresAt') = 'number' AND jsonb_typeof(payload_json->'lastActiveAt') = 'number' AND payload_json->>'accountId' ~ '^0x[0-9a-f]{64}$' AND NOT (payload_json ? 'token'))
             );
             CREATE INDEX IF NOT EXISTS session_expiry ON session(((payload_json->>'expiresAt')::bigint));
             CREATE INDEX IF NOT EXISTS session_account ON session((payload_json->>'accountId'));
             CREATE INDEX IF NOT EXISTS session_activity ON session((payload_json->>'institutionCode'),((payload_json->>'lastActiveAt')::bigint));

             CREATE TABLE IF NOT EXISTS credential (
                credential_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                passkey_data JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_credentials_account
                ON credential(account_id);

             CREATE TABLE IF NOT EXISTS ceremony (
                ceremony_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                record_kind TEXT NOT NULL CHECK (record_kind IN ('REG', 'AUTH')),
                state_data JSONB NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_ceremonies_expires
                ON ceremony(expires_at);

             CREATE TABLE IF NOT EXISTS assertion (
                assertion_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_assertions_expires
                ON assertion(expires_at);

             CREATE TABLE IF NOT EXISTS replay (
                qr_id TEXT PRIMARY KEY,
                consumed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_qr_consumed_expires ON replay(expires_at);

             CREATE TABLE IF NOT EXISTS request (
                route_key TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                nonce_value TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                received_at TIMESTAMPTZ NOT NULL,
                payload_json JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_chain_requests_received
                ON request(received_at);

             CREATE TABLE IF NOT EXISTS nonce (
                nonce_value TEXT PRIMARY KEY,
                seen_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_chain_nonces_seen ON nonce(seen_at);

             CREATE TABLE IF NOT EXISTS \"transaction\" (
                row_id BIGSERIAL PRIMARY KEY,
                block_number BIGINT NOT NULL,
                extrinsic_index SMALLINT,
                event_index SMALLINT NOT NULL,
                tx_type TEXT NOT NULL,
                sender_account TEXT
                    CHECK (sender_account IS NULL OR sender_account ~ '^0x[0-9a-f]{64}$'),
                recipient_account TEXT
                    CHECK (recipient_account IS NULL OR recipient_account ~ '^0x[0-9a-f]{64}$'),
                amount_fen BIGINT NOT NULL,
                fee_fen BIGINT,
                block_timestamp TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE INDEX IF NOT EXISTS idx_tx_records_sender_account_id
                ON \"transaction\"(sender_account, block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_recipient_account_id
                ON \"transaction\"(recipient_account, block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_block
                ON \"transaction\"(block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_type ON \"transaction\"(tx_type);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_tx_records_event
                ON \"transaction\"(block_number, event_index);

             CREATE TABLE IF NOT EXISTS projection (
                projection_key TEXT PRIMARY KEY,
                genesis_hash TEXT NOT NULL,
                block_hash TEXT NOT NULL DEFAULT '',
                block_number BIGINT,
                item_count BIGINT NOT NULL DEFAULT 0,
                account_count BIGINT NOT NULL DEFAULT 0,
                state_code TEXT NOT NULL CHECK (state_code IN ('OK', 'FAILED')),
                synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );",
        )
        .map_err(|err| format!("init core schema failed: {}", postgres_error_text(&err)))?;
        Self::init_subject_partition_schema(&mut tx)?;
        tx.batch_execute(crate::institutions::subjects::photos::SCHEMA)
            .map_err(|err| format!("init legal representative photos failed: {err}"))?;

        let comment: String = tx
            .query_one(
                "SELECT format('COMMENT ON SCHEMA %I IS %L',current_schema(),$1::text)",
                &[&SCHEMA_FORMAT],
            )
            .map_err(|e| e.to_string())?
            .get(0);
        tx.batch_execute(&comment).map_err(|e| e.to_string())?;
        if !Self::schema_ready(&mut tx)? {
            return Err("onchina schema initialization incomplete".into());
        }
        tx.commit()
            .map_err(|e| format!("commit schema initialization failed: {e}"))
    }

    fn init_subject_partition_schema(
        conn: &mut impl postgres::GenericClient,
    ) -> Result<(), String> {
        conn.batch_execute(
            "CREATE TABLE IF NOT EXISTS identity (
                cid_number TEXT PRIMARY KEY,
                record_kind TEXT NOT NULL CHECK (record_kind IN ('CITIZEN', 'PUBLIC', 'PRIVATE')),
                province_code TEXT NOT NULL,
                city_code TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );

             CREATE TABLE IF NOT EXISTS subject (
                cid_number TEXT NOT NULL,
                record_kind TEXT NOT NULL CHECK (record_kind IN ('CITIZEN', 'PUBLIC', 'PRIVATE')),
                full_name TEXT,
                short_name TEXT,
                state_code TEXT CHECK (state_code IN ('ACTIVE', 'REVOKED')),
                chain_status TEXT CHECK (chain_status IN ('ACTIVE_ON_CHAIN', 'REVOKED_ON_CHAIN')),
                subject_kind TEXT,
                profit_kind TEXT,
                province_code TEXT NOT NULL,
                city_code TEXT,
                town_code TEXT,
                institution_code TEXT,
                education_type TEXT,
                private_type TEXT,
                partnership_kind TEXT,
                legal_personality BOOLEAN,
                parent_cid TEXT,
                family_name TEXT,
                given_name TEXT,
                representative_cid TEXT,

                representative_account TEXT
                    CHECK (representative_account IS NULL OR
                           representative_account ~ '^0x[0-9a-f]{64}$'),
                issuer_cid TEXT,
                source_type TEXT,
                proposal_id TEXT,
                creator_account TEXT
                    CHECK (creator_account IS NULL OR creator_account ~ '^0x[0-9a-f]{64}$'),
                updater_account TEXT
                    CHECK (updater_account IS NULL OR updater_account ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS member (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                family_name TEXT NOT NULL DEFAULT '管理',
                given_name TEXT NOT NULL DEFAULT '员',
                admin_department TEXT,
                admin_job TEXT,
                contact_phone TEXT,
                contact_email TEXT,
                photo_path TEXT,
                photo_name TEXT,
                photo_mime TEXT,
                photo_size BIGINT,
                credential_id TEXT,
                source_id TEXT,
                admin_status TEXT,
                updated_at TIMESTAMPTZ,
                creator_account TEXT
                    CHECK (creator_account IS NULL OR creator_account ~ '^0x[0-9a-f]{64}$'),
                operation_id TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, cid_number, account_id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS citizen (
                cid_number TEXT NOT NULL,
                passport_no TEXT NOT NULL DEFAULT '',
                family_name TEXT NOT NULL DEFAULT '',
                given_name TEXT NOT NULL DEFAULT '',
                citizen_sex TEXT NOT NULL DEFAULT '',
                birth_date TEXT NOT NULL DEFAULT '',
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                row_id BIGINT,
                account_id TEXT
                    CHECK (account_id IS NULL OR account_id ~ '^0x[0-9a-f]{64}$'),
                binding_revision BIGINT NOT NULL DEFAULT 0 CHECK (binding_revision >= 0),
                binding_block BIGINT,
                binding_hash TEXT CHECK (
                    binding_hash IS NULL OR
                    binding_hash ~ '^0x[0-9a-f]{64}$'
                ),
                citizen_status TEXT NOT NULL,
                voting_eligible BOOLEAN NOT NULL,
                valid_from TEXT NOT NULL DEFAULT '',
                valid_until TEXT NOT NULL DEFAULT '',
                status_at BIGINT,
                town_code TEXT NOT NULL DEFAULT '',
                birth_province TEXT NOT NULL DEFAULT '',
                birth_city TEXT NOT NULL DEFAULT '',
                birth_town TEXT NOT NULL DEFAULT '',
                archive_hash TEXT,
                tx_hash TEXT,
                block_number BIGINT,
                onchain_at TIMESTAMPTZ,
                creator_account TEXT NOT NULL
                    CHECK (creator_account ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updater_account TEXT
                    CHECK (updater_account IS NULL OR updater_account ~ '^0x[0-9a-f]{64}$'),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS document (
                row_id BIGSERIAL,
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                file_name TEXT NOT NULL,
                document_type TEXT NOT NULL,
                file_size BIGINT NOT NULL DEFAULT 0,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                uploader_account TEXT NOT NULL
                    CHECK (uploader_account ~ '^0x[0-9a-f]{64}$'),
                uploaded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, row_id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS government (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                town_code TEXT,
                institution_code TEXT NOT NULL,
                source_kind TEXT NOT NULL DEFAULT 'CHAIN' CHECK (source_kind IN ('CHAIN', 'MANUAL')),
                home_province TEXT,
                home_city TEXT,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS private (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                kind_code TEXT NOT NULL,
                private_type TEXT NOT NULL CHECK (
                    private_type IN ('SOLE', 'PARTNERSHIP', 'COMPANY', 'CORPORATION',
                                     'WELFARE', 'ASSOCIATION')
                ),
                partnership_kind TEXT CHECK (partnership_kind IN ('GENERAL', 'LIMITED')),
                legal_personality BOOLEAN NOT NULL,
                profit_kind TEXT NOT NULL CHECK (profit_kind IN ('0', '1')),
                parent_cid TEXT,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS account (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                account_name TEXT NOT NULL,
                account_id TEXT
                    CHECK (account_id IS NULL OR account_id ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, cid_number, account_name)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS attachment (
                row_id BIGSERIAL,
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                file_name TEXT NOT NULL,
                doc_type TEXT NOT NULL,
                file_size BIGINT NOT NULL DEFAULT 0,
                file_path TEXT NOT NULL,
                uploader_account TEXT NOT NULL
                    CHECK (uploader_account ~ '^0x[0-9a-f]{64}$'),
                uploaded_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, row_id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS audit (
                row_id BIGSERIAL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                actor_account TEXT
                    CHECK (actor_account IS NULL OR actor_account ~ '^0x[0-9a-f]{64}$'),
                action_kind TEXT NOT NULL,
                target_cid TEXT,
                detail_json JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, row_id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS sequence (
                seq_key TEXT PRIMARY KEY,
                next_seq BIGINT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS passport (
                passport_no TEXT PRIMARY KEY,
                cid_number TEXT NOT NULL UNIQUE,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE TABLE IF NOT EXISTS recycle (
                pool_id TEXT PRIMARY KEY,
                passport_no TEXT NOT NULL,
                source_cid TEXT NOT NULL DEFAULT '',
                deleted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                released_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                used_at TIMESTAMPTZ,
                used_cid TEXT
             );

             CREATE INDEX IF NOT EXISTS idx_passport_number_recycle_available
                ON recycle(released_at, pool_id) WHERE used_at IS NULL;
             CREATE INDEX IF NOT EXISTS idx_subjects_city
                ON subject(province_code, city_code, record_kind, cid_number);
             CREATE INDEX IF NOT EXISTS idx_subjects_town
                ON subject(province_code, city_code, town_code, record_kind, cid_number);
             CREATE INDEX IF NOT EXISTS idx_subjects_scope_created
                ON subject(subject_kind, province_code, city_code, created_at DESC, cid_number DESC);
             CREATE INDEX IF NOT EXISTS idx_subjects_exact_lookup
                ON subject(subject_kind, province_code, city_code, cid_number, full_name, short_name);
             CREATE INDEX IF NOT EXISTS idx_subjects_legal_rep
                ON subject(province_code, representative_cid);
             CREATE INDEX IF NOT EXISTS idx_subjects_education
                ON subject(province_code, city_code, institution_code, education_type);
             CREATE INDEX IF NOT EXISTS idx_citizens_scope_created
                ON citizen(province_code, city_code, created_at DESC, row_id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_province_created
                ON citizen(province_code, created_at DESC, row_id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_exact_lookup
                ON citizen(province_code, city_code, cid_number, passport_no, account_id);
             CREATE INDEX IF NOT EXISTS idx_citizens_passport_no ON citizen(passport_no);
             CREATE INDEX IF NOT EXISTS idx_citizens_account_id
                ON citizen(account_id) WHERE account_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_citizens_town_scope
                ON citizen(province_code, city_code, town_code, created_at DESC, row_id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_birth_scope
                ON citizen(birth_province, birth_city, birth_town, created_at DESC, row_id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizen_documents_cid
                ON document(province_code, cid_number, uploaded_at DESC, row_id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizen_documents_type
                ON document(province_code, cid_number, document_type);
             CREATE INDEX IF NOT EXISTS idx_gov_city
                ON government(province_code, city_code, institution_code);
             CREATE INDEX IF NOT EXISTS idx_private_city
                ON private(province_code, city_code, private_type, kind_code);
             CREATE INDEX IF NOT EXISTS idx_accounts_cid ON account(province_code, cid_number);
             CREATE INDEX IF NOT EXISTS idx_docs_cid
                ON attachment(province_code, cid_number, uploaded_at DESC);
             CREATE INDEX IF NOT EXISTS idx_audit_scope_time
                ON audit(province_code, city_code, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_institution_admins_cid
                ON member(province_code, cid_number);
             CREATE INDEX IF NOT EXISTS idx_institution_admins_account
                ON member(province_code, account_id);",
        )
        .map_err(|err| {
            format!(
                "init subject partition schema failed: {}",
                postgres_error_text(&err)
            )
        })?;

        for province in crate::cid::china::provinces() {
            Self::create_subject_partitions(conn, province.province_code)?;
        }
        Ok(())
    }

    fn create_subject_partitions(
        conn: &mut impl postgres::GenericClient,
        province_code: &str,
    ) -> Result<(), String> {
        for table in crate::institutions::subjects::schema::PARTITIONED_TABLES {
            let partition_name = format!("{}{}", table, province_code.to_ascii_lowercase());
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {partition_name} PARTITION OF {table} \
                 FOR VALUES IN ('{province_code}')"
            );
            conn.batch_execute(sql.as_str()).map_err(|err| {
                format!(
                    "init partition {partition_name} failed: {}",
                    postgres_error_text(&err)
                )
            })?;
        }
        Ok(())
    }
}

/// 只供显式启用的集成回归：独立 schema 和连接池，不接受生产库名或远端地址。
#[cfg(test)]
pub(crate) struct IsolatedTestDb {
    pub(crate) db: Db,
    config: postgres::Config,
    schema: String,
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
impl IsolatedTestDb {
    pub(crate) fn new() -> Self {
        // 同步 postgres 自建 Tokio runtime，隔离初始化不得嵌套调用者的异步 runtime。
        std::thread::spawn(Self::create)
            .join()
            .expect("隔离数据库初始化线程失败")
    }

    fn create() -> Self {
        let url =
            std::env::var("ONCHINA_TEST_DATABASE_URL").expect("控制台必须显式提供隔离测试数据库");
        let config: postgres::Config = url.parse().expect("测试数据库配置无效");
        assert!(
            config
                .get_dbname()
                .is_some_and(|name| name.starts_with("onchina_test")),
            "仅允许 onchina_test 前缀数据库"
        );
        assert!(
            config.get_hosts().iter().all(|host| match host {
                postgres::config::Host::Tcp(host) =>
                    matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
                #[cfg(unix)]
                postgres::config::Host::Unix(_) => true,
            }),
            "测试数据库必须位于本机"
        );
        let schema = format!("onchina_{}", uuid::Uuid::new_v4().as_simple());
        let mut admin = config
            .connect(postgres::NoTls)
            .expect("无法连接隔离测试数据库");
        admin
            .batch_execute(&format!("CREATE SCHEMA {schema}"))
            .expect("无法创建测试 schema");
        let mut scoped = config.clone();
        scoped.options(&format!("-c search_path={schema}"));
        let mut first = scoped
            .connect(postgres::NoTls)
            .expect("无法连接测试 schema");
        Db::init_current_schema(&mut first).expect("测试 schema 初始化失败");
        let mut clients = vec![Mutex::new(first)];
        for _ in 1..4 {
            clients.push(Mutex::new(
                scoped.connect(postgres::NoTls).expect("测试连接池创建失败"),
            ));
        }
        Self {
            db: Db {
                clients: Arc::new(clients),
                next_client_idx: Arc::new(AtomicUsize::new(0)),
            },
            config,
            schema,
        }
    }
}

#[cfg(test)]
impl Drop for IsolatedTestDb {
    fn drop(&mut self) {
        let config = self.config.clone();
        let schema = self.schema.clone();
        let db = std::mem::replace(&mut self.db, Db::empty_for_route_test());
        // 清理同样放在独立线程，仅删除本测试拥有的 UUID schema。
        let result = std::thread::spawn(move || {
            // postgres::Client 析构也会 block_on，所有本测试连接必须在线程内关闭。
            drop(db);
            let mut connection = config.connect(postgres::NoTls).expect("测试清理连接失败");
            connection
                .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
                .expect("测试 schema 清理失败");
        })
        .join();
        assert!(result.is_ok(), "隔离数据库清理线程失败");
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    #[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
    fn current_schema_is_exact_and_reopens_without_schema_writes() {
        let sandbox = IsolatedTestDb::new();
        sandbox.db.with_client(|conn| {
            assert!(Db::schema_ready(conn)?);
            // 只读事务内再次初始化必须只核验，不能执行CREATE或COMMENT。
            conn.batch_execute("BEGIN READ ONLY").map_err(|e| e.to_string())?;
            Db::init_current_schema(conn)?;
            conn.batch_execute("ROLLBACK").map_err(|e| e.to_string())?;
            for (table,expected) in [("challenge",1_i64),("grant",1),("login",1),("enrollment",1),("session",2),("result",2)] {
                let count:i64=conn.query_one("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name=$1", &[&table]).map_err(|e| e.to_string())?.get(0);
                assert_eq!(count,expected,"{table}");
            }
            Ok(())
        }).unwrap();
    }

    #[test]
    #[ignore = "由控制台交付本机onchina_test隔离数据库后执行"]
    fn legacy_and_damaged_schema_are_rejected_without_repair_or_data_loss() {
        let sandbox = IsolatedTestDb::new();
        sandbox.db.with_client(|conn| {
            conn.batch_execute("CREATE TABLE admin_sessions(legacy_value TEXT); INSERT INTO admin_sessions VALUES('synthetic retained record')").map_err(|e| e.to_string())?;
            assert!(Db::init_current_schema(conn).is_err());
            let value:String=conn.query_one("SELECT legacy_value FROM admin_sessions", &[]).map_err(|e| e.to_string())?.get(0);
            assert_eq!(value,"synthetic retained record");
            conn.batch_execute("DROP TABLE admin_sessions; ALTER TABLE subject RENAME COLUMN full_name TO legacy").map_err(|e| e.to_string())?;
            assert!(Db::init_current_schema(conn).is_err());
            let count:i64=conn.query_one("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='subject' AND column_name='legacy'", &[]).map_err(|e| e.to_string())?.get(0);
            assert_eq!(count,1);
            Ok(())
        }).unwrap();
    }
}
