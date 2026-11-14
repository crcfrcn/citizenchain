//! 同一余额账户的两种收款地址表示。
//!
//! SS58 在界面边界派生；较短的以太坊地址需要通过既有 Revive 反向索引还原账户。
//! System 账户被回收后仍保留映射，确保已分享的收款地址不会改指向回退账户。

use crate::{AccountId, Runtime, System};
use alloc::vec::Vec;
use frame_support::{
    migrations::MultiStepMigrator,
    storage::unhashed,
    traits::{Get, OnNewAccount, OnRuntimeUpgrade},
    weights::{constants::RocksDbWeight, Weight},
};
use pallet_revive::{AccountId32Mapper, AddressMapper};

const MIGRATED: &[u8] = b":citizenchain:evm-account-mapping:v1";

/// 检查 System 建户、业务登记和历史回填共用的幂等映射状态。
/// 仅已有反向映射指向其他账户时判定冲突；回退账户收到资金不获得以太坊别名绑定权。
fn mapping_exists(account: &AccountId) -> Result<bool, &'static str> {
    type Mapper = AccountId32Mapper<Runtime>;
    if Mapper::is_eth_derived(account) {
        return Ok(true);
    }
    let address = Mapper::to_address(account);
    if Mapper::is_mapped(account) {
        return if Mapper::to_account_id(&address) == *account {
            Ok(true)
        } else {
            Err("Ethereum address already maps to another AccountId")
        };
    }
    Ok(false)
}

pub(crate) fn ensure_mapping(account: &AccountId) -> Result<(), &'static str> {
    if mapping_exists(account)? {
        return Ok(());
    }
    // 可信调用方仅包括 System 建户、权威业务账户登记（含零余额账户）及其历史回填。
    // 此处不开放外部登记交易。map() 即使押金为零，也会在 System 写入新账户前检查冻结条件，
    // 因而不能用于 OnNewAccount 回调或尚未入金的已登记业务账户。
    AccountId32Mapper::<Runtime>::map_no_deposit_unchecked(account)
        .map_err(|_| "Ethereum account mapping failed")
}

pub struct EvmAccountMapping;

impl OnNewAccount<AccountId> for EvmAccountMapping {
    fn on_new_account(account: &AccountId) {
        if let Err(error) = ensure_mapping(account) {
            // FRAME 建户回调无法返回错误；保留已有映射，由收款地址查询报告冲突。
            sp_runtime::print(error);
        }
    }
}

impl Get<Weight> for EvmAccountMapping {
    fn get() -> Weight {
        // 为哈希计算、映射检查及一次索引写入预留保守资源上界。
        Weight::from_parts(250_000_000, 12_000)
            .saturating_add(RocksDbWeight::get().reads_writes(12, 1))
    }
}

/// 原生本金转账最多新建一个收款账户；三个公共手续费收款方的预算由
/// EvmPaymentWeight 统一预留，本适配器不重复计算。
pub struct EvmOnchainWeight;
impl onchain::weights::WeightInfo for EvmOnchainWeight {
    fn transfer_with_remark() -> Weight {
        <onchain::weights::SubstrateWeight<Runtime> as onchain::weights::WeightInfo>::transfer_with_remark()
            .saturating_add(EvmAccountMapping::get())
    }
}

// 以下适配器保留既有基准权重表达式，仅追加建户回调的保守预算，
// 不改变手续费金额或转账逻辑。
pub struct EvmPaymentWeight;
impl pallet_transaction_payment::WeightInfo for EvmPaymentWeight {
    fn charge_transaction_payment() -> Weight {
        <pallet_transaction_payment::weights::SubstrateWeight<Runtime> as pallet_transaction_payment::WeightInfo>::charge_transaction_payment()
            .saturating_add(EvmAccountMapping::get().saturating_mul(3))
    }
}

pub struct EvmFullnodeWeight;
impl fullnodeissue::weights::WeightInfo for EvmFullnodeWeight {
    fn bind_reward_account() -> Weight {
        <fullnodeissue::weights::SubstrateWeight<Runtime> as fullnodeissue::weights::WeightInfo>::bind_reward_account()
    }
    fn rebind_reward_account() -> Weight {
        <fullnodeissue::weights::SubstrateWeight<Runtime> as fullnodeissue::weights::WeightInfo>::rebind_reward_account()
    }
    fn on_finalize_reward() -> Weight {
        <fullnodeissue::weights::SubstrateWeight<Runtime> as fullnodeissue::weights::WeightInfo>::on_finalize_reward()
            .saturating_add(EvmAccountMapping::get())
    }
}

