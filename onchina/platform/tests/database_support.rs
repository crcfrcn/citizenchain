// 隔离数据库辅助，仅供显式测试模式。
/// 只供显式启用的集成回归：独立 schema 和连接池，不接受生产库名或远端地址。
pub struct IsolatedTestDb {
    pub db: Db,
    config: postgres::Config,
    schema: String,
}

// PostgreSQL优先使用hostaddr连接，必须同时检查主机声明和实际地址，且不能省略目标。
fn validate_test_database(config: &postgres::Config) -> Result<(), &'static str> {
    if !config
        .get_dbname()
        .is_some_and(|name| name.starts_with("onchina_test"))
    {
        return Err("仅允许 onchina_test 前缀数据库");
    }
    if (config.get_hosts().is_empty() && config.get_hostaddrs().is_empty())
        || !config.get_hosts().iter().all(|host| match host {
            postgres::config::Host::Tcp(host) => {
                matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1")
            }
            #[cfg(unix)]
            postgres::config::Host::Unix(_) => true,
        })
        || !config
            .get_hostaddrs()
            .iter()
            .all(|address| address.is_loopback())
    {
        return Err("测试数据库必须显式指向本机");
    }
    Ok(())
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
impl IsolatedTestDb {
    pub fn new() -> Self {
        // 同步 postgres 自建 Tokio runtime，隔离初始化不得嵌套调用者的异步 runtime。
        std::thread::spawn(Self::create)
            .join()
            .expect("隔离数据库初始化线程失败")
    }

    fn create() -> Self {
        let url =
            std::env::var("ONCHINA_TEST_DATABASE_URL").expect("控制台必须显式提供隔离测试数据库");
        let config: postgres::Config = url.parse().expect("测试数据库配置无效");
        validate_test_database(&config).expect("隔离测试数据库目标无效");
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

// 空路由状态工厂正文也只归测试目录。
impl Db {
    /// 路由单测不创建数据库；任何误入持久化的请求都会明确失败。
    pub fn empty_for_route_test() -> Self {
        Self {
            clients: Arc::new(Vec::new()),
            next_client_idx: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[cfg(test)]
mod database_target_tests {
    use super::validate_test_database;

    // 只解析合成配置，不连接网络或数据库。
    #[test]
    fn accepts_explicit_loopback_targets() {
        for target in [
            "host=localhost",
            "host=127.0.0.1",
            "host=::1",
            "host=localhost hostaddr=127.0.0.1",
            "hostaddr=::1",
            "hostaddr=127.0.0.1,::1",
        ] {
            let config = format!("{target} dbname=onchina_test_config")
                .parse()
                .expect("合成配置有效");
            assert_eq!(validate_test_database(&config), Ok(()), "{target}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn accepts_explicit_unix_socket() {
        let config = "host=/tmp/onchina_test_socket dbname=onchina_test_config"
            .parse()
            .expect("合成配置有效");
        assert_eq!(validate_test_database(&config), Ok(()));
    }

    #[test]
    fn rejects_remote_or_missing_targets() {
        for target in [
            "",
            "host=database.invalid",
            "hostaddr=203.0.113.10",
            "host=localhost hostaddr=203.0.113.10",
            "host=127.0.0.1,203.0.113.10",
            "hostaddr=127.0.0.1,203.0.113.10",
        ] {
            let config = format!("{target} dbname=onchina_test_config")
                .parse()
                .expect("合成配置有效");
            assert!(validate_test_database(&config).is_err(), "{target}");
        }
    }

    #[test]
    fn rejects_production_or_missing_database() {
        for target in ["host=localhost", "host=localhost dbname=onchina"] {
            let config = target.parse().expect("合成配置有效");
            assert!(validate_test_database(&config).is_err(), "{target}");
        }
    }
}
