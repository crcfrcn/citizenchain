//! 核心常量。

// 货币基础参数。
pub const TOKEN_SYMBOL: &str = "GMB"; // 公民币符号
pub const TOKEN_DECIMALS: u32 = 2; // 精度：2 位（元 / 分制），1 GMB = 100 FEN
pub const TOKEN_MIN_UNIT: u128 = 1; // 最小计价单位（1 分）
pub const SS58_FORMAT: u16 = 2027; // 地址格式前缀（SS58）
pub const CHAIN_NAME: &str = "CitizenChain"; // 链显示名称
pub const CHAIN_ID: &str = "citizenchain"; // 链唯一 ID（chain spec id）
pub const SUPPORT_URL: &str = "https://www.crcfrcn.com"; // 官方支持网址
pub const BLOCK_HASH_COUNT: u32 = 2400; // 最近区块哈希保留数量
pub const NORMAL_DISPATCH_PERCENT: u32 = 75; // 普通交易可用区块权重比例
pub const MAX_BLOCK_BYTES: u32 = 100 * 1024 * 1024; // 单区块最大字节数：100MB

// 省储行质押年利率模型。
pub const PROVINCIALBANK_INITIAL_INTEREST_BP: u32 = 100; // 省储行初始年利率（第一年）：1.00%
pub const PROVINCIALBANK_INTEREST_DECREASE_BP: u32 = 1; // 年利率递减值：0.01%
pub const PROVINCIALBANK_INTEREST_DURATION_YEARS: u32 = 100; // 利率递减年限（100 年后归零）
pub const ENABLE_PROVINCIALBANK_INTEREST_DECAY: bool = true; // 是否启用逐年递减利率模型

// 安全与反滥用参数。
pub const ACCOUNT_EXISTENTIAL_DEPOSIT: u128 = 111; // 账户存在最低余额（Existential Deposit），余额 < 111 分 → 链上账户状态被删除，剩余余额销毁
pub const ALLOW_ZERO_BALANCE_ACCOUNT: bool = false; // 是否允许零余额账户存在（必须关闭）
pub const ENABLE_DUST_CLEANUP: bool = true; // 是否允许 Dust 回收（必须开启）
pub const ALLOW_LOCAL_ADDRESS_GENERATION: bool = true; // 是否允许无限地址本地生成（链下）

/// 地址派生与签名 payload 共用的域分隔符。
pub const GMB: &[u8; 3] = b"GMB";

/// CID 机构号 `cid_number` 的统一最大字节数。
pub const CID_NUMBER_MAX_BYTES: u32 = 32;

// 签名 payload op_tag 从 `crate::sign` 透出。
pub use crate::sign::{OP_SIGN_CITIZEN_IDENTITY, OP_SIGN_DEREGISTER, OP_SIGN_INST};
