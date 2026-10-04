/// 公民 CID finalized 六读闭环快照；注册局办理、查询和投影共用。
pub(crate) mod chain_citizen_identity;
/// 跨业务复用的链上凭证签名、SCALE payload 与 genesis hash 对齐工具。
pub(crate) mod chain_runtime;
pub(crate) mod chain_submit;
/// 链 RPC URL 统一读取入口,业务模块不得直接读环境变量。
pub(crate) mod chain_url;
/// PostgreSQL 连接池和当前结构化 schema 初始化。
pub(crate) mod db;
/// 内嵌私有 PostgreSQL 生命周期(onchina 自管;Card 05 零依赖部署)。
pub(crate) mod embedded_pg;
pub(crate) mod http_security;
/// 机构治理裸 SCALE call data 编码器（OnChina 唯一真源）。
pub(crate) mod institution_call;
/// QR_V1 协议和链上中国平台签名二维码构造。
pub(crate) mod qr;
/// HTTP API 通用响应、分页和健康检查输出模型。
pub(crate) mod response;
pub(crate) mod runtime_ops;
/// onchina 内网 API 机构私有 CA TLS(Card 05;rcgen CA 签发 + rustls)。
pub(crate) mod tls;
