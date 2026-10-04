//! 清算行 pallet 集成测试。
//!
//! 覆盖**端到端**路径:构造 mock `Test` runtime(含 `frame_system` +
//! `pallet_balances` + 本 pallet + mock CID),用真实 sr25519 密钥签 L3 支付
//! 意图,经 `submit_offchain_batch` 执行后断言链上 Storage 全部吻合。
//!
//! 测试清单:
//! - `bind_deposit_withdraw_full_cycle`      绑定 / 充值 / 提现三合一
//! - `double_bind_rejected`                  禁止重复绑定
//! - `bind_rejects_unregistered_bank`        拒绝未注册清算行
//! - `switch_requires_zero_balance`          先清零再切换
//! - `switch_after_withdraw_all_works`
//! - `submit_batch_rejects_non_admin`        非管理员提交批次应失败
//! - `submit_batch_same_bank_end_to_end`     单笔同行支付完整结算 + PaymentSettled 事件
//!
//! Mock 与生产 runtime 均由 pallet `Config` 在编译期限定为官方 `AccountId32`；L3
//! 签名账户由 sr25519 `Public` 转换得到，settlement 使用完整32字节验签，不从泛型
//! SCALE 编码截断或猜测公钥。

#![cfg(test)]

use crate as offchain;
use crate::{
    batch_item::OffchainBatchItem, pallet::*, BankTotalDeposits, DepositBalance, L2FeeRateBp,
    L3PaymentNonce, LastClearingBatchSeq, UserBank,
};
use codec::Encode;
use frame_support::{
    assert_noop, assert_ok, construct_runtime, derive_impl, parameter_types,
    traits::{ConstU32, Currency},
    BoundedVec,
};
use sp_core::{sr25519, Pair, H256};
use sp_io::TestExternalities;
use sp_runtime::{AccountId32, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        OffchainTx: offchain,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = sp_runtime::traits::IdentityLookup<AccountId32>;
    type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type Balance = u128;
    type AccountStore = System;
    type ExistentialDeposit = ExistentialDeposit;
}

// ─── 清算行 fixture ───────────────────────────────────────────────────────

const BANK_MAIN_BYTES: [u8; 32] = [0xAA; 32];
const BANK_FEE_BYTES: [u8; 32] = [0xAB; 32];
const BANK_ADMIN_SEED: [u8; 32] = [0xAC; 32];
const BANK_CLEARING_BYTES: [u8; 32] = [0xAD; 32];
const BANK_CID: &[u8] = b"GD001-SCB05-000000001-2026";

// 第二家清算行(跨行结算 fixture):独立 CID(省码 AH≠GD)+ 独立主/费/清算账户。
const BANK2_MAIN_BYTES: [u8; 32] = [0x2A; 32];
const BANK2_FEE_BYTES: [u8; 32] = [0x2B; 32];
const BANK2_CLEARING_BYTES: [u8; 32] = [0x2D; 32];
const BANK2_CID: &[u8] = b"AH001-SCB05-000000002-2026";

fn bank_main() -> AccountId32 {
    AccountId32::new(BANK_MAIN_BYTES)
}
fn bank_fee() -> AccountId32 {
    AccountId32::new(BANK_FEE_BYTES)
}
/// 清算账户:L2 存款准备金池,充值/提现/结算/偿付的资金落点。
fn bank_clearing() -> AccountId32 {
    AccountId32::new(BANK_CLEARING_BYTES)
}
fn bank_admin_pair() -> sr25519::Pair {
    sr25519::Pair::from_seed(&BANK_ADMIN_SEED)
}
fn bank_admin() -> AccountId32 {
    AccountId32::new(bank_admin_pair().public().0)
}
fn bank_cid() -> crate::InstitutionCidNumber {
    BANK_CID.to_vec().try_into().expect("测试 CID 长度合法")
}
/// 第二家清算行的主/费/清算账户与 CID(跨行结算用)。
fn bank2_main() -> AccountId32 {
    AccountId32::new(BANK2_MAIN_BYTES)
}
fn bank2_fee() -> AccountId32 {
    AccountId32::new(BANK2_FEE_BYTES)
}
fn bank2_clearing() -> AccountId32 {
    AccountId32::new(BANK2_CLEARING_BYTES)
}
fn bank2_cid() -> crate::InstitutionCidNumber {
    BANK2_CID.to_vec().try_into().expect("测试 CID 长度合法")
}

fn bank_role_code() -> crate::ActorRoleCode {
    b"CLEARING_OPERATOR"
        .to_vec()
        .try_into()
        .expect("测试岗位码长度合法")
}

/// Mock `CidAccountQuery`:把 `BANK_MAIN_BYTES` 注册为 K1=S 的主账户，
/// `BANK_FEE_BYTES` 注册为 K1=S 的费用账户,`BANK_CLEARING_BYTES` 注册为清算账户;
/// `bank_admin()` 是 `BANK_CID` 的管理员。未注册的 CID / 地址走负路径拒绝。
pub struct MockCid;