pub struct EvmCitizenWeight;
impl citizenissue::weights::WeightInfo for EvmCitizenWeight {
    fn on_voting_identity_registered() -> Weight {
        // 身份模块提前预留本回调对应的延迟奖励发放预算。
        <citizenissue::weights::SubstrateWeight<Runtime> as citizenissue::weights::WeightInfo>::on_voting_identity_registered()
            .saturating_add(EvmAccountMapping::get())
    }
}

pub struct EvmProvincialWeight;
impl prbbankissue::weights::WeightInfo for EvmProvincialWeight {
    fn on_finalize_settlement() -> Weight {
        <prbbankissue::weights::SubstrateWeight<Runtime> as prbbankissue::weights::WeightInfo>::on_finalize_settlement()
            .saturating_add(EvmAccountMapping::get().saturating_mul(primitives::cid::china::china_ch::CHINA_CH.len() as u64))
    }
}

pub struct EvmPersonalWeight;
impl personalentity::weights::WeightInfo for EvmPersonalWeight {
    fn propose_create(n: u32) -> Weight {
        // 既有方法同时提供异步执行上界：一个本金收款方及最多三个内部分账收款方。
        <personalentity::weights::SubstrateWeight<Runtime> as personalentity::weights::WeightInfo>::propose_create(n)
            .saturating_add(EvmAccountMapping::get().saturating_mul(4))
    }
    fn propose_close() -> Weight {
        <personalentity::weights::SubstrateWeight<Runtime> as personalentity::weights::WeightInfo>::propose_close()
            .saturating_add(EvmAccountMapping::get().saturating_mul(4))
    }
    fn terminal_callback_ignored() -> Weight {
        <personalentity::weights::SubstrateWeight<Runtime> as personalentity::weights::WeightInfo>::terminal_callback_ignored()
    }
    fn terminal_callback() -> Weight {
        <personalentity::weights::SubstrateWeight<Runtime> as personalentity::weights::WeightInfo>::terminal_callback()
    }
}

pub struct EvmSquareWeight;
impl square::weights::WeightInfo for EvmSquareWeight {
    fn publish_post() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::publish_post()
    }
    fn cancel() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::cancel()
    }
    fn set_creator_plans(n: u32) -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::set_creator_plans(n)
    }
    fn update_creator_tier_name() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::update_creator_tier_name()
    }
    fn process_one_due() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::process_one_due(
        )
        .saturating_add(EvmAccountMapping::get())
    }
    fn subscribe() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::subscribe()
            .saturating_add(EvmAccountMapping::get())
    }
    fn change_subscription_plan() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::change_subscription_plan()
            .saturating_add(EvmAccountMapping::get())
    }
    fn propose_set_platform_price() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::propose_set_platform_price()
    }
    fn renewal_success() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::renewal_success(
        )
        .saturating_add(EvmAccountMapping::get())
    }
    fn renewal_creator() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::renewal_creator(
        )
        .saturating_add(EvmAccountMapping::get())
    }
    fn on_initialize_base() -> Weight {
        <square::weights::SubstrateWeight<Runtime> as square::weights::WeightInfo>::on_initialize_base()
    }
}

const PROGRESS: &[u8] = b":citizenchain:evm-account-mapping:progress:v1";

#[derive(codec::Encode, codec::Decode)]
struct Progress {
    stage: u8,
    cursor: Option<Vec<u8>>,
}

// 使用 FRAME 类型化迭代器，从准确的原始存储键之后续跑；
// 不建立第二份账户索引，不无界收集账户，也不假定底层存储哈希布局。
fn next_account(progress: &Progress) -> Option<(Vec<u8>, AccountId, bool)> {
    macro_rules! next {
        ($map:ty) => {{
            let mut iterator = match &progress.cursor {
                Some(key) => <$map>::iter_keys_from(key.clone()),
                None => <$map>::iter_keys(),
            };
            iterator
                .next()
                .map(|account| (iterator.last_raw_key().to_vec(), account, true))
        }};
    }
    match progress.stage {
        0 => next!(frame_system::Account<Runtime>),
        1 => next!(publicentity::AccountRegisteredCid<Runtime>),
        2 => next!(privateentity::AccountRegisteredCid<Runtime>),
        3 => next!(personalentity::PersonalAccounts<Runtime>).map(|(key, account, _)| {
            let active = personalentity::PersonalAccounts::<Runtime>::get(&account)
                .is_some_and(|info| info.status == personalentity::types::PersonalStatus::Active);
            (key, account, active)
        }),
        4 => next!(identity::CidByAccountId<Runtime>),
        _ => None,
    }
}

