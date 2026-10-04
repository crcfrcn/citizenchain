//! OnChina 结构化数据库入口。
//!
//! 本模块只负责 PostgreSQL 连接池、创世前最终 schema 初始化和短事务封装。
//! schema 只描述目标结构，启动时幂等创建表、约束、索引和行政区分区。

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

#[derive(Clone)]
pub(crate) struct Db {
    clients: Arc<Vec<Mutex<postgres::Client>>>,
    next_client_idx: Arc<AtomicUsize>,
}

impl Db {
    pub(crate) fn from_database_url(database_url: &str) -> Result<Self, String> {
        let db_url = database_url.to_string();
        let pool_size = std::env::var("ONCHINA_PG_POOL_SIZE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(4);
        let handle = thread::spawn(move || {
            let mut bootstrap = postgres::Client::connect(db_url.as_str(), postgres::NoTls)
                .map_err(|err| format!("connect postgres failed: {}", postgres_error_text(&err)))?;
            Self::init_current_schema(&mut bootstrap)?;
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

    fn init_current_schema(conn: &mut postgres::Client) -> Result<(), String> {
        conn.batch_execute(
            "CREATE TABLE IF NOT EXISTS citizen_onchain_operations (
                operation_id TEXT PRIMARY KEY,
                registrar_account_id TEXT NOT NULL
                    CHECK (registrar_account_id ~ '^0x[0-9a-f]{64}$'),
                institution_code TEXT NOT NULL,
                actor_role_code TEXT NOT NULL,
                cid_number TEXT NOT NULL,
                citizen_account_id TEXT NOT NULL
                    CHECK (citizen_account_id ~ '^0x[0-9a-f]{64}$'),
                identity_level TEXT NOT NULL,
                payload_hex TEXT NOT NULL,
                expected_identity_version BIGINT NOT NULL,
                authorization_expires_at BIGINT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                citizen_signed_at TIMESTAMPTZ
             );
             CREATE INDEX IF NOT EXISTS idx_citizen_onchain_operations_expiry
                ON citizen_onchain_operations(expires_at);

             CREATE TABLE IF NOT EXISTS chain_sign_sessions (
                request_id TEXT PRIMARY KEY,
                purpose TEXT NOT NULL,
                account_id TEXT NOT NULL
                    CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                call_data TEXT NOT NULL,
                nonce BIGINT NOT NULL,
                signing_hash TEXT NOT NULL,
                context JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ
             );
             CREATE INDEX IF NOT EXISTS idx_chain_sign_sessions_expiry
                ON chain_sign_sessions(expires_at) WHERE consumed_at IS NULL;

             CREATE TABLE IF NOT EXISTS admins (
                admin_id BIGINT PRIMARY KEY,
                account_id TEXT NOT NULL UNIQUE CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                family_name TEXT NOT NULL,
                given_name TEXT NOT NULL,
                institution_code TEXT NOT NULL,
                built_in BOOLEAN NOT NULL DEFAULT FALSE,
                creator_account_id TEXT NOT NULL
                    CHECK (creator_account_id ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ,
                city_name TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS idx_admins_institution_code
                ON admins(institution_code);
             CREATE INDEX IF NOT EXISTS idx_admins_institution_code_city_name
                ON admins(institution_code, city_name);
             CREATE INDEX IF NOT EXISTS idx_admins_account_id ON admins(account_id);
             CREATE INDEX IF NOT EXISTS idx_admins_creator_account_id
                ON admins(creator_account_id);

             CREATE TABLE IF NOT EXISTS admin_action_challenges (
                action_id TEXT PRIMARY KEY,
                actor_account_id TEXT NOT NULL
                    CHECK (actor_account_id ~ '^0x[0-9a-f]{64}$'),
                action_type TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                consumed BOOLEAN NOT NULL DEFAULT FALSE,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_action_challenges_expires
                ON admin_action_challenges(expires_at);

             CREATE TABLE IF NOT EXISTS admin_security_grants (
                grant_id TEXT PRIMARY KEY,
                actor_account_id TEXT NOT NULL
                    CHECK (actor_account_id ~ '^0x[0-9a-f]{64}$'),
                action_type TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                consumed BOOLEAN NOT NULL DEFAULT FALSE,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_security_grants_expires
                ON admin_security_grants(expires_at);

             CREATE TABLE IF NOT EXISTS admin_login_sign_requests (
                challenge_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                account_id TEXT NOT NULL
                    CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed BOOLEAN NOT NULL DEFAULT FALSE,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_login_sign_requests_expires
                ON admin_login_sign_requests(expires_at);

             CREATE TABLE IF NOT EXISTS node_institution_bindings (
                binding_id TEXT PRIMARY KEY,
                candidate_id TEXT NOT NULL,
                institution_code TEXT NOT NULL,
                institution_cid_number TEXT NOT NULL,
                frg_province_code TEXT,
                bound_account_id TEXT NOT NULL
                    CHECK (bound_account_id ~ '^0x[0-9a-f]{64}$'),
                bound_at TIMESTAMPTZ NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'INACTIVE'))
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_node_binding_one_active
                ON node_institution_bindings ((status)) WHERE status = 'ACTIVE';

             CREATE TABLE IF NOT EXISTS node_binding_challenges (
                binding_challenge_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed BOOLEAN NOT NULL DEFAULT FALSE,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_node_binding_challenges_expires
                ON node_binding_challenges(expires_at);

             CREATE TABLE IF NOT EXISTS admin_qr_login_results (
                challenge_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                payload JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_qr_login_results_session
                ON admin_qr_login_results(session_id, expires_at);

             CREATE TABLE IF NOT EXISTS admin_sessions (
                token TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                institution_code TEXT NOT NULL,
                candidate_id TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                last_active_at TIMESTAMPTZ NOT NULL,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_sessions_account
                ON admin_sessions(account_id);
             CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires
                ON admin_sessions(expires_at);

             CREATE TABLE IF NOT EXISTS admin_passkey_credentials (
                credential_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                passkey JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_credentials_account
                ON admin_passkey_credentials(account_id);

             CREATE TABLE IF NOT EXISTS admin_passkey_ceremonies (
                ceremony_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                kind TEXT NOT NULL CHECK (kind IN ('REG', 'AUTH')),
                state JSONB NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_ceremonies_expires
                ON admin_passkey_ceremonies(expires_at);

             CREATE TABLE IF NOT EXISTS admin_passkey_assertions (
                assertion_id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_admin_passkey_assertions_expires
                ON admin_passkey_assertions(expires_at);

             CREATE TABLE IF NOT EXISTS qr_consumed (
                qr_id TEXT PRIMARY KEY,
                consumed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_qr_consumed_expires ON qr_consumed(expires_at);

             CREATE TABLE IF NOT EXISTS chain_requests (
                route_key TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                nonce TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                received_at TIMESTAMPTZ NOT NULL,
                payload JSONB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_chain_requests_received
                ON chain_requests(received_at);

             CREATE TABLE IF NOT EXISTS chain_nonces (
                nonce TEXT PRIMARY KEY,
                seen_at TIMESTAMPTZ NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_chain_nonces_seen ON chain_nonces(seen_at);

             CREATE TABLE IF NOT EXISTS tx_records (
                id BIGSERIAL PRIMARY KEY,
                block_number BIGINT NOT NULL,
                extrinsic_index SMALLINT,
                event_index SMALLINT NOT NULL,
                tx_type TEXT NOT NULL,
                sender_account_id TEXT
                    CHECK (sender_account_id IS NULL OR sender_account_id ~ '^0x[0-9a-f]{64}$'),
                recipient_account_id TEXT
                    CHECK (recipient_account_id IS NULL OR recipient_account_id ~ '^0x[0-9a-f]{64}$'),
                amount_fen BIGINT NOT NULL,
                fee_fen BIGINT,
                block_timestamp TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE INDEX IF NOT EXISTS idx_tx_records_sender_account_id
                ON tx_records(sender_account_id, block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_recipient_account_id
                ON tx_records(recipient_account_id, block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_block
                ON tx_records(block_number DESC);
             CREATE INDEX IF NOT EXISTS idx_tx_records_type ON tx_records(tx_type);

             CREATE TABLE IF NOT EXISTS tx_indexer_state (
                id INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
                last_indexed_block BIGINT NOT NULL DEFAULT 0,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             INSERT INTO tx_indexer_state(id, last_indexed_block)
             VALUES (1, 0) ON CONFLICT (id) DO NOTHING;

             CREATE TABLE IF NOT EXISTS chain_projection_state (
                projection_key TEXT PRIMARY KEY,
                chain_genesis_hash TEXT NOT NULL,
                chain_block_hash TEXT NOT NULL DEFAULT '',
                chain_block_number BIGINT,
                item_count BIGINT NOT NULL DEFAULT 0,
                account_count BIGINT NOT NULL DEFAULT 0,
                status TEXT NOT NULL CHECK (status IN ('OK', 'FAILED')),
                synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );",
        )
        .map_err(|err| format!("init core schema failed: {}", postgres_error_text(&err)))?;
        Self::init_subject_partition_schema(conn)?;
        conn.batch_execute(crate::institution::subjects::photos::SCHEMA)
            .map_err(|err| format!("init legal representative photos failed: {err}"))
    }

    fn init_subject_partition_schema(conn: &mut postgres::Client) -> Result<(), String> {
        conn.batch_execute(
            "CREATE TABLE IF NOT EXISTS ids (
                cid_number TEXT PRIMARY KEY,
                kind TEXT NOT NULL CHECK (kind IN ('CITIZEN', 'PUBLIC', 'PRIVATE')),
                province_code TEXT NOT NULL,
                city_code TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );

             CREATE TABLE IF NOT EXISTS subjects (
                cid_number TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind IN ('CITIZEN', 'PUBLIC', 'PRIVATE')),
                cid_full_name TEXT,
                cid_short_name TEXT,
                status TEXT CHECK (status IN ('ACTIVE', 'REVOKED')),
                category TEXT,
                p1 TEXT,
                province_code TEXT NOT NULL,
                city_code TEXT,
                town_code TEXT,
                institution_code TEXT,
                education_type TEXT,
                private_type TEXT,
                partnership_kind TEXT,
                has_legal_personality BOOLEAN,
                parent_cid_number TEXT,
                family_name TEXT,
                given_name TEXT,
                legal_representative_cid_number TEXT,
                legal_representative_photo_path TEXT,
                legal_representative_photo_name TEXT,
                legal_representative_photo_mime TEXT,
                legal_representative_photo_size BIGINT,
                legal_representative_account_id TEXT
                    CHECK (legal_representative_account_id IS NULL OR
                           legal_representative_account_id ~ '^0x[0-9a-f]{64}$'),
                issuer_cid_number TEXT,
                institution_source_type TEXT,
                register_proposal_id TEXT,
                creator_account_id TEXT
                    CHECK (creator_account_id IS NULL OR creator_account_id ~ '^0x[0-9a-f]{64}$'),
                updater_account_id TEXT
                    CHECK (updater_account_id IS NULL OR updater_account_id ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS institution_admins (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                account_id TEXT NOT NULL CHECK (account_id ~ '^0x[0-9a-f]{64}$'),
                family_name TEXT NOT NULL DEFAULT '管理',
                given_name TEXT NOT NULL DEFAULT '员',
                admin_department TEXT,
                admin_job TEXT,
                admin_contact_phone TEXT,
                admin_contact_email TEXT,
                admin_photo_path TEXT,
                admin_photo_name TEXT,
                admin_photo_mime TEXT,
                admin_photo_size BIGINT,
                admin_passkey_credential_id TEXT,
                admin_source_id TEXT,
                admin_status TEXT,
                admin_updated_at TIMESTAMPTZ,
                creator_account_id TEXT
                    CHECK (creator_account_id IS NULL OR creator_account_id ~ '^0x[0-9a-f]{64}$'),
                operation_log_id TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, cid_number, account_id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS citizens (
                cid_number TEXT NOT NULL,
                passport_no TEXT NOT NULL DEFAULT '',
                family_name TEXT NOT NULL DEFAULT '',
                given_name TEXT NOT NULL DEFAULT '',
                citizen_sex TEXT NOT NULL DEFAULT '',
                citizen_birth_date TEXT NOT NULL DEFAULT '',
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                id BIGINT,
                account_id TEXT
                    CHECK (account_id IS NULL OR account_id ~ '^0x[0-9a-f]{64}$'),
                binding_revision BIGINT NOT NULL DEFAULT 0 CHECK (binding_revision >= 0),
                binding_finalized_block_number BIGINT,
                binding_finalized_block_hash TEXT CHECK (
                    binding_finalized_block_hash IS NULL OR
                    binding_finalized_block_hash ~ '^0x[0-9a-f]{64}$'
                ),
                citizen_status TEXT NOT NULL,
                voting_eligible BOOLEAN NOT NULL,
                passport_valid_from TEXT NOT NULL DEFAULT '',
                passport_valid_until TEXT NOT NULL DEFAULT '',
                status_updated_at BIGINT,
                town_code TEXT NOT NULL DEFAULT '',
                birth_province_code TEXT NOT NULL DEFAULT '',
                birth_city_code TEXT NOT NULL DEFAULT '',
                birth_town_code TEXT NOT NULL DEFAULT '',
                archive_hash TEXT,
                onchain_tx_hash TEXT,
                onchain_block_number BIGINT,
                onchain_at TIMESTAMPTZ,
                creator_account_id TEXT NOT NULL
                    CHECK (creator_account_id ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updater_account_id TEXT
                    CHECK (updater_account_id IS NULL OR updater_account_id ~ '^0x[0-9a-f]{64}$'),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS citizen_documents (
                id BIGSERIAL,
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                file_name TEXT NOT NULL,
                document_type TEXT NOT NULL,
                file_size BIGINT NOT NULL DEFAULT 0,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                uploader_account_id TEXT NOT NULL
                    CHECK (uploader_account_id ~ '^0x[0-9a-f]{64}$'),
                uploaded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS gov (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                town_code TEXT,
                institution_code TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'CHAIN' CHECK (source IN ('CHAIN', 'MANUAL')),
                home_p TEXT,
                home_c TEXT,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS private (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                code TEXT NOT NULL,
                private_type TEXT NOT NULL CHECK (
                    private_type IN ('SOLE', 'PARTNERSHIP', 'COMPANY', 'CORPORATION',
                                     'WELFARE', 'ASSOCIATION')
                ),
                partnership_kind TEXT CHECK (partnership_kind IN ('GENERAL', 'LIMITED')),
                has_legal_personality BOOLEAN NOT NULL,
                p1 TEXT NOT NULL CHECK (p1 IN ('0', '1')),
                parent_cid_number TEXT,
                PRIMARY KEY (province_code, cid_number)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS accounts (
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                account_name TEXT NOT NULL,
                account_id TEXT
                    CHECK (account_id IS NULL OR account_id ~ '^0x[0-9a-f]{64}$'),
                created_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, cid_number, account_name)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS docs (
                id BIGSERIAL,
                cid_number TEXT NOT NULL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                file_name TEXT NOT NULL,
                doc_type TEXT NOT NULL,
                file_size BIGINT NOT NULL DEFAULT 0,
                file_path TEXT NOT NULL,
                uploader_account_id TEXT NOT NULL
                    CHECK (uploader_account_id ~ '^0x[0-9a-f]{64}$'),
                uploaded_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (province_code, id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS audit (
                id BIGSERIAL,
                province_code TEXT NOT NULL,
                city_code TEXT,
                actor_account_id TEXT
                    CHECK (actor_account_id IS NULL OR actor_account_id ~ '^0x[0-9a-f]{64}$'),
                action TEXT NOT NULL,
                target_cid TEXT,
                detail JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (province_code, id)
             ) PARTITION BY LIST (province_code);

             CREATE TABLE IF NOT EXISTS sequence_counters (
                seq_key TEXT PRIMARY KEY,
                next_seq BIGINT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS passport_numbers (
                passport_no TEXT PRIMARY KEY,
                cid_number TEXT NOT NULL UNIQUE,
                province_code TEXT NOT NULL,
                city_code TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
             );
             CREATE TABLE IF NOT EXISTS passport_number_recycle_pool (
                pool_id TEXT PRIMARY KEY,
                passport_no TEXT NOT NULL,
                source_cid_number TEXT NOT NULL DEFAULT '',
                deleted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                released_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                used_at TIMESTAMPTZ,
                used_by_cid_number TEXT
             );

             CREATE INDEX IF NOT EXISTS idx_passport_number_recycle_available
                ON passport_number_recycle_pool(released_at, pool_id) WHERE used_at IS NULL;
             CREATE INDEX IF NOT EXISTS idx_subjects_city
                ON subjects(province_code, city_code, kind, cid_number);
             CREATE INDEX IF NOT EXISTS idx_subjects_town
                ON subjects(province_code, city_code, town_code, kind, cid_number);
             CREATE INDEX IF NOT EXISTS idx_subjects_scope_created
                ON subjects(category, province_code, city_code, created_at DESC, cid_number DESC);
             CREATE INDEX IF NOT EXISTS idx_subjects_exact_lookup
                ON subjects(category, province_code, city_code, cid_number, cid_full_name, cid_short_name);
             CREATE INDEX IF NOT EXISTS idx_subjects_legal_rep
                ON subjects(province_code, legal_representative_cid_number);
             CREATE INDEX IF NOT EXISTS idx_subjects_education
                ON subjects(province_code, city_code, institution_code, education_type);
             CREATE INDEX IF NOT EXISTS idx_citizens_scope_created
                ON citizens(province_code, city_code, created_at DESC, id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_province_created
                ON citizens(province_code, created_at DESC, id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_exact_lookup
                ON citizens(province_code, city_code, cid_number, passport_no, account_id);
             CREATE INDEX IF NOT EXISTS idx_citizens_passport_no ON citizens(passport_no);
             CREATE INDEX IF NOT EXISTS idx_citizens_account_id
                ON citizens(account_id) WHERE account_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_citizens_town_scope
                ON citizens(province_code, city_code, town_code, created_at DESC, id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizens_birth_scope
                ON citizens(birth_province_code, birth_city_code, birth_town_code, created_at DESC, id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizen_documents_cid
                ON citizen_documents(province_code, cid_number, uploaded_at DESC, id DESC);
             CREATE INDEX IF NOT EXISTS idx_citizen_documents_type
                ON citizen_documents(province_code, cid_number, document_type);
             CREATE INDEX IF NOT EXISTS idx_gov_city
                ON gov(province_code, city_code, institution_code);
             CREATE INDEX IF NOT EXISTS idx_private_city
                ON private(province_code, city_code, private_type, code);
             CREATE INDEX IF NOT EXISTS idx_accounts_cid ON accounts(province_code, cid_number);
             CREATE INDEX IF NOT EXISTS idx_docs_cid
                ON docs(province_code, cid_number, uploaded_at DESC);
             CREATE INDEX IF NOT EXISTS idx_audit_scope_time
                ON audit(province_code, city_code, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_institution_admins_cid
                ON institution_admins(province_code, cid_number);
             CREATE INDEX IF NOT EXISTS idx_institution_admins_account
                ON institution_admins(province_code, account_id);",
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
        conn: &mut postgres::Client,
        province_code: &str,
    ) -> Result<(), String> {
        for table in crate::institution::subjects::schema::PARTITIONED_TABLES {
            let partition_name = format!("{}_{}", table, province_code.to_ascii_lowercase());
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