impl crate::bank_check::CidAccountQuery<AccountId32> for MockCid {
    fn account_info(addr: &AccountId32) -> Option<(sp_std::vec::Vec<u8>, sp_std::vec::Vec<u8>)> {
        let bytes: &[u8; 32] = addr.as_ref();
        if *bytes == BANK_MAIN_BYTES {
            Some((BANK_CID.to_vec(), "主账户".as_bytes().to_vec()))
        } else if *bytes == BANK_FEE_BYTES {
            Some((BANK_CID.to_vec(), "费用账户".as_bytes().to_vec()))
        } else if *bytes == BANK_CLEARING_BYTES {
            Some((BANK_CID.to_vec(), "清算账户".as_bytes().to_vec()))
        } else if *bytes == BANK2_MAIN_BYTES {
            Some((BANK2_CID.to_vec(), "主账户".as_bytes().to_vec()))
        } else if *bytes == BANK2_FEE_BYTES {
            Some((BANK2_CID.to_vec(), "费用账户".as_bytes().to_vec()))
        } else if *bytes == BANK2_CLEARING_BYTES {
            Some((BANK2_CID.to_vec(), "清算账户".as_bytes().to_vec()))
        } else {
            None
        }
    }

    fn find_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        let (main, fee, clearing) = if cid_number == BANK_CID {
            (BANK_MAIN_BYTES, BANK_FEE_BYTES, BANK_CLEARING_BYTES)
        } else if cid_number == BANK2_CID {
            (BANK2_MAIN_BYTES, BANK2_FEE_BYTES, BANK2_CLEARING_BYTES)
        } else {
            return None;
        };
        if account_name == "主账户".as_bytes() {
            Some(AccountId32::new(main))
        } else if account_name == "费用账户".as_bytes() {
            Some(AccountId32::new(fee))
        } else if account_name == "清算账户".as_bytes() {
            Some(AccountId32::new(clearing))
        } else {
            None
        }
    }

    fn account_exists(addr: &AccountId32) -> bool {
        let bytes: &[u8; 32] = addr.as_ref();
        *bytes == BANK_MAIN_BYTES
            || *bytes == BANK_FEE_BYTES
            || *bytes == BANK_CLEARING_BYTES
            || *bytes == BANK2_MAIN_BYTES
            || *bytes == BANK2_FEE_BYTES
            || *bytes == BANK2_CLEARING_BYTES
    }

    fn is_institution_role_authorized(
        cid_number: &[u8],
        role_code: &[u8],
        who: &AccountId32,
        _action_code: u32,
    ) -> bool {
        cid_number == BANK_CID && role_code == bank_role_code().as_slice() && who == &bank_admin()
    }

    /// 测试 mock:BANK_MAIN / BANK2_MAIN 满足资格白名单,负路径单测自行覆盖。
    fn is_clearing_bank_eligible(addr: &AccountId32) -> bool {
        let bytes: &[u8; 32] = addr.as_ref();
        *bytes == BANK_MAIN_BYTES || *bytes == BANK2_MAIN_BYTES
    }

    /// 测试 mock:BANK_MAIN / BANK2_MAIN 已声明清算行节点。
    fn is_registered_clearing_node(bank: &AccountId32) -> bool {
        let bytes: &[u8; 32] = bank.as_ref();
        *bytes == BANK_MAIN_BYTES || *bytes == BANK2_MAIN_BYTES
    }
}

/// 测试用链上费执行器:真取整(`calculate_onchain_fee`)+ 从费用账户扣款;
/// 把费用账户 seed 成不足即可验证 fail-closed 整批回滚。
pub struct MockOnchainFeeCharger;
impl primitives::fee_policy::OnchainFeeCharger<AccountId32, u128> for MockOnchainFeeCharger {
    fn charge(
        payer_account_id: &AccountId32,
        transaction_amount: u128,
    ) -> Result<u128, sp_runtime::DispatchError> {
        let fee = primitives::fee_policy::calculate_onchain_fee(transaction_amount);
        // 扣下的 NegativeImbalance 直接丢弃(等额销毁);真实分账由 runtime 的
        // OnchainExecutionFeeCharger 走 80/10/10,测试桩只验证费用账户被扣。
        let _imbalance = <Balances as Currency<AccountId32>>::withdraw(
            payer_account_id,
            fee,
            frame_support::traits::WithdrawReasons::FEE,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        )?;
        Ok(fee)
    }
}

impl offchain::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type MaxBatchSize = ConstU32<256>;
    type MaxBatchSignatureLength = ConstU32<128>;
    type InstitutionAsset = (); // fail-open,测试白名单放行
    type CidAccountQuery = MockCid;
    type OnchainFeeCharger = MockOnchainFeeCharger;
    type WeightInfo = ();
}

// ─── 公共 setup ───────────────────────────────────────────────────────────

fn new_test_ext() -> TestExternalities {
    let mut t: TestExternalities = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into();
    t.execute_with(|| {
        // L2 资金池在清算账户;主账户仅身份锚,给少量余额即可。
        Balances::make_free_balance_be(&bank_clearing(), 10_000_000_000u128);
        Balances::make_free_balance_be(&bank_main(), 1_000_000u128);
        Balances::make_free_balance_be(&bank_fee(), 1_000_000u128);
        // 第二家清算行(跨行结算 fixture)。
        Balances::make_free_balance_be(&bank2_clearing(), 10_000_000_000u128);
        Balances::make_free_balance_be(&bank2_main(), 1_000_000u128);
        Balances::make_free_balance_be(&bank2_fee(), 1_000_000u128);
        System::set_block_number(1);
    });
    t
}

/// 从 sr25519 种子生成 `(AccountId32, Pair)`,并给该账户预置余额。
fn new_l3_user(seed: &[u8; 32], balance: u128) -> (AccountId32, sr25519::Pair) {
    let pair = sr25519::Pair::from_seed(seed);
    let acc = AccountId32::new(pair.public().0);
    Balances::make_free_balance_be(&acc, balance);
    (acc, pair)
}

mod cases;