/// 首次升级与有界续跑共用本模块；续跑在正常非空区块的固有交易处理后执行。
/// 期间仅暂停 EVM 写交易，原生交易继续驱动出块，遵守本链禁止空块的规则。
pub struct BackfillEvmAccounts;

impl BackfillEvmAccounts {
    pub(crate) fn pending() -> bool {
        <Self as MultiStepMigrator>::ongoing()
    }
}

impl frame_support::traits::PostInherents for BackfillEvmAccounts {
    fn post_inherents() {
        if Self::pending() {
            let used = <Self as MultiStepMigrator>::step();
            // 在接纳普通交易前计入本次有界迁移权重；
            // 不允许仅含时间戳的区块，也不绕过原有挖矿门控。
            System::register_extra_weight_unchecked(
                used,
                frame_support::dispatch::DispatchClass::Mandatory,
            );
        }
    }
}

impl MultiStepMigrator for BackfillEvmAccounts {
    fn ongoing() -> bool {
        unhashed::exists(PROGRESS)
    }

    fn step() -> Weight {
        let db = RocksDbWeight::get();
        if !Self::ongoing() {
            return db.reads(1);
        }
        let mut progress =
            unhashed::get::<Progress>(PROGRESS).expect("EVM account migration cursor must decode");
        let maximum = crate::configs::RuntimeBlockWeights::get().max_block;
        let budget = Weight::from_parts(maximum.ref_time() / 4, maximum.proof_size() / 4);
        let cost = EvmAccountMapping::get();
        let mut used = db
            .reads_writes(1, 3)
            .saturating_add(Weight::from_parts(0, 1_024));
        while used.saturating_add(cost).all_lte(budget) {
            used = used.saturating_add(cost);
            if progress.stage >= 5 {
                unhashed::put(MIGRATED, &true);
                unhashed::kill(PROGRESS);
                return used;
            }
            match next_account(&progress) {
                Some((key, account, active)) => {
                    if active {
                        // 若预检后状态再次出现冲突，保持原生治理可用，并在后续区块重试当前账户；
                        // EVM 写交易继续暂停，且不写入迁移完成标记。
                        if let Err(error) = ensure_mapping(&account) {
                            sp_runtime::print(error);
                            unhashed::put(PROGRESS, &progress);
                            return used;
                        }
                    }
                    progress.cursor = Some(key);
                }
                None => {
                    progress.stage += 1;
                    progress.cursor = None;
                }
            }
        }
        unhashed::put(PROGRESS, &progress);
        used
    }
}

impl OnRuntimeUpgrade for BackfillEvmAccounts {
    fn on_runtime_upgrade() -> Weight {
        if unhashed::get::<bool>(MIGRATED) == Some(true) {
            return RocksDbWeight::get().reads(1);
        }
        if !Self::ongoing() {
            unhashed::put(
                PROGRESS,
                &Progress {
                    stage: 0,
                    cursor: None,
                },
            );
        }
        // 再次进入时沿既有游标续跑，不重置尚未完成的进度。
        Self::step().saturating_add(RocksDbWeight::get().reads_writes(2, 1))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        use codec::Encode;
        let mut progress = Progress {
            stage: 0,
            cursor: None,
        };
        while progress.stage < 5 {
            match next_account(&progress) {
                Some((key, account, active)) => {
                    if active {
                        mapping_exists(&account).map_err(sp_runtime::TryRuntimeError::Other)?;
                    }
                    progress.cursor = Some(key);
                }
                None => {
                    progress.stage += 1;
                    progress.cursor = None;
                }
            }
        }
        Ok(crate::Balances::total_issuance().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        use codec::Decode;
        let issuance = crate::Balance::decode(&mut &state[..])
            .map_err(|_| "EVM backfill pre-upgrade state is invalid")?;
        if crate::Balances::total_issuance() != issuance {
            return Err("EVM mapping changed total issuance".into());
        }
        if !Self::ongoing() && unhashed::get::<bool>(MIGRATED) != Some(true) {
            return Err("EVM mapping backfill was neither scheduled nor completed".into());
        }
        Ok(())
    }
}
