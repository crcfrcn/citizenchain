//! # 链上公民身份模块 (citizen-identity)
//!
//! 本模块是公民链上身份唯一真源。OnChina 只能作为注册局操作入口提交交易,
//! 投票引擎只能读取本模块的投票身份、参选身份和人口快照。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::ConstU32;
use frame_support::BoundedVec;
use scale_info::TypeInfo;
use sp_io::hashing::blake2_256;
use sp_runtime::Debug;

use core::marker::PhantomData;

pub type CidNumberBound = BoundedVec<u8, ConstU32<32>>;
pub type AreaCodeBound = BoundedVec<u8, ConstU32<16>>;
pub type RoleCodeBound = BoundedVec<u8, ConstU32<64>>;

/// CID 首次绑定或换绑授权自签发起后允许提交上链的最长时间（Unix 秒）。
pub const MAX_CID_AUTHORIZATION_LIFETIME_SECS: u64 = 600;

/// CID 换绑离线授权的唯一 SCALE 载荷。
///
/// 字段顺序是公民、冷钱包、OnChina 与 runtime 的共同协议；绑定创世哈希、当前账户和
/// 当前 revision，确保授权不能跨链、跨绑定轮次或在账户再次轮换后重放。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CidRebindAuthorization<Hash, AccountId> {
    pub genesis_hash: Hash,
    pub cid_number: CidNumberBound,
    pub current_account_id: AccountId,
    pub new_account_id: AccountId,
    pub expected_binding_revision: u64,
    /// Unix 秒；必须晚于当前链上时间，且不得超过当前时间 600 秒。
    pub expires_at: u64,
}

/// 注册局首次占号时由新账户签署的唯一 SCALE 载荷。
///
/// `expected_binding_revision` 只能为 0；链上同时要求 CID 尚无登记、绑定和 revision，
/// 因而公开过的首次绑定证明不能在吊销、换绑或其它链上再次使用。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CidOccupyAuthorization<Hash, AccountId> {
    pub genesis_hash: Hash,
    pub cid_number: CidNumberBound,
    pub account_id: AccountId,
    pub expected_binding_revision: u64,
    /// Unix 秒；必须晚于当前链上时间，且不得超过当前时间 600 秒。
    pub expires_at: u64,
}

/// 自助占号(公民本人、无注册局)时写入 `CidRecord.registrar_cid_number` 的 sentinel。
/// 区别于注册局代占(存注册局机构 CID);自助记录 registrar = `SELF`、居住地空。
pub const SELF_OCCUPY_REGISTRAR: &[u8] = b"SELF";
/// 公民姓、名各自的最大字节数；与管理员人员姓名字段保持一致。
pub const PERSON_NAME_MAX_BYTES: u32 = 128;
/// 姓。结构本身已经限定公民语义，字段和类型都不再重复增加 `citizen_` 前缀。
pub type FamilyName = BoundedVec<u8, ConstU32<PERSON_NAME_MAX_BYTES>>;
/// 名。与 `family_name` 分开保存，不生成或存储合并姓名。
pub type GivenName = BoundedVec<u8, ConstU32<PERSON_NAME_MAX_BYTES>>;
pub const MIN_ONCHAIN_CITIZEN_AGE_YEARS: u8 = 16;

/// 从 Runtime 最大区块权重派生公民人口日期维护的独立预算。
///
/// 日期推进只使用 `on_idle` 的剩余权重，并进一步受本预算、每日转换数量和推进天数
/// 三重上限约束，避免集中到期的人口变化挤占业务交易。
pub struct PopulationMaintenanceWeightFraction<T, const DIVISOR: u64>(PhantomData<T>);

impl<T: frame_system::Config, const DIVISOR: u64>
    frame_support::traits::Get<frame_support::weights::Weight>
    for PopulationMaintenanceWeightFraction<T, DIVISOR>
{
    fn get() -> frame_support::weights::Weight {
        let divisor = DIVISOR.max(1);
        let max = <T as frame_system::Config>::BlockWeights::get().max_block;
        frame_support::weights::Weight::from_parts(
            max.ref_time() / divisor,
            max.proof_size() / divisor,
        )
    }
}

/// CID 占号登记状态:吊销走墓碑,存储项永不删除、号码永不复用。
#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
pub enum CidRecordStatus {
    Active = 0,
    Revoked = 1,
}

/// CID 占号登记记录:链上写入时原子查重的唯一仲裁真源。
///
/// 只含号码归属与承诺哈希,不含姓名生日等隐私;居住地码用于吊销时的
/// 注册局作用域授权;承诺哈希用于建档落库失败后的幂等续用识别。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CidRecord<BlockNumber> {
    /// 执行登记的注册局机构 CID；管理员账户只存在于外层签名 origin。
    pub registrar_cid_number: CidNumberBound,
    pub commitment: [u8; 32],
    pub residence_province_code: AreaCodeBound,
    pub residence_city_code: AreaCodeBound,
    pub status: CidRecordStatus,
    pub registered_at: BlockNumber,
    pub revoked_at: Option<BlockNumber>,
}

/// days since 1970-01-01 → 公历 (年, 月, 日)。
///
/// Howard Hinnant civil-from-days 整数算法,与 chrono 等价;no_std 下自带,
/// 供护照有效期(YYYYMMDD)与链上时间戳比对。
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
#[derive(Default)]
pub enum CitizenStatus {
    #[default]
    Normal = 0,
    Revoked = 1,
}

/// 公民性别(参选身份公开档案字段)。
#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
pub enum CitizenSex {
    Male = 0,
    Female = 1,
}

#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
pub enum CitizenIdentityLevel {
    Voting = 1,
    Candidate = 2,
}

/// 公民授权主体。
///
/// 公民 CID 与账户必须同时匹配 `citizen-identity` 的有效双向绑定：CID 是唯一
/// 身份主键并承载身份权益，当前账户只证明本次操作获得该 CID 控制者授权。
/// 本结构只在读取时构造，不作为新的身份或权限 Storage。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CitizenSubject<AccountId> {
    /// 公民 CID 号；由本模块保存的有效身份提供，消费方不得自行生成或修改。
    pub cid_number: CidNumberBound,
    /// 公民账户；用于验证签名，并与 `cid_number` 共同确认公民主体。
    pub account_id: AccountId,
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct VotingIdentityPayload<AccountId> {
    pub cid_number: CidNumberBound,
    pub account_id: AccountId,
    pub passport_valid_from: u32,
    pub passport_valid_until: u32,
    pub citizen_status: CitizenStatus,
    pub residence_province_code: AreaCodeBound,
    pub residence_city_code: AreaCodeBound,
    pub residence_town_code: AreaCodeBound,
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CandidateIdentityPayload<AccountId> {
    /// 公民的完整投票身份载荷；竞选身份必须建立在有效投票身份之上。
    pub voting: VotingIdentityPayload<AccountId>,
    /// 出生省级行政区代码；表示出生地，不表示当前居住地。
    pub birth_province_code: AreaCodeBound,
    /// 出生市级行政区代码；表示出生地，不表示当前居住地。
    pub birth_city_code: AreaCodeBound,
    /// 出生镇级行政区代码；表示出生地，不表示当前居住地。
    pub birth_town_code: AreaCodeBound,
    /// 姓；直接使用公民身份真源中的 `family_name`，不生成合并姓名。
    pub family_name: FamilyName,
    /// 名；直接使用公民身份真源中的 `given_name`，不生成合并姓名。
    pub given_name: GivenName,
    /// 公民性别；用于竞选资格校验和竞选信息展示。
    pub citizen_sex: CitizenSex,
    /// 出生日期(YYYYMMDD 整数)。仅竞选身份携带,写入后不可修改;
    /// 链上凭此实时计算竞选公民年龄(见 `candidate_age`)。
    pub birth_date: u32,
}

/// 公民对本次身份写入的同意授权；四端共用，字段声明顺序即 SCALE 协议顺序。
///
/// `genesis_hash` 由链上注入，锁定授权只对本链有效；
/// `expected_identity_version` 必须等于该 CID 当前身份版本，锁死历史载荷回滚；
/// `expires_at` 为 Unix 秒，上限 [`MAX_CID_AUTHORIZATION_LIFETIME_SECS`]。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CitizenIdentityAuthorization<Hash, Payload> {
    pub genesis_hash: Hash,
    pub payload: Payload,
    pub expected_identity_version: u64,
    /// Unix 秒；必须晚于当前链上时间，且不得超过当前时间 600 秒。
    pub expires_at: u64,
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct VotingIdentity<BlockNumber> {
    pub passport_valid_from: u32,
    pub passport_valid_until: u32,
    pub citizen_status: CitizenStatus,
    pub residence_province_code: AreaCodeBound,
    pub residence_city_code: AreaCodeBound,
    pub residence_town_code: AreaCodeBound,
    pub updated_at: BlockNumber,
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct CandidateIdentity<BlockNumber> {
    /// 生成该竞选身份时采用的出生省级行政区代码。
    pub birth_province_code: AreaCodeBound,
    /// 生成该竞选身份时采用的出生市级行政区代码。
    pub birth_city_code: AreaCodeBound,
    /// 生成该竞选身份时采用的出生镇级行政区代码。
    pub birth_town_code: AreaCodeBound,
    /// 生成该竞选身份时采用的姓。
    pub family_name: FamilyName,
    /// 生成该竞选身份时采用的名。
    pub given_name: GivenName,
    /// 生成该竞选身份时采用的公民性别。
    pub citizen_sex: CitizenSex,
    /// 出生日期(YYYYMMDD 整数),写一次即锁定,后续更新不得变更。
    pub birth_date: u32,
    /// 最近一次写入或更新该竞选身份的区块号；不代表现实世界时间。
    pub updated_at: BlockNumber,
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum PopulationScope {
    Country,
    Province(AreaCodeBound),
    City(AreaCodeBound, AreaCodeBound),
    Town(AreaCodeBound, AreaCodeBound, AreaCodeBound),
}

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct PopulationData {
    pub scope: PopulationScope,
    pub eligible_total: u64,
    /// 读取人口数据时已经提交的最后一个身份资格版本。
    pub eligibility_revision: u64,
    /// 读取人口数据时的 UTC+8 日期，投票引擎据此冻结护照判定日期。
    /// 该人口数据用于资格历史判定的 UTC+8 日期；本字段不是身份模块快照标识。
    pub eligibility_date: u32,
}

/// 护照日期变化对四级有效人口的影响。
#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
pub enum PopulationTransitionKind {
    /// 护照从本日开始生效，满足其他身份条件时加入四级人口。
    Activate = 0,
    /// 护照有效期已于前一日结束，满足同一身份 revision 时退出四级人口。
    Deactivate = 1,
}

/// 单个永久 CID 的日期人口转换项。
///
/// 只保存 CID、身份 revision 和转换种类；账户、姓名、居住地和身份全文继续从
/// `citizen-identity` 唯一真源读取，不在日期队列重复保存。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct PopulationTransition {
    pub cid_number: CidNumberBound,
    pub eligibility_revision: u64,
    pub transition_kind: PopulationTransitionKind,
}

/// 四级人口维护发现的不可恢复不变量错误。
///
/// 一旦写入故障状态，人口读取和身份人口变更全部 fail-closed；本模块不提供管理员
/// 清除入口，防止绕过链上人口真源。
#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
#[repr(u8)]
pub enum PopulationFault {
    DateMovedBackwards = 0,
    InvalidReadyDate = 1,
    CounterOverflow = 2,
    CounterUnderflow = 3,
    MissingTransition = 4,
}

/// 单个账户的一段不可变投票资格历史。
///
/// 全局 revision 区分同一区块内的多次身份写入；`valid_until_revision` 为开区间上界。
/// 公投按 snapshot revision 二分定位版本，不依赖投票时的当前身份。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct VotingEligibilityVersion<BlockNumber> {
    pub identity: VotingIdentity<BlockNumber>,
    pub valid_from_revision: u64,
    pub valid_until_revision: Option<u64>,
}

/// Polkadot SDK 官方 benchmark helper 模式：在 benchmark externalities 的临时
/// keystore 中生成 sr25519 签名者并签署真实消息。helper 只能准备计时区间外的夹具，
/// 不得改变正式验签结果。
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<AccountId, Signature> {
    fn signer() -> (sp_core::sr25519::Public, AccountId);
    fn sign(signer: &sp_core::sr25519::Public, message: &[u8]) -> Signature;
}

pub trait CitizenIdentityAuthority<AccountId, Signature> {
    fn can_manage_voting_identity(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        residence_province_code: &[u8],
        residence_city_code: &[u8],
        level: CitizenIdentityLevel,
        action_code: u32,
    ) -> bool;

    fn verify_citizen_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &Signature,
    ) -> bool;

    /// 校验当前绑定账户对匿名 CID 自助换绑的授权签名(op_tag `OP_SIGN_CID_REBIND`);
    /// 与 `verify_citizen_signature` 同 sr25519 校验、不同签名域(域分离防重放)。
    fn verify_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &Signature,
    ) -> bool;

    /// 匿名 CID 管理鉴权(注册局占号/换绑):任一在册注册局(CREG 市级 / FRG 省级)
    /// 持 citizen-identity 管理权即可,**不做辖区匹配**——CID 是全国号、匿名无省市归属。
    /// 与 `can_manage_voting_identity`(需居住地作用域精确匹配)分离。
    fn can_manage_anonymous_cid(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool;

    /// 校验用户对注册局首次绑定 [`CidOccupyAuthorization`] 的授权签名
    /// (op_tag `OP_SIGN_CID_OCCUPY`)，证明新账户受控。
    fn verify_occupy_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &Signature,
    ) -> bool;

    /// 校验注册局代匿名或实名 CID 换绑时，新账户对 [`CidRebindAuthorization`] 的
    /// 控制证明(op_tag `OP_SIGN_CID_ADMIN_REBIND`)；与占号域分离，防止跨动作重放。
    fn verify_admin_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &Signature,
    ) -> bool;

    /// 为 FRAME benchmark 返回一组真实注册局岗位授权主体。
    ///
    /// 具体 runtime 必须从其正式创世机构、岗位和任职目录选择主体；benchmark
    /// 只在计时区间外准备夹具，计时区间内仍走与生产一致的岗位授权读取。
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority() -> Option<(
        AccountId,
        CidNumberBound,
        RoleCodeBound,
        AreaCodeBound,
        AreaCodeBound,
    )> {
        None
    }

    /// 调整 benchmark externalities 的链上时间；仅用于覆盖人口日期推进路径。
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_timestamp(_timestamp_millis: u64) {}
}

impl<AccountId, Signature> CitizenIdentityAuthority<AccountId, Signature> for () {
    fn can_manage_voting_identity(
        _registrar: &AccountId,
        _actor_cid_number: &[u8],
        _actor_role_code: &[u8],
        _residence_province_code: &[u8],
        _residence_city_code: &[u8],
        _level: CitizenIdentityLevel,
        _action_code: u32,
    ) -> bool {
        false
    }

    fn verify_citizen_signature(
        _account_id: &AccountId,
        _payload: &[u8],
        _signature: &Signature,
    ) -> bool {
        false
    }

    fn verify_rebind_signature(
        _account_id: &AccountId,
        _payload: &[u8],
        _signature: &Signature,
    ) -> bool {
        false
    }

    fn can_manage_anonymous_cid(
        _registrar: &AccountId,
        _actor_cid_number: &[u8],
        _actor_role_code: &[u8],
        _action_code: u32,
    ) -> bool {
        false
    }

    fn verify_occupy_signature(
        _account_id: &AccountId,
        _payload: &[u8],
        _signature: &Signature,
    ) -> bool {
        false
    }

    fn verify_admin_rebind_signature(
        _account_id: &AccountId,
        _payload: &[u8],
        _signature: &Signature,
    ) -> bool {
        false
    }
}

pub trait OnVotingIdentityRegistered<AccountId> {
    fn on_voting_identity_registered(_who: &AccountId, _cid_number: &CidNumberBound) {}
}

impl<AccountId> OnVotingIdentityRegistered<AccountId> for () {}

pub trait OnVotingIdentityRegisteredWeight {
    fn on_voting_identity_registered_weight() -> frame_support::weights::Weight {
        frame_support::weights::Weight::zero()
    }
}

impl OnVotingIdentityRegisteredWeight for () {}

pub trait CitizenIdentityProvider<AccountId> {
    /// 读取经过 CID 状态和 CID↔账户双向绑定校验的完整公民主体。
    fn citizen_subject(who: &AccountId) -> Option<CitizenSubject<AccountId>>;
    /// 返回当前日期在指定作用域内有效的完整投票公民主体。
    fn voting_subject(
        who: &AccountId,
        scope: &PopulationScope,
    ) -> Option<CitizenSubject<AccountId>>;
    /// 返回当前日期在指定作用域内有效的完整竞选公民主体。
    fn candidate_subject(
        who: &AccountId,
        scope: &PopulationScope,
    ) -> Option<CitizenSubject<AccountId>>;
    /// 只在四级人口已经完整推进到当前 UTC+8 日期时返回数据。
    fn population_data(scope: &PopulationScope) -> Option<PopulationData>;
    /// 按投票引擎冻结的人口数据返回完整投票公民主体。
    fn voting_subject_at(
        who: &AccountId,
        population_data: &PopulationData,
    ) -> Option<CitizenSubject<AccountId>>;
}

impl<AccountId> CitizenIdentityProvider<AccountId> for () {
    fn citizen_subject(_who: &AccountId) -> Option<CitizenSubject<AccountId>> {
        None
    }

    fn voting_subject(
        _who: &AccountId,
        _scope: &PopulationScope,
    ) -> Option<CitizenSubject<AccountId>> {
        None
    }

    fn candidate_subject(
        _who: &AccountId,
        _scope: &PopulationScope,
    ) -> Option<CitizenSubject<AccountId>> {
        None
    }

    fn population_data(_scope: &PopulationScope) -> Option<PopulationData> {
        None
    }

    fn voting_subject_at(
        _who: &AccountId,
        _population_data: &PopulationData,
    ) -> Option<CitizenSubject<AccountId>> {
        None
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    /// 创世链直接采用当前存储结构，不保留历史迁移或兼容分支。
    pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    pub type SignatureOf<T> = BoundedVec<u8, <T as Config>::MaxCitizenSignatureLength>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxCitizenSignatureLength: Get<u32>;

        type CitizenIdentityAuthority: CitizenIdentityAuthority<Self::AccountId, SignatureOf<Self>>;

        /// 只负责为 FRAME benchmark 生成真实账户和签名，不参与生产鉴权。
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: BenchmarkHelper<Self::AccountId, SignatureOf<Self>>;

        type OnVotingIdentityRegistered: OnVotingIdentityRegistered<Self::AccountId>
            + OnVotingIdentityRegisteredWeight;

        /// 链上时间源(pallet-timestamp),用于投票时校验护照有效期窗口。
        type TimeProvider: frame_support::traits::UnixTime;

        /// 单个区块最多推进的自然日数量；空日期同样受此上限保护。
        #[pallet::constant]
        type MaxPopulationDaysPerBlock: Get<u32>;

        /// 单个区块最多处理的护照生效或到期转换项数量。
        #[pallet::constant]
        type MaxPopulationTransitionsPerBlock: Get<u32>;

        /// 人口日期维护在单个区块内可使用的独立最大权重。
        type MaxPopulationMaintenanceWeightPerBlock: Get<frame_support::weights::Weight>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    /// 永久公民 CID 到投票身份。CID 是身份主键，账户不参与身份寻址。
    pub type VotingIdentityByCid<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        CidNumberBound,
        VotingIdentity<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 永久公民 CID 到竞选身份。更换签名账户不会搬迁竞选资料。
    #[pallet::storage]
    pub type CandidateIdentityByCid<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        CidNumberBound,
        CandidateIdentity<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 永久公民 CID 当前绑定的唯一签名账户。
    #[pallet::storage]
    pub type AccountIdByCid<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberBound, T::AccountId, OptionQuery>;

    /// 当前签名账户反向绑定的永久公民 CID；与 `AccountIdByCid` 必须闭环。
    #[pallet::storage]
    pub type CidByAccountId<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, CidNumberBound, OptionQuery>;

    /// CID 当前账户绑定的单调修订号；首次绑定为 1，每次换绑或吊销严格加 1。
    #[pallet::storage]
    pub type BindingRevisionByCid<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberBound, u64, OptionQuery>;

    /// CID 占号登记表:发号全局唯一的链上真源(占号先行,墓碑不删除)。
    #[pallet::storage]
    pub type CidRegistry<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberBound, CidRecord<BlockNumberFor<T>>, OptionQuery>;

    /// 当前有效(Active)CID 数量:占号 +1、吊销 −1,恒等于 `CidRegistry` 里 Active 记录数。
    ///
    /// `CidRegistry` 保留吊销墓碑,直接数键会把墓碑一起算进去;本计数只反映当前有效号,
    /// 且是定长单值,避免为了拿一个数字去做全表前缀扫描。
    #[pallet::storage]
    pub type CidCount<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type CountryVotingCount<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type ProvinceVotingCount<T: Config> =
        StorageMap<_, Blake2_128Concat, AreaCodeBound, u64, ValueQuery>;

    #[pallet::storage]
    pub type CityVotingCount<T: Config> =
        StorageMap<_, Blake2_128Concat, (AreaCodeBound, AreaCodeBound), u64, ValueQuery>;

    #[pallet::storage]
    pub type TownVotingCount<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (AreaCodeBound, AreaCodeBound, AreaCodeBound),
        u64,
        ValueQuery,
    >;

    /// 四级人口计数已经完整推进至的 UTC+8 日期；`0` 表示尚未初始化。
    #[pallet::storage]
    pub type PopulationReadyDate<T> = StorageValue<_, u32, ValueQuery>;

    /// 指定日期已经登记的转换项数量，同时作为该日期下一个顺序号。
    #[pallet::storage]
    pub type PopulationTransitionCountByDate<T> =
        StorageMap<_, Blake2_128Concat, u32, u64, ValueQuery>;

    /// 指定日期尚未处理的第一个转换项顺序号。
    #[pallet::storage]
    pub type PopulationTransitionCursorByDate<T> =
        StorageMap<_, Blake2_128Concat, u32, u64, ValueQuery>;

    /// `(UTC+8 日期, 日期内顺序号)` 到人口转换项。
    #[pallet::storage]
    pub type PopulationTransitions<T> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u32,
        Blake2_128Concat,
        u64,
        PopulationTransition,
        OptionQuery,
    >;

    /// 人口维护故障；存在时身份人口变更和新人口快照均永久 fail-closed。
    #[pallet::storage]
    pub type PopulationMaintenanceFault<T> = StorageValue<_, PopulationFault, OptionQuery>;

    /// 全局身份资格修订号。每次投票身份写入严格递增，用于冻结同区块交易顺序。
    #[pallet::storage]
    pub type NextEligibilityRevision<T> = StorageValue<_, u64, ValueQuery>;

    /// 单个永久 CID 的历史版本数量；版本索引为 0..count，支持按 revision 有界二分。
    #[pallet::storage]
    pub type VotingEligibilityVersionCount<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberBound, u64, ValueQuery>;

    /// 永久 CID 的不可变投票资格历史：(CID, 版本序号) → 资格区间。
    #[pallet::storage]
    pub type VotingEligibilityVersions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberBound,
        Blake2_128Concat,
        u64,
        VotingEligibilityVersion<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 创世身份绑定配置。
    ///
    /// 每项依次为 `(cid_number, account_id, registrar_cid_number)`。创世只建立
    /// Active CID 登记和 CID↔AccountId 双向闭环，不凭空生成投票或竞选身份。
    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub initial_cid_bindings: Vec<(CidNumberBound, T::AccountId, CidNumberBound)>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (index, (cid_number, account_id, registrar_cid_number)) in
                self.initial_cid_bindings.iter().enumerate()
            {
                assert!(!cid_number.is_empty(), "创世 CID 不能为空");
                assert!(
                    !registrar_cid_number.is_empty(),
                    "创世 CID 的登记来源不能为空"
                );
                Pallet::<T>::ensure_valid_self_registrable_cid(cid_number)
                    .expect("创世 CID 必须是合法的 CTZN 或 NATP 人主体号码");
                for (previous_cid, previous_account_id, _) in
                    self.initial_cid_bindings[..index].iter()
                {
                    assert!(previous_cid != cid_number, "创世 CID 不能重复");
                    assert!(
                        previous_account_id != account_id,
                        "创世 AccountId 不能绑定多个 CID"
                    );
                }
                assert!(
                    !CidRegistry::<T>::contains_key(cid_number),
                    "创世 CID 已存在登记记录"
                );
                assert!(
                    !AccountIdByCid::<T>::contains_key(cid_number),
                    "创世 CID 已存在账户绑定"
                );
                assert!(
                    !CidByAccountId::<T>::contains_key(account_id),
                    "创世 AccountId 已存在 CID 反向绑定"
                );

                let record = CidRecord {
                    registrar_cid_number: registrar_cid_number.clone(),
                    commitment: blake2_256(&account_id.encode()),
                    residence_province_code: AreaCodeBound::default(),
                    residence_city_code: AreaCodeBound::default(),
                    status: CidRecordStatus::Active,
                    registered_at: BlockNumberFor::<T>::default(),
                    revoked_at: None,
                };
                CidRegistry::<T>::insert(cid_number, record);
                AccountIdByCid::<T>::insert(cid_number, account_id);
                CidByAccountId::<T>::insert(account_id, cid_number);
                BindingRevisionByCid::<T>::insert(cid_number, 1);
            }
            // 创世登记全部是 Active(上面逐条断言过 CID 不重复),计数直接取条数。
            CidCount::<T>::put(self.initial_cid_bindings.len() as u64);
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        VotingIdentityRegistered {
            account_id: T::AccountId,
            cid_number: CidNumberBound,
        },
        VotingIdentityUpdated {
            account_id: T::AccountId,
            cid_number: CidNumberBound,
        },
        CandidateIdentityUpgraded {
            account_id: T::AccountId,
            cid_number: CidNumberBound,
        },
        CandidateIdentityUpdated {
            account_id: T::AccountId,
            cid_number: CidNumberBound,
        },
        CitizenIdentityRevoked {
            account_id: T::AccountId,
            cid_number: CidNumberBound,
            binding_revision: u64,
        },
        CidOccupied {
            cid_number: CidNumberBound,
            registrar_cid_number: CidNumberBound,
            binding_revision: u64,
        },
        /// 公民自助占号(匿名):本人自签占号 + 占即绑账户,无注册局、无公民档。
        CidSelfOccupied {
            cid_number: CidNumberBound,
            account_id: T::AccountId,
            binding_revision: u64,
        },
        /// CID 完成换绑；业务数据仍由永久 CID 寻址。
        CidAccountIdRebound {
            cid_number: CidNumberBound,
            previous_account_id: T::AccountId,
            new_account_id: T::AccountId,
            binding_revision: u64,
        },
        CidRevoked {
            cid_number: CidNumberBound,
            binding_revision: u64,
        },
        /// 四级人口已经完整推进至该 UTC+8 日期。
        PopulationDateReady { eligibility_date: u32 },
        /// 日期推进发现计数或日期不变量损坏，人口读取随即 fail-closed。
        PopulationMaintenanceFaulted {
            eligibility_date: u32,
            fault: PopulationFault,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        EmptyCidNumber,
        EmptyResidenceScope,
        EmptyBirthScope,
        EmptyFamilyName,
        EmptyGivenName,
        /// 出生日期非法(非 YYYYMMDD 或无法计算年龄)。
        InvalidBirthDate,
        /// 出生日期写入后不可修改,更新竞选身份时不得变更。
        BirthDateImmutable,
        /// 出生省市镇、性别写入竞选档后不可修改(D4a:填后锁定)。
        CandidateProfileImmutable,
        InvalidDateRange,
        InvalidCitizenCode,
        UnauthorizedRegistrar,
        InvalidCitizenSignature,
        /// 注册局占号:用户对占号账户的授权签名无效。
        InvalidOccupySignature,
        /// 竞选身份未达最低参选年龄；投票资格不设年龄门，本错误只用于竞选档。
        UnderCandidateAge,
        /// 该永久 CID 已经建立投票身份；登记入口不得兼作更新入口。
        VotingIdentityAlreadyExists,
        /// 调用声明的身份版本不是该 CID 当前身份版本；旧签名重放与状态回滚在此拒死。
        IdentityVersionMismatch,
        /// CID 与入参账户不符合当前双向绑定。
        CidAccountIdBindingMismatch,
        /// 入参账户已经绑定另一个永久 CID。
        AccountIdAlreadyBoundToAnotherCid,
        /// 自助换绑:该 CID 当前未绑定任何账户(号未占或已吊销)。
        NotBoundToAnyCid,
        /// 自助换绑:该 CID 已升级投票/竞选公民,账户变更只能经注册局。
        CivicRebindRequiresRegistrar,
        /// 换绑目标与当前绑定账户相同；绑定 revision 只记录真实变更。
        RebindAccountIdUnchanged,
        /// 调用声明的绑定 revision 不是该 CID 当前 revision。
        BindingRevisionMismatch,
        /// 绑定 revision 不存在，说明 CID 绑定状态不完整。
        BindingRevisionMissing,
        /// 绑定 revision 已达上限，禁止静默饱和后继续换绑或吊销。
        BindingRevisionOverflow,
        /// 换绑授权已过期，或 expires_at 不晚于当前链上时间。
        CidAuthorizationExpired,
        /// CID 首次绑定或换绑授权有效期超过 600 秒。
        CidAuthorizationLifetimeTooLong,
        /// 自助换绑:当前绑定账户的授权签名无效。
        InvalidRebindSignature,
        /// 注册局代换绑:新账户的控制证明签名无效。
        InvalidAdminRebindSignature,
        CidNotFound,
        VotingIdentityNotFound,
        CidAlreadyOccupied,
        CidNotOccupied,
        CidAlreadyRevoked,
        /// 身份资格修订号达到 u64 上限。
        EligibilityRevisionOverflow,
        /// 单个永久 CID 的身份历史版本数达到 u64 上限。
        EligibilityVersionOverflow,
        /// 四级人口尚未完整推进到当前 UTC+8 日期。
        PopulationDataNotReady,
        /// 四级人口维护已经进入故障状态。
        PopulationMaintenanceFaulted,
        /// 指定日期的转换项顺序号达到 u64 上限。
        PopulationTransitionOverflow,
        /// 四级人口计数加法溢出。
        PopulationCounterOverflow,
        /// 四级人口计数减法下溢，说明人口不变量已经损坏。
        PopulationCounterUnderflow,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Timestamp inherent 已在 `on_idle` 前写入；人口日期只在剩余权重内有界推进。
        fn on_idle(
            _n: BlockNumberFor<T>,
            remaining_weight: frame_support::weights::Weight,
        ) -> frame_support::weights::Weight {
            Self::process_population_maintenance(remaining_weight)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(
            T::WeightInfo::register_voting_identity()
                .saturating_add(T::OnVotingIdentityRegistered::on_voting_identity_registered_weight())
        )]
        pub fn register_voting_identity(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            payload: VotingIdentityPayload<T::AccountId>,
            expected_identity_version: u64,
            expires_at: u64,
            citizen_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            Self::ensure_valid_voting_payload(&payload)?;
            Self::ensure_authorized(
                &registrar,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                payload.residence_province_code.as_slice(),
                payload.residence_city_code.as_slice(),
                CitizenIdentityLevel::Voting,
                0,
            )?;
            Self::ensure_identity_authorization(
                &payload.cid_number,
                expected_identity_version,
                expires_at,
            )?;
            Self::ensure_citizen_signature(
                &payload.account_id,
                &Self::citizen_identity_authorization(
                    payload.clone(),
                    expected_identity_version,
                    expires_at,
                )
                .encode(),
                &citizen_signature,
            )?;
            Self::ensure_cid_occupied_active(&payload.cid_number)?;
            ensure!(
                !VotingIdentityByCid::<T>::contains_key(&payload.cid_number),
                Error::<T>::VotingIdentityAlreadyExists
            );
            // 账户已在占号阶段绑定(占即绑);登记只做纯升级,校验现有绑定不重绑。
            Self::ensure_current_account_id_binding(&payload.cid_number, &payload.account_id)?;

            let identity = Self::identity_from_payload(&payload);
            Self::replace_voting_identity(payload.cid_number.clone(), identity, None)?;
            T::OnVotingIdentityRegistered::on_voting_identity_registered(
                &payload.account_id,
                &payload.cid_number,
            );
            Self::deposit_event(Event::<T>::VotingIdentityRegistered {
                account_id: payload.account_id,
                cid_number: payload.cid_number,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(
            <T as Config>::WeightInfo::upgrade_to_candidate_identity()
                .saturating_add(T::OnVotingIdentityRegistered::on_voting_identity_registered_weight())
        )]
        pub fn upgrade_to_candidate_identity(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            payload: CandidateIdentityPayload<T::AccountId>,
            expected_identity_version: u64,
            expires_at: u64,
            citizen_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            Self::ensure_valid_candidate_payload(&payload)?;
            Self::ensure_authorized(
                &registrar,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                payload.voting.residence_province_code.as_slice(),
                payload.voting.residence_city_code.as_slice(),
                CitizenIdentityLevel::Candidate,
                1,
            )?;
            Self::ensure_identity_authorization(
                &payload.voting.cid_number,
                expected_identity_version,
                expires_at,
            )?;
            Self::ensure_citizen_signature(
                &payload.voting.account_id,
                &Self::citizen_identity_authorization(
                    payload.clone(),
                    expected_identity_version,
                    expires_at,
                )
                .encode(),
                &citizen_signature,
            )?;
            Self::ensure_cid_occupied_active(&payload.voting.cid_number)?;
            // 账户已在占号阶段绑定(占即绑);升级只做纯升级,校验现有绑定不重绑。
            Self::ensure_current_account_id_binding(
                &payload.voting.cid_number,
                &payload.voting.account_id,
            )?;

            Self::ensure_candidate_locked_immutable(&payload.voting.cid_number, &payload)?;

            let old = VotingIdentityByCid::<T>::get(&payload.voting.cid_number);
            // 竞选身份可作为公民首次上链(第 3 条:不强制先投票)。首次即建投票身份时,
            // 与投票身份首次登记同权,触发一次性公民认证发行;已有投票身份的升级不再触发。
            let is_first_onchain_identity = old.is_none();
            let identity = Self::identity_from_payload(&payload.voting);
            Self::replace_voting_identity(payload.voting.cid_number.clone(), identity, old)?;
            if is_first_onchain_identity {
                T::OnVotingIdentityRegistered::on_voting_identity_registered(
                    &payload.voting.account_id,
                    &payload.voting.cid_number,
                );
            }
            CandidateIdentityByCid::<T>::insert(
                &payload.voting.cid_number,
                CandidateIdentity {
                    birth_province_code: payload.birth_province_code,
                    birth_city_code: payload.birth_city_code,
                    birth_town_code: payload.birth_town_code,
                    family_name: payload.family_name,
                    given_name: payload.given_name,
                    citizen_sex: payload.citizen_sex,
                    birth_date: payload.birth_date,
                    updated_at: frame_system::Pallet::<T>::block_number(),
                },
            );
            Self::deposit_event(Event::<T>::CandidateIdentityUpgraded {
                account_id: payload.voting.account_id,
                cid_number: payload.voting.cid_number,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::update_voting_identity())]
        pub fn update_voting_identity(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            payload: VotingIdentityPayload<T::AccountId>,
            expected_identity_version: u64,
            expires_at: u64,
            citizen_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            Self::ensure_valid_voting_payload(&payload)?;
            Self::ensure_authorized(
                &registrar,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                payload.residence_province_code.as_slice(),
                payload.residence_city_code.as_slice(),
                CitizenIdentityLevel::Voting,
                2,
            )?;
            Self::ensure_identity_authorization(
                &payload.cid_number,
                expected_identity_version,
                expires_at,
            )?;
            Self::ensure_citizen_signature(
                &payload.account_id,
                &Self::citizen_identity_authorization(
                    payload.clone(),
                    expected_identity_version,
                    expires_at,
                )
                .encode(),
                &citizen_signature,
            )?;
            Self::ensure_cid_occupied_active(&payload.cid_number)?;
            Self::ensure_current_account_id_binding(&payload.cid_number, &payload.account_id)?;

            let old = VotingIdentityByCid::<T>::get(&payload.cid_number)
                .ok_or(Error::<T>::VotingIdentityNotFound)?;
            let identity = Self::identity_from_payload(&payload);
            Self::replace_voting_identity(payload.cid_number.clone(), identity, Some(old))?;
            Self::deposit_event(Event::<T>::VotingIdentityUpdated {
                account_id: payload.account_id,
                cid_number: payload.cid_number,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::update_candidate_identity())]
        pub fn update_candidate_identity(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            payload: CandidateIdentityPayload<T::AccountId>,
            expected_identity_version: u64,
            expires_at: u64,
            citizen_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            Self::ensure_valid_candidate_payload(&payload)?;
            Self::ensure_authorized(
                &registrar,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                payload.voting.residence_province_code.as_slice(),
                payload.voting.residence_city_code.as_slice(),
                CitizenIdentityLevel::Candidate,
                3,
            )?;
            Self::ensure_identity_authorization(
                &payload.voting.cid_number,
                expected_identity_version,
                expires_at,
            )?;
            Self::ensure_citizen_signature(
                &payload.voting.account_id,
                &Self::citizen_identity_authorization(
                    payload.clone(),
                    expected_identity_version,
                    expires_at,
                )
                .encode(),
                &citizen_signature,
            )?;
            Self::ensure_cid_occupied_active(&payload.voting.cid_number)?;
            Self::ensure_current_account_id_binding(
                &payload.voting.cid_number,
                &payload.voting.account_id,
            )?;

            Self::ensure_candidate_locked_immutable(&payload.voting.cid_number, &payload)?;

            let old = VotingIdentityByCid::<T>::get(&payload.voting.cid_number)
                .ok_or(Error::<T>::VotingIdentityNotFound)?;
            let identity = Self::identity_from_payload(&payload.voting);
            Self::replace_voting_identity(payload.voting.cid_number.clone(), identity, Some(old))?;
            CandidateIdentityByCid::<T>::insert(
                &payload.voting.cid_number,
                CandidateIdentity {
                    birth_province_code: payload.birth_province_code,
                    birth_city_code: payload.birth_city_code,
                    birth_town_code: payload.birth_town_code,
                    family_name: payload.family_name,
                    given_name: payload.given_name,
                    citizen_sex: payload.citizen_sex,
                    birth_date: payload.birth_date,
                    updated_at: frame_system::Pallet::<T>::block_number(),
                },
            );
            Self::deposit_event(Event::<T>::CandidateIdentityUpdated {
                account_id: payload.voting.account_id,
                cid_number: payload.voting.cid_number,
            });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::revoke_identity())]
        pub fn revoke_identity(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            cid_number: CidNumberBound,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
            // 重复吊销必须在任何身份、人口或 revision 写入前拒绝，避免已撤销
            // CID 再次推进绑定轮次。
            Self::ensure_cid_occupied_active(&cid_number)?;
            let account = AccountIdByCid::<T>::get(&cid_number).ok_or(Error::<T>::CidNotFound)?;
            let old = VotingIdentityByCid::<T>::get(&cid_number)
                .ok_or(Error::<T>::VotingIdentityNotFound)?;
            Self::ensure_authorized(
                &registrar,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                old.residence_province_code.as_slice(),
                old.residence_city_code.as_slice(),
                CitizenIdentityLevel::Voting,
                4,
            )?;

            // 在任何身份/人口 storage 写入前先验证 revision 可推进，避免失败留下半状态。
            let binding_revision = Self::next_binding_revision(&cid_number)?;
            let mut revoked = old.clone();
            revoked.citizen_status = CitizenStatus::Revoked;
            revoked.updated_at = frame_system::Pallet::<T>::block_number();
            Self::replace_voting_identity(cid_number.clone(), revoked, Some(old))?;
            CandidateIdentityByCid::<T>::remove(&cid_number);
            // 身份吊销联动登记表墓碑并推进绑定 revision，旧轮次授权随即永久失效。
            Self::tombstone_cid_record(&cid_number, binding_revision);
            Self::deposit_event(Event::<T>::CitizenIdentityRevoked {
                account_id: account,
                cid_number,
                binding_revision,
            });
            Ok(())
        }

        /// 公民自助占号(匿名):本人一笔自签交易占一个 CN 前缀的匿名身份 CID + 绑本账户。
        /// 类型限 CTZN(公民)/ NATP(居民),用户自选(q3);投票/竞选公民只能经注册局线下
        /// 注册(q1)。不写公民档、无注册局;签名人自付最低链上费(fee_route 走
        /// signer_onchain_route)。commitment 由链上从 origin 计算(= `blake2_256(account_id
        /// 公钥)`,D2 锚定基;换绑授权来自当前双向绑定而非本值)。复用已退役的 call_index(5)。
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::self_occupy_cid())]
        pub fn self_occupy_cid(origin: OriginFor<T>, cid_number: CidNumberBound) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            // 自助只出匿名身份,类型限 CTZN(公民)/ NATP(居民),用户自选。
            Self::ensure_valid_self_registrable_cid(&cid_number)?;
            // commitment 链上算:同一账户重放得同值,do_occupy_cid 幂等续用。
            let commitment = blake2_256(&account_id.encode());
            // 一账户一 CID + CID 未被他人绑(复用双向闭环校验)。
            Self::ensure_account_id_binding_available(&cid_number, &account_id)?;
            // 占号:SELF sentinel 作 registrar、居住地空(匿名去地域);
            // 格式 + CTZN|NATP + 全局原子查重仲裁在 do_occupy_cid 内。
            let self_registrar = CidNumberBound::truncate_from(SELF_OCCUPY_REGISTRAR.to_vec());
            let empty_area = AreaCodeBound::default();
            Self::do_occupy_cid(
                &self_registrar,
                &cid_number,
                &commitment,
                &empty_area,
                &empty_area,
            )?;
            // 占即绑:写双向绑定(匿名 CID = 有绑定、无 VotingIdentity)。
            let binding_revision = Self::bind_account_id(&cid_number, &account_id);
            Self::deposit_event(Event::<T>::CidSelfOccupied {
                cid_number,
                account_id,
                binding_revision,
            });
            Ok(())
        }

        /// 匿名 CID 自助换绑(轮换):把 CID 从当前绑定账户换绑到新账户,CID 与业务数据不变。
        /// origin = 新账户(自签即证新账户受控 + 自付费);`current_account_signature` = 当前绑定账户
        /// 对 [`CidRebindAuthorization`] 的 op-tag 授权签名。
        /// 当前账户从 `AccountIdByCid[cid]` 反查、不用传;已升级投票/竞选公民的 CID 只能经注册局
        /// 换绑。CTZN 匿名 + NATP 均可自助换绑。新账户任意(能签即可),但不得已绑另一 CID。
        #[pallet::call_index(9)]
        #[pallet::weight(<T as Config>::WeightInfo::self_rebind_cid_account_id())]
        pub fn self_rebind_cid_account_id(
            origin: OriginFor<T>,
            cid_number: CidNumberBound,
            expected_binding_revision: u64,
            expires_at: u64,
            current_account_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let new_account_id = ensure_signed(origin)?;
            // 当前账户 = 该 CID 当前绑定账户(不存在=号未占/已吊销 → 拒)，以链上绑定为准。
            let current_account_id =
                AccountIdByCid::<T>::get(&cid_number).ok_or(Error::<T>::NotBoundToAnyCid)?;
            Self::ensure_cid_occupied_active(&cid_number)?;
            // 匿名限定(q1):已升级投票/竞选公民只能经注册局换绑。
            ensure!(
                !VotingIdentityByCid::<T>::contains_key(&cid_number),
                Error::<T>::CivicRebindRequiresRegistrar
            );
            ensure!(
                current_account_id != new_account_id,
                Error::<T>::RebindAccountIdUnchanged
            );
            Self::ensure_expected_binding_revision(&cid_number, expected_binding_revision)?;
            Self::ensure_cid_authorization_window(expires_at)?;
            // 创世哈希 + 当前账户 + revision + 过期时间共同锁死本次授权，禁止跨链/跨轮次重放。
            let payload = Self::rebind_authorization(
                cid_number.clone(),
                current_account_id.clone(),
                new_account_id.clone(),
                expected_binding_revision,
                expires_at,
            )
            .encode();
            Self::ensure_rebind_signature(
                &current_account_id,
                &payload,
                &current_account_signature,
            )?;
            // 新账户任意,但不得已绑另一个 CID(一账户一 CID)。
            if let Some(existing) = CidByAccountId::<T>::get(&new_account_id) {
                ensure!(
                    existing == cid_number,
                    Error::<T>::AccountIdAlreadyBoundToAnotherCid
                );
            }
            // 换绑:删旧反向索引、写新双向绑定。
            let binding_revision = Self::rebind_account_id(
                &cid_number,
                &current_account_id,
                &new_account_id,
                expected_binding_revision,
            )?;
            Self::deposit_event(Event::<T>::CidAccountIdRebound {
                cid_number,
                previous_account_id: current_account_id,
                new_account_id,
                binding_revision,
            });
            Ok(())
        }

        /// 注册局代 CID 换绑：由有权限注册局把永久 CID 换绑到新账户；无论当前钱包
        /// 是否可用，本入口都不要求当前账户签名，也不引入换绑投票。
        ///
        /// 匿名 CID 由任一在册 CREG/FRG 办理；实名 CID 必须由本市 CREG 或对应省 FRG
        /// 依其投票身份居住地作用域办理。新账户对 [`CidRebindAuthorization`] 使用
        /// `OP_SIGN_CID_ADMIN_REBIND` 签名，证明新账户受控。费用由注册局机构承担。
        /// 复用删批量占号释放出的 `call_index(7)`。
        #[pallet::call_index(7)]
        #[pallet::weight(<T as Config>::WeightInfo::admin_rebind_cid_account_id())]
        // 8 个参数逐字节镜像 CidRebindAuthorization 签名载荷,属链上编码契约。
        // citizenwallet payload_decoder 逐字段解码并中文展示(两色识别要求
        // expected_binding_revision / expires_at / new_account_signature 三个防重放
        // 字段单独可见),合并成结构体会同时改 SCALE 编码和破坏该展示,故只放宽本函数。
        #[allow(clippy::too_many_arguments)]
        pub fn admin_rebind_cid_account_id(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            cid_number: CidNumberBound,
            new_account_id: T::AccountId,
            expected_binding_revision: u64,
            expires_at: u64,
            new_account_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            // 当前账户 = 该 CID 当前绑定账户(不存在=号未占/已吊销 → 拒)。
            let current_account_id =
                AccountIdByCid::<T>::get(&cid_number).ok_or(Error::<T>::NotBoundToAnyCid)?;
            Self::ensure_cid_occupied_active(&cid_number)?;
            ensure!(
                current_account_id != new_account_id,
                Error::<T>::RebindAccountIdUnchanged
            );
            Self::ensure_expected_binding_revision(&cid_number, expected_binding_revision)?;
            Self::ensure_cid_authorization_window(expires_at)?;
            // 实名按居住地作用域，匿名按全国注册局权限；两条路径共用 action_code 7。
            let authorized = match VotingIdentityByCid::<T>::get(&cid_number) {
                Some(identity) => T::CitizenIdentityAuthority::can_manage_voting_identity(
                    &registrar,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    identity.residence_province_code.as_slice(),
                    identity.residence_city_code.as_slice(),
                    CitizenIdentityLevel::Voting,
                    7,
                ),
                None => T::CitizenIdentityAuthority::can_manage_anonymous_cid(
                    &registrar,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    7,
                ),
            };
            ensure!(authorized, Error::<T>::UnauthorizedRegistrar);
            // 创世哈希 + 当前账户 + revision + 过期时间共同锁死新账户控制证明。
            let payload = Self::rebind_authorization(
                cid_number.clone(),
                current_account_id.clone(),
                new_account_id.clone(),
                expected_binding_revision,
                expires_at,
            )
            .encode();
            Self::ensure_admin_rebind_signature(&new_account_id, &payload, &new_account_signature)?;
            // 新账户任意,但不得已绑另一个 CID(一账户一 CID)。
            if let Some(existing) = CidByAccountId::<T>::get(&new_account_id) {
                ensure!(
                    existing == cid_number,
                    Error::<T>::AccountIdAlreadyBoundToAnotherCid
                );
            }
            // 换绑:删旧反向索引、写新双向绑定并单调推进 revision。
            let binding_revision = Self::rebind_account_id(
                &cid_number,
                &current_account_id,
                &new_account_id,
                expected_binding_revision,
            )?;
            Self::deposit_event(Event::<T>::CidAccountIdRebound {
                cid_number,
                previous_account_id: current_account_id,
                new_account_id,
                binding_revision,
            });
            Ok(())
        }

        /// 注册局占号(占即绑,匿名):管理员一笔冷签为用户占一个 CN 前缀的匿名身份 CID
        /// 并当场绑定用户钱包账户。类型限 CTZN(公民)/ NATP(居民);CID 为全国号、无省市
        /// 归属,任一在册注册局(CREG/FRG)均可办(`can_manage_anonymous_cid`,不做辖区匹配)。
        /// 用户对 [`CidOccupyAuthorization`] 的签名(域 `OP_SIGN_CID_OCCUPY`)证明账户受控;
        /// commitment 链上算 `blake2_256(account_id 公钥)`(与自助占号同基);不写 VotingIdentity
        /// (匿名),居住/姓名等档案改由 onchina 链下 `citizens` 表选填保存。
        #[pallet::call_index(6)]
        #[pallet::weight(<T as Config>::WeightInfo::occupy_cid())]
        pub fn occupy_cid(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            cid_number: CidNumberBound,
            account_id: T::AccountId,
            expires_at: u64,
            citizen_signature: SignatureOf<T>,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            // 类型:公民 CTZN 或 居民 NATP(智能人 SMTP 与机构码拒)。
            Self::ensure_valid_self_registrable_cid(&cid_number)?;
            // 鉴权:任一在册注册局持 citizen-identity 管理权即可,不做辖区匹配(全国号)。
            ensure!(
                T::CitizenIdentityAuthority::can_manage_anonymous_cid(
                    &registrar,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    6,
                ),
                Error::<T>::UnauthorizedRegistrar
            );
            // 注册局首次绑定只接受从未登记、从未绑定且 revision 不存在的 CID。
            ensure!(
                !CidRegistry::<T>::contains_key(&cid_number)
                    && !AccountIdByCid::<T>::contains_key(&cid_number)
                    && !BindingRevisionByCid::<T>::contains_key(&cid_number),
                Error::<T>::CidAlreadyOccupied
            );
            Self::ensure_cid_authorization_window(expires_at)?;
            // 创世哈希 + 固定 revision=0 + 过期时间共同锁死首次绑定证明。
            let payload = CidOccupyAuthorization {
                genesis_hash: frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::default()),
                cid_number: cid_number.clone(),
                account_id: account_id.clone(),
                expected_binding_revision: 0,
                expires_at,
            }
            .encode();
            Self::ensure_occupy_signature(&account_id, &payload, &citizen_signature)?;
            // 一账户一 CID + CID 未被他人绑(复用双向闭环校验)。
            Self::ensure_account_id_binding_available(&cid_number, &account_id)?;
            // 占号:actor_cid 作 registrar、居住地空(匿名去地域);
            // commitment = blake2_256(account_id),格式+类型+全局原子查重在 do_occupy_cid 内。
            let commitment = blake2_256(&account_id.encode());
            let empty_area = AreaCodeBound::default();
            Self::do_occupy_cid(
                &actor_cid_number,
                &cid_number,
                &commitment,
                &empty_area,
                &empty_area,
            )?;
            // 占即绑:写双向绑定(匿名 CID = 有绑定、无 VotingIdentity)。
            Self::bind_account_id(&cid_number, &account_id);
            Ok(())
        }

        /// 吊销:登记表墓碑(Active→Revoked,永不复用);号已绑定链上身份
        /// 则联动置 Revoked。已升级投票/竞选公民按其登记居住地作用域授权(防跨域吊销);
        /// 匿名 CID(无投票身份、全国号无居住地)任一在册注册局可吊销。
        #[pallet::call_index(8)]
        #[pallet::weight(<T as Config>::WeightInfo::revoke_cid())]
        pub fn revoke_cid(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberBound,
            actor_role_code: RoleCodeBound,
            cid_number: CidNumberBound,
        ) -> DispatchResult {
            let registrar = ensure_signed(origin)?;
            let rec = CidRegistry::<T>::get(&cid_number).ok_or(Error::<T>::CidNotOccupied)?;
            ensure!(
                rec.status == CidRecordStatus::Active,
                Error::<T>::CidAlreadyRevoked
            );
            // 居住地作用域取自投票身份(占号阶段已去地域,CidRecord 不再存居住地)。
            let authorized = match VotingIdentityByCid::<T>::get(&cid_number) {
                Some(identity) => T::CitizenIdentityAuthority::can_manage_voting_identity(
                    &registrar,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    identity.residence_province_code.as_slice(),
                    identity.residence_city_code.as_slice(),
                    CitizenIdentityLevel::Voting,
                    8,
                ),
                None => T::CitizenIdentityAuthority::can_manage_anonymous_cid(
                    &registrar,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    8,
                ),
            };
            ensure!(authorized, Error::<T>::UnauthorizedRegistrar);
            // 在身份/人口 storage 发生任何写入前验证 revision 可推进。
            let binding_revision = Self::next_binding_revision(&cid_number)?;
            if AccountIdByCid::<T>::contains_key(&cid_number) {
                Self::revoke_bound_identity(&cid_number)?;
            }
            Self::tombstone_cid_record(&cid_number, binding_revision);
            Self::deposit_event(Event::<T>::CidRevoked {
                cid_number,
                binding_revision,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// 公民 CID 号全量校验(段结构+机构码+盈利位+校验和)单源
        /// primitives::cid,且机构码必须是公民人 CTZN。
        fn ensure_valid_citizen_cid(cid_number: &CidNumberBound) -> DispatchResult {
            ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
            let parts =
                primitives::cid::number::parse_cid_number_parts_bytes(cid_number.as_slice())
                    .map_err(|_| Error::<T>::InvalidCitizenCode)?;
            ensure!(
                parts.institution == *b"CTZN",
                Error::<T>::InvalidCitizenCode
            );
            Ok(())
        }

        /// 自助占号可注册的匿名身份类型:公民 CTZN 或 居民 NATP,用户自选(q3)。
        /// 投票/竞选公民只能经注册局线下注册(q1),自助只出匿名 CID(智能人 SMTP 与机构码均拒)。
        fn ensure_valid_self_registrable_cid(cid_number: &CidNumberBound) -> DispatchResult {
            ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
            let parts =
                primitives::cid::number::parse_cid_number_parts_bytes(cid_number.as_slice())
                    .map_err(|_| Error::<T>::InvalidCitizenCode)?;
            ensure!(
                parts.institution == *b"CTZN" || parts.institution == *b"NATP",
                Error::<T>::InvalidCitizenCode
            );
            Ok(())
        }

        /// 身份写入前置:CID 必须已占号且未吊销(占号先行铁律)。
        fn ensure_cid_occupied_active(cid_number: &CidNumberBound) -> DispatchResult {
            match CidRegistry::<T>::get(cid_number) {
                Some(rec) if rec.status == CidRecordStatus::Active => Ok(()),
                Some(_) => Err(Error::<T>::CidAlreadyRevoked.into()),
                None => Err(Error::<T>::CidNotOccupied.into()),
            }
        }

        /// 登记表墓碑:Active → Revoked，并严格推进绑定 revision。
        ///
        /// 调用方必须已完成 Active 状态及 revision 可推进的全部预检；本 helper
        /// 在首个写入之后保持不可失败，确保身份、人口、登记表和 revision 原子落地。
        fn tombstone_cid_record(cid_number: &CidNumberBound, binding_revision: u64) {
            CidRegistry::<T>::mutate(cid_number, |record| {
                if let Some(record) = record {
                    debug_assert_eq!(record.status, CidRecordStatus::Active);
                    record.status = CidRecordStatus::Revoked;
                    record.revoked_at = Some(frame_system::Pallet::<T>::block_number());
                    // 只在真的把一条 Active 记录改成墓碑时减 1；helper 契约要求首个写入
                    // 之后不可失败，故用 saturating_sub，不做断言也不返回错误。
                    CidCount::<T>::mutate(|count| *count = count.saturating_sub(1));
                }
            });
            BindingRevisionByCid::<T>::insert(cid_number, binding_revision);
        }

        /// 占号核心:链上原子「验格式+查重+登记」。
        /// 同注册局+同承诺哈希的重复提交幂等放行(建档落库失败恢复路径)。
        fn do_occupy_cid(
            registrar_cid_number: &CidNumberBound,
            cid_number: &CidNumberBound,
            commitment: &[u8; 32],
            residence_province_code: &AreaCodeBound,
            residence_city_code: &AreaCodeBound,
        ) -> DispatchResult {
            // CID 号类型校验由各调用方负责；注册局与自助入口均只允许 CTZN|NATP。
            match CidRegistry::<T>::get(cid_number) {
                None => {
                    CidRegistry::<T>::insert(
                        cid_number,
                        CidRecord {
                            registrar_cid_number: registrar_cid_number.clone(),
                            commitment: *commitment,
                            residence_province_code: residence_province_code.clone(),
                            residence_city_code: residence_city_code.clone(),
                            status: CidRecordStatus::Active,
                            registered_at: frame_system::Pallet::<T>::block_number(),
                            revoked_at: None,
                        },
                    );
                    // 只有真正新登记才加 1；下面的幂等重入分支不写库，也绝不能计数。
                    CidCount::<T>::mutate(|count| *count = count.saturating_add(1));
                    Self::deposit_event(Event::<T>::CidOccupied {
                        cid_number: cid_number.clone(),
                        registrar_cid_number: registrar_cid_number.clone(),
                        binding_revision: 1,
                    });
                    Ok(())
                }
                Some(rec)
                    if rec.status == CidRecordStatus::Active
                        && rec.registrar_cid_number == *registrar_cid_number
                        && rec.commitment == *commitment =>
                {
                    Ok(())
                }
                Some(_) => Err(Error::<T>::CidAlreadyOccupied.into()),
            }
        }

        /// 吊销已绑定的链上身份:状态置 Revoked、退出人口分母、移除参选档案。
        fn revoke_bound_identity(cid_number: &CidNumberBound) -> DispatchResult {
            if let Some(old) = VotingIdentityByCid::<T>::get(cid_number) {
                if old.citizen_status != CitizenStatus::Revoked {
                    let mut revoked = old.clone();
                    revoked.citizen_status = CitizenStatus::Revoked;
                    revoked.updated_at = frame_system::Pallet::<T>::block_number();
                    Self::replace_voting_identity(cid_number.clone(), revoked, Some(old))?;
                    CandidateIdentityByCid::<T>::remove(cid_number);
                }
            }
            Ok(())
        }

        fn ensure_valid_voting_payload(
            payload: &VotingIdentityPayload<T::AccountId>,
        ) -> DispatchResult {
            Self::ensure_valid_citizen_cid(&payload.cid_number)?;
            ensure!(
                !payload.residence_province_code.is_empty()
                    && !payload.residence_city_code.is_empty()
                    && !payload.residence_town_code.is_empty(),
                Error::<T>::EmptyResidenceScope
            );
            ensure!(
                Self::is_plausible_yyyymmdd(payload.passport_valid_from)
                    && Self::is_plausible_yyyymmdd(payload.passport_valid_until)
                    && payload.passport_valid_from <= payload.passport_valid_until,
                Error::<T>::InvalidDateRange
            );
            // 投票身份不在链上计算或存储年龄:能否投票由 citizen_status=Normal + 护照有效期窗口
            // 判定,最小年龄门只在竞选身份按 birth_date 实时校验(见 ensure_valid_candidate_payload)。
            Ok(())
        }

        fn ensure_valid_candidate_payload(
            payload: &CandidateIdentityPayload<T::AccountId>,
        ) -> DispatchResult {
            Self::ensure_valid_voting_payload(&payload.voting)?;
            ensure!(
                !payload.birth_province_code.is_empty()
                    && !payload.birth_city_code.is_empty()
                    && !payload.birth_town_code.is_empty(),
                Error::<T>::EmptyBirthScope
            );
            ensure!(!payload.family_name.is_empty(), Error::<T>::EmptyFamilyName);
            ensure!(!payload.given_name.is_empty(), Error::<T>::EmptyGivenName);
            ensure!(
                Self::is_plausible_yyyymmdd(payload.birth_date),
                Error::<T>::InvalidBirthDate
            );
            // 出生日期决定竞选公民年龄:必须能算出年龄且不低于法定最小年龄。
            let age = Self::age_from_birth_date(payload.birth_date)
                .ok_or(Error::<T>::InvalidBirthDate)?;
            ensure!(
                age >= MIN_ONCHAIN_CITIZEN_AGE_YEARS as u32,
                Error::<T>::UnderCandidateAge
            );
            Ok(())
        }

        /// 校验注册局对该居住地、该身份档次的管辖权。
        fn ensure_authorized(
            registrar: &T::AccountId,
            actor_cid_number: &[u8],
            actor_role_code: &[u8],
            residence_province_code: &[u8],
            residence_city_code: &[u8],
            level: CitizenIdentityLevel,
            action_code: u32,
        ) -> DispatchResult {
            ensure!(
                T::CitizenIdentityAuthority::can_manage_voting_identity(
                    registrar,
                    actor_cid_number,
                    actor_role_code,
                    residence_province_code,
                    residence_city_code,
                    level,
                    action_code,
                ),
                Error::<T>::UnauthorizedRegistrar
            );
            Ok(())
        }

        /// 校验调用声明的身份版本等于该 CID 链上当前身份版本。
        ///
        /// 版本由 [`VotingEligibilityVersionCount`] 承载，每次身份写入单调 +1，尚无身份时为 0。
        fn ensure_expected_identity_version(
            cid_number: &CidNumberBound,
            expected_identity_version: u64,
        ) -> DispatchResult {
            ensure!(
                VotingEligibilityVersionCount::<T>::get(cid_number) == expected_identity_version,
                Error::<T>::IdentityVersionMismatch
            );
            Ok(())
        }

        /// 构造四端共用的公民身份写入授权；字段声明顺序即 SCALE 协议顺序。
        fn citizen_identity_authorization<Payload>(
            payload: Payload,
            expected_identity_version: u64,
            expires_at: u64,
        ) -> CitizenIdentityAuthorization<T::Hash, Payload> {
            CitizenIdentityAuthorization {
                genesis_hash: frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::default()),
                payload,
                expected_identity_version,
                expires_at,
            }
        }

        /// 身份写入四入口共用的防重放前置；必须在验签之前调用。
        fn ensure_identity_authorization(
            cid_number: &CidNumberBound,
            expected_identity_version: u64,
            expires_at: u64,
        ) -> DispatchResult {
            Self::ensure_cid_authorization_window(expires_at)?;
            Self::ensure_expected_identity_version(cid_number, expected_identity_version)
        }

        fn ensure_citizen_signature(
            account_id: &T::AccountId,
            payload: &[u8],
            signature: &SignatureOf<T>,
        ) -> DispatchResult {
            ensure!(
                T::CitizenIdentityAuthority::verify_citizen_signature(
                    account_id, payload, signature,
                ),
                Error::<T>::InvalidCitizenSignature
            );
            Ok(())
        }

        /// 校验当前绑定账户对匿名 CID 自助换绑的授权签名(op_tag OP_SIGN_CID_REBIND)。
        fn ensure_rebind_signature(
            account_id: &T::AccountId,
            payload: &[u8],
            signature: &SignatureOf<T>,
        ) -> DispatchResult {
            ensure!(
                T::CitizenIdentityAuthority::verify_rebind_signature(
                    account_id, payload, signature,
                ),
                Error::<T>::InvalidRebindSignature
            );
            Ok(())
        }

        /// 校验用户对注册局首次绑定 [`CidOccupyAuthorization`] 的授权签名
        /// (op_tag OP_SIGN_CID_OCCUPY)。
        fn ensure_occupy_signature(
            account_id: &T::AccountId,
            payload: &[u8],
            signature: &SignatureOf<T>,
        ) -> DispatchResult {
            ensure!(
                T::CitizenIdentityAuthority::verify_occupy_signature(
                    account_id, payload, signature,
                ),
                Error::<T>::InvalidOccupySignature
            );
            Ok(())
        }

        /// 校验注册局代换绑时新账户的控制证明签名(op_tag OP_SIGN_CID_ADMIN_REBIND)。
        fn ensure_admin_rebind_signature(
            account_id: &T::AccountId,
            payload: &[u8],
            signature: &SignatureOf<T>,
        ) -> DispatchResult {
            ensure!(
                T::CitizenIdentityAuthority::verify_admin_rebind_signature(
                    account_id, payload, signature,
                ),
                Error::<T>::InvalidAdminRebindSignature
            );
            Ok(())
        }

        /// 初次登记或候选升级时校验 CID↔账户双向绑定没有指向另一主体。
        fn ensure_account_id_binding_available(
            cid_number: &CidNumberBound,
            account: &T::AccountId,
        ) -> DispatchResult {
            if let Some(existing) = AccountIdByCid::<T>::get(cid_number) {
                ensure!(
                    existing == *account,
                    Error::<T>::CidAccountIdBindingMismatch
                );
            }
            if let Some(existing) = CidByAccountId::<T>::get(account) {
                ensure!(
                    existing == *cid_number,
                    Error::<T>::AccountIdAlreadyBoundToAnotherCid
                );
            }
            Ok(())
        }

        /// 身份资料更新只能使用该永久 CID 当前绑定的账户；CID 主键和账户绑定都不属于本入口可变字段。
        fn ensure_current_account_id_binding(
            cid_number: &CidNumberBound,
            account: &T::AccountId,
        ) -> DispatchResult {
            ensure!(
                AccountIdByCid::<T>::get(cid_number).as_ref() == Some(account)
                    && CidByAccountId::<T>::get(account).as_ref() == Some(cid_number),
                Error::<T>::CidAccountIdBindingMismatch
            );
            Ok(())
        }

        fn bind_account_id(cid_number: &CidNumberBound, account: &T::AccountId) -> u64 {
            AccountIdByCid::<T>::insert(cid_number, account);
            CidByAccountId::<T>::insert(account, cid_number);
            if let Some(binding_revision) = BindingRevisionByCid::<T>::get(cid_number) {
                return binding_revision;
            }
            BindingRevisionByCid::<T>::insert(cid_number, 1);
            1
        }

        /// 换绑:删除当前账户反向索引、写入新双向绑定并把 revision 严格推进一轮。
        fn rebind_account_id(
            cid_number: &CidNumberBound,
            current_account_id: &T::AccountId,
            new_account_id: &T::AccountId,
            expected_binding_revision: u64,
        ) -> Result<u64, DispatchError> {
            let binding_revision = expected_binding_revision
                .checked_add(1)
                .ok_or(Error::<T>::BindingRevisionOverflow)?;
            CidByAccountId::<T>::remove(current_account_id);
            AccountIdByCid::<T>::insert(cid_number, new_account_id);
            CidByAccountId::<T>::insert(new_account_id, cid_number);
            BindingRevisionByCid::<T>::insert(cid_number, binding_revision);
            Ok(binding_revision)
        }

        /// 构造四端共用换绑载荷；字段声明顺序即 SCALE 协议顺序。
        fn rebind_authorization(
            cid_number: CidNumberBound,
            current_account_id: T::AccountId,
            new_account_id: T::AccountId,
            expected_binding_revision: u64,
            expires_at: u64,
        ) -> CidRebindAuthorization<T::Hash, T::AccountId> {
            CidRebindAuthorization {
                genesis_hash: frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::default()),
                cid_number,
                current_account_id,
                new_account_id,
                expected_binding_revision,
                expires_at,
            }
        }

        fn ensure_expected_binding_revision(
            cid_number: &CidNumberBound,
            expected_binding_revision: u64,
        ) -> DispatchResult {
            let current = BindingRevisionByCid::<T>::get(cid_number)
                .ok_or(Error::<T>::BindingRevisionMissing)?;
            ensure!(
                current == expected_binding_revision,
                Error::<T>::BindingRevisionMismatch
            );
            Ok(())
        }

        fn ensure_cid_authorization_window(expires_at: u64) -> DispatchResult {
            let now = <T::TimeProvider as frame_support::traits::UnixTime>::now().as_secs();
            ensure!(expires_at > now, Error::<T>::CidAuthorizationExpired);
            ensure!(
                expires_at <= now.saturating_add(MAX_CID_AUTHORIZATION_LIFETIME_SECS),
                Error::<T>::CidAuthorizationLifetimeTooLong
            );
            Ok(())
        }

        fn next_binding_revision(cid_number: &CidNumberBound) -> Result<u64, DispatchError> {
            BindingRevisionByCid::<T>::get(cid_number)
                .ok_or(Error::<T>::BindingRevisionMissing)?
                .checked_add(1)
                .ok_or(Error::<T>::BindingRevisionOverflow.into())
        }

        fn identity_from_payload(
            payload: &VotingIdentityPayload<T::AccountId>,
        ) -> VotingIdentity<BlockNumberFor<T>> {
            VotingIdentity {
                passport_valid_from: payload.passport_valid_from,
                passport_valid_until: payload.passport_valid_until,
                citizen_status: payload.citizen_status,
                residence_province_code: payload.residence_province_code.clone(),
                residence_city_code: payload.residence_city_code.clone(),
                residence_town_code: payload.residence_town_code.clone(),
                updated_at: frame_system::Pallet::<T>::block_number(),
            }
        }

        /// 身份状态基础校验；人口分母还必须同时满足护照日期、CID 和账户绑定规则。
        fn identity_counts_as_voter(identity: &VotingIdentity<BlockNumberFor<T>>) -> bool {
            identity.citizen_status == CitizenStatus::Normal
        }

        /// 链上当前日期(UTC+8,YYYYMMDD 整数;时间戳未初始化时返回 0,fail-closed)。
        pub fn current_date_int() -> u32 {
            let secs = <T::TimeProvider as frame_support::traits::UnixTime>::now().as_secs();
            if secs == 0 {
                return 0;
            }
            let days = (secs as i64 + 8 * 3600) / 86_400;
            let (year, month, day) = crate::civil_from_days(days);
            if !(1900..=9999).contains(&year) {
                return 0;
            }
            (year as u32) * 10_000 + month * 100 + day
        }

        /// 严格校验 YYYYMMDD 公历日期（年 1900–9999，含大小月和闰年）。
        pub fn is_plausible_yyyymmdd(date: u32) -> bool {
            let year = date / 10_000;
            let month = (date / 100) % 100;
            let day = date % 100;
            if !(1900..=9999).contains(&year) || !(1..=12).contains(&month) || day == 0 {
                return false;
            }
            let leap =
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
            let days_in_month = match month {
                2 if leap => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            day <= days_in_month
        }

        /// 返回严格公历日期的下一自然日；`99991231` 没有可表示后继日。
        pub fn next_calendar_date(date: u32) -> Option<u32> {
            if !Self::is_plausible_yyyymmdd(date) || date == 99_991_231 {
                return None;
            }
            let year = date / 10_000;
            let month = (date / 100) % 100;
            let candidate = if Self::is_plausible_yyyymmdd(date.saturating_add(1)) {
                date.saturating_add(1)
            } else if month == 12 {
                year.checked_add(1)?.checked_mul(10_000)?.checked_add(101)?
            } else {
                year.checked_mul(10_000)?
                    .checked_add(month.checked_add(1)?.checked_mul(100)?)?
                    .checked_add(1)?
            };
            Self::is_plausible_yyyymmdd(candidate).then_some(candidate)
        }

        /// 由出生日期(YYYYMMDD)与链上当前日期(UTC+8)计算周岁。
        /// 整数除法自动判断今年生日是否已过;当前日期未初始化(时间戳=0)、
        /// 出生日期为 0 或落在未来一律返回 `None`(fail-closed)。
        pub fn age_from_birth_date(birth_date: u32) -> Option<u32> {
            let today = Self::current_date_int();
            if today == 0 || birth_date == 0 || birth_date > today {
                return None;
            }
            Some((today - birth_date) / 10_000)
        }

        /// 读取某当前账户所绑定永久 CID 的竞选身份并计算周岁；无有效主体返回 `None`。
        /// 出生日期是链上公开信息,任何调用方可据此实时计算竞选公民年龄。
        pub fn candidate_age(account: &T::AccountId) -> Option<u32> {
            let subject = Self::citizen_subject(account)?;
            let identity = CandidateIdentityByCid::<T>::get(subject.cid_number)?;
            Self::age_from_birth_date(identity.birth_date)
        }

        /// 竞选档锁定字段写一次即不可改(D4a):已存在竞选身份时,入参的出生日期、
        /// 出生省市镇、性别都必须与链上一致,否则拒绝(防止升级/更新时篡改)。
        fn ensure_candidate_locked_immutable(
            cid_number: &CidNumberBound,
            incoming: &CandidateIdentityPayload<T::AccountId>,
        ) -> DispatchResult {
            if let Some(existing) = CandidateIdentityByCid::<T>::get(cid_number) {
                ensure!(
                    existing.birth_date == incoming.birth_date,
                    Error::<T>::BirthDateImmutable
                );
                ensure!(
                    existing.birth_province_code == incoming.birth_province_code
                        && existing.birth_city_code == incoming.birth_city_code
                        && existing.birth_town_code == incoming.birth_town_code
                        && existing.citizen_sex == incoming.citizen_sex,
                    Error::<T>::CandidateProfileImmutable
                );
            }
            Ok(())
        }

        /// 护照有效期窗口校验:valid_from ≤ 今日 ≤ valid_until。
        /// 过期或未生效的护照不能投票;时间戳缺失时按不可投票处理。
        fn passport_window_valid(identity: &VotingIdentity<BlockNumberFor<T>>) -> bool {
            let today = Self::current_date_int();
            Self::passport_window_valid_on(identity, today)
        }

        fn passport_window_valid_on(
            identity: &VotingIdentity<BlockNumberFor<T>>,
            date: u32,
        ) -> bool {
            date != 0
                && identity.passport_valid_from <= date
                && date <= identity.passport_valid_until
        }

        /// 身份人口变更只能在四级人口已经完整推进至当前日期且没有故障时执行。
        fn ensure_population_ready() -> Result<u32, DispatchError> {
            ensure!(
                PopulationMaintenanceFault::<T>::get().is_none(),
                Error::<T>::PopulationMaintenanceFaulted
            );
            let current_date = Self::current_date_int();
            ensure!(current_date != 0, Error::<T>::PopulationDataNotReady);
            ensure!(
                PopulationReadyDate::<T>::get() == current_date,
                Error::<T>::PopulationDataNotReady
            );
            Ok(current_date)
        }

        /// 当前永久 CID 与账户必须形成唯一双向闭环。
        fn cid_account_id_binding_complete(cid_number: &CidNumberBound) -> bool {
            AccountIdByCid::<T>::get(cid_number).is_some_and(|account| {
                CidByAccountId::<T>::get(&account).as_ref() == Some(cid_number)
            })
        }

        /// 身份是否属于指定日期的四级有效人口。
        fn identity_eligible_on(identity: &VotingIdentity<BlockNumberFor<T>>, date: u32) -> bool {
            Self::identity_counts_as_voter(identity)
                && Self::passport_window_valid_on(identity, date)
        }

        fn replace_voting_identity(
            cid_number: CidNumberBound,
            next: VotingIdentity<BlockNumberFor<T>>,
            old: Option<VotingIdentity<BlockNumberFor<T>>>,
        ) -> DispatchResult {
            let ready_date = Self::ensure_population_ready()?;
            frame_support::storage::with_transaction(|| {
                match Self::do_replace_voting_identity(cid_number, next, old, ready_date) {
                    Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                    Err(err) => frame_support::storage::TransactionOutcome::Rollback(Err(err)),
                }
            })
        }

        fn do_replace_voting_identity(
            cid_number: CidNumberBound,
            next: VotingIdentity<BlockNumberFor<T>>,
            old: Option<VotingIdentity<BlockNumberFor<T>>>,
            ready_date: u32,
        ) -> DispatchResult {
            let revision = NextEligibilityRevision::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::EligibilityRevisionOverflow)?;
            let version_count = VotingEligibilityVersionCount::<T>::get(&cid_number);
            if let Some(old_identity) = old {
                if Self::identity_eligible_on(&old_identity, ready_date) {
                    Self::decrement_scope_counts(&old_identity)?;
                }
                if version_count > 0 {
                    VotingEligibilityVersions::<T>::mutate(
                        &cid_number,
                        version_count.saturating_sub(1),
                        |version| {
                            if let Some(version) = version {
                                version.valid_until_revision = Some(revision);
                            }
                        },
                    );
                }
            }
            if Self::identity_eligible_on(&next, ready_date) {
                Self::increment_scope_counts(&next)?;
            }
            let next_version_count = version_count
                .checked_add(1)
                .ok_or(Error::<T>::EligibilityVersionOverflow)?;
            VotingEligibilityVersions::<T>::insert(
                &cid_number,
                version_count,
                VotingEligibilityVersion {
                    identity: next.clone(),
                    valid_from_revision: revision,
                    valid_until_revision: None,
                },
            );
            VotingEligibilityVersionCount::<T>::insert(&cid_number, next_version_count);
            NextEligibilityRevision::<T>::put(revision);
            VotingIdentityByCid::<T>::insert(&cid_number, &next);
            Self::schedule_identity_transitions(&cid_number, &next, revision, ready_date)?;
            Ok(())
        }

        fn schedule_identity_transitions(
            cid_number: &CidNumberBound,
            identity: &VotingIdentity<BlockNumberFor<T>>,
            revision: u64,
            ready_date: u32,
        ) -> DispatchResult {
            if identity.citizen_status != CitizenStatus::Normal {
                return Ok(());
            }
            if identity.passport_valid_from > ready_date {
                Self::append_population_transition(
                    identity.passport_valid_from,
                    PopulationTransition {
                        cid_number: cid_number.clone(),
                        eligibility_revision: revision,
                        transition_kind: PopulationTransitionKind::Activate,
                    },
                )?;
            }
            if let Some(deactivate_date) = Self::next_calendar_date(identity.passport_valid_until) {
                if deactivate_date > ready_date {
                    Self::append_population_transition(
                        deactivate_date,
                        PopulationTransition {
                            cid_number: cid_number.clone(),
                            eligibility_revision: revision,
                            transition_kind: PopulationTransitionKind::Deactivate,
                        },
                    )?;
                }
            }
            Ok(())
        }

        fn append_population_transition(
            date: u32,
            transition: PopulationTransition,
        ) -> DispatchResult {
            let index = PopulationTransitionCountByDate::<T>::get(date);
            let next = index
                .checked_add(1)
                .ok_or(Error::<T>::PopulationTransitionOverflow)?;
            PopulationTransitions::<T>::insert(date, index, transition);
            PopulationTransitionCountByDate::<T>::insert(date, next);
            Ok(())
        }

        fn increment_scope_counts(identity: &VotingIdentity<BlockNumberFor<T>>) -> DispatchResult {
            Self::write_adjusted_scope_counts(identity, true).map_err(|fault| match fault {
                PopulationFault::CounterOverflow => Error::<T>::PopulationCounterOverflow.into(),
                _ => Error::<T>::PopulationCounterUnderflow.into(),
            })
        }

        fn decrement_scope_counts(identity: &VotingIdentity<BlockNumberFor<T>>) -> DispatchResult {
            Self::write_adjusted_scope_counts(identity, false).map_err(|fault| match fault {
                PopulationFault::CounterUnderflow => Error::<T>::PopulationCounterUnderflow.into(),
                _ => Error::<T>::PopulationCounterOverflow.into(),
            })
        }

        /// 先读取并验证四级结果，再一次性写入，避免中途溢出留下部分更新。
        fn write_adjusted_scope_counts(
            identity: &VotingIdentity<BlockNumberFor<T>>,
            increment: bool,
        ) -> Result<(), PopulationFault> {
            let province_key = identity.residence_province_code.clone();
            let city_key = (
                identity.residence_province_code.clone(),
                identity.residence_city_code.clone(),
            );
            let town_key = (
                identity.residence_province_code.clone(),
                identity.residence_city_code.clone(),
                identity.residence_town_code.clone(),
            );
            let adjust = |value: u64| {
                if increment {
                    value.checked_add(1).ok_or(PopulationFault::CounterOverflow)
                } else {
                    value
                        .checked_sub(1)
                        .ok_or(PopulationFault::CounterUnderflow)
                }
            };
            let country = adjust(CountryVotingCount::<T>::get())?;
            let province = adjust(ProvinceVotingCount::<T>::get(&province_key))?;
            let city = adjust(CityVotingCount::<T>::get(&city_key))?;
            let town = adjust(TownVotingCount::<T>::get(&town_key))?;
            CountryVotingCount::<T>::put(country);
            ProvinceVotingCount::<T>::insert(province_key, province);
            CityVotingCount::<T>::insert(city_key, city);
            TownVotingCount::<T>::insert(town_key, town);
            Ok(())
        }

        fn current_identity_revision(cid_number: &CidNumberBound) -> Option<u64> {
            let count = VotingEligibilityVersionCount::<T>::get(cid_number);
            if count == 0 {
                return None;
            }
            VotingEligibilityVersions::<T>::get(cid_number, count.checked_sub(1)?)
                .map(|version| version.valid_from_revision)
        }

        fn process_population_transition(
            date: u32,
            transition: &PopulationTransition,
        ) -> Result<(), PopulationFault> {
            if Self::current_identity_revision(&transition.cid_number)
                != Some(transition.eligibility_revision)
            {
                // 身份已更新或吊销，旧任务自然失效。
                return Ok(());
            }
            let identity = VotingIdentityByCid::<T>::get(&transition.cid_number)
                .ok_or(PopulationFault::MissingTransition)?;
            let active_cid = CidRegistry::<T>::get(&transition.cid_number)
                .is_some_and(|record| record.status == CidRecordStatus::Active);
            if !active_cid || !Self::cid_account_id_binding_complete(&transition.cid_number) {
                return Err(PopulationFault::MissingTransition);
            }
            match transition.transition_kind {
                PopulationTransitionKind::Activate => {
                    if identity.citizen_status == CitizenStatus::Normal
                        && identity.passport_valid_from == date
                        && Self::passport_window_valid_on(&identity, date)
                    {
                        Self::write_adjusted_scope_counts(&identity, true)?;
                    }
                }
                PopulationTransitionKind::Deactivate => {
                    if identity.citizen_status == CitizenStatus::Normal
                        && Self::next_calendar_date(identity.passport_valid_until) == Some(date)
                    {
                        Self::write_adjusted_scope_counts(&identity, false)?;
                    }
                }
            }
            Ok(())
        }

        fn record_population_fault(date: u32, fault: PopulationFault) {
            if PopulationMaintenanceFault::<T>::get().is_none() {
                PopulationMaintenanceFault::<T>::put(fault);
                Self::deposit_event(Event::<T>::PopulationMaintenanceFaulted {
                    eligibility_date: date,
                    fault,
                });
            }
        }

        /// 在当块剩余权重和独立预算内推进四级人口日期。
        pub fn process_population_maintenance(
            remaining_weight: frame_support::weights::Weight,
        ) -> frame_support::weights::Weight {
            let configured = T::MaxPopulationMaintenanceWeightPerBlock::get();
            let max_weight = frame_support::weights::Weight::from_parts(
                remaining_weight.ref_time().min(configured.ref_time()),
                remaining_weight.proof_size().min(configured.proof_size()),
            );
            let base_weight = T::WeightInfo::population_maintenance_base();
            if base_weight.any_gt(max_weight) {
                return frame_support::weights::Weight::zero();
            }
            let mut used = base_weight;
            if PopulationMaintenanceFault::<T>::get().is_some() {
                return used;
            }
            let current_date = Self::current_date_int();
            if current_date == 0 {
                return used;
            }

            let mut ready_date = PopulationReadyDate::<T>::get();
            if ready_date == 0 {
                let initialize_weight = T::WeightInfo::initialize_population_date();
                if used.saturating_add(initialize_weight).any_gt(max_weight) {
                    return used;
                }
                PopulationReadyDate::<T>::put(current_date);
                Self::deposit_event(Event::<T>::PopulationDateReady {
                    eligibility_date: current_date,
                });
                return used.saturating_add(initialize_weight);
            }
            if !Self::is_plausible_yyyymmdd(ready_date) {
                Self::record_population_fault(ready_date, PopulationFault::InvalidReadyDate);
                return used;
            }
            if ready_date > current_date {
                Self::record_population_fault(current_date, PopulationFault::DateMovedBackwards);
                return used;
            }

            let max_days = T::MaxPopulationDaysPerBlock::get();
            let max_transitions = T::MaxPopulationTransitionsPerBlock::get();
            let mut processed_days = 0u32;
            let mut processed_transitions = 0u32;
            let mut last_completed_date = None;

            'dates: while ready_date < current_date && processed_days < max_days {
                let Some(date) = Self::next_calendar_date(ready_date) else {
                    Self::record_population_fault(ready_date, PopulationFault::InvalidReadyDate);
                    break;
                };
                let day_weight = T::WeightInfo::advance_population_day();
                if used.saturating_add(day_weight).any_gt(max_weight) {
                    break;
                }
                used = used.saturating_add(day_weight);

                let transition_count = PopulationTransitionCountByDate::<T>::get(date);
                let mut cursor = PopulationTransitionCursorByDate::<T>::get(date);
                if cursor > transition_count {
                    Self::record_population_fault(date, PopulationFault::MissingTransition);
                    break;
                }
                while cursor < transition_count {
                    if processed_transitions >= max_transitions {
                        break 'dates;
                    }
                    let transition_weight = T::WeightInfo::process_population_transition();
                    if used.saturating_add(transition_weight).any_gt(max_weight) {
                        break 'dates;
                    }
                    let Some(transition) = PopulationTransitions::<T>::get(date, cursor) else {
                        Self::record_population_fault(date, PopulationFault::MissingTransition);
                        return used;
                    };
                    if let Err(fault) = Self::process_population_transition(date, &transition) {
                        Self::record_population_fault(date, fault);
                        return used.saturating_add(transition_weight);
                    }
                    PopulationTransitions::<T>::remove(date, cursor);
                    cursor = cursor.saturating_add(1);
                    PopulationTransitionCursorByDate::<T>::insert(date, cursor);
                    processed_transitions = processed_transitions.saturating_add(1);
                    used = used.saturating_add(transition_weight);
                }
                if cursor < transition_count {
                    break;
                }

                PopulationTransitionCountByDate::<T>::remove(date);
                PopulationTransitionCursorByDate::<T>::remove(date);
                PopulationReadyDate::<T>::put(date);
                ready_date = date;
                processed_days = processed_days.saturating_add(1);
                last_completed_date = Some(date);
            }

            if let Some(eligibility_date) = last_completed_date {
                Self::deposit_event(Event::<T>::PopulationDateReady { eligibility_date });
            }
            used
        }

        fn scope_matches(
            identity: &VotingIdentity<BlockNumberFor<T>>,
            scope: &PopulationScope,
        ) -> bool {
            match scope {
                PopulationScope::Country => true,
                PopulationScope::Province(p) => &identity.residence_province_code == p,
                PopulationScope::City(p, c) => {
                    &identity.residence_province_code == p && &identity.residence_city_code == c
                }
                PopulationScope::Town(p, c, t) => {
                    &identity.residence_province_code == p
                        && &identity.residence_city_code == c
                        && &identity.residence_town_code == t
                }
            }
        }

        pub fn population_count_for_scope(scope: &PopulationScope) -> u64 {
            match scope {
                PopulationScope::Country => CountryVotingCount::<T>::get(),
                PopulationScope::Province(p) => ProvinceVotingCount::<T>::get(p.clone()),
                PopulationScope::City(p, c) => CityVotingCount::<T>::get((p.clone(), c.clone())),
                PopulationScope::Town(p, c, t) => {
                    TownVotingCount::<T>::get((p.clone(), c.clone(), t.clone()))
                }
            }
        }

        /// 从身份 Storage 构造完整公民主体。
        ///
        /// 账户反向 CID、CID 正向账户、CID 身份和 CID 登记状态必须同时一致；身份或
        /// CID 已吊销、任一方向绑定缺失或错配都返回 `None`，不得退化为裸账户授权。
        pub fn citizen_subject(who: &T::AccountId) -> Option<CitizenSubject<T::AccountId>> {
            let cid_number = CidByAccountId::<T>::get(who)?;
            if AccountIdByCid::<T>::get(&cid_number).as_ref() != Some(who) {
                return None;
            }
            let identity = VotingIdentityByCid::<T>::get(&cid_number)?;
            if identity.citizen_status != CitizenStatus::Normal {
                return None;
            }
            let record = CidRegistry::<T>::get(&cid_number)?;
            if record.status != CidRecordStatus::Active {
                return None;
            }
            Some(CitizenSubject {
                cid_number,
                account_id: who.clone(),
            })
        }

        /// 返回投票引擎生成提案快照所需的同源人口数据。
        ///
        /// 本函数只读取 citizen-identity 自有的四级人口计数、资格 revision 和日期，
        /// 不创建、保存或释放任何投票快照。
        pub fn governance_population_data(scope: &PopulationScope) -> Option<PopulationData> {
            if PopulationMaintenanceFault::<T>::get().is_some() {
                return None;
            }
            let current_date = Self::current_date_int();
            if current_date == 0 || PopulationReadyDate::<T>::get() != current_date {
                return None;
            }
            Some(PopulationData {
                scope: scope.clone(),
                eligible_total: Self::population_count_for_scope(scope),
                eligibility_revision: NextEligibilityRevision::<T>::get(),
                eligibility_date: current_date,
            })
        }

        /// 按快照 revision 二分定位永久 CID 当时的身份版本。
        fn identity_at_revision(
            cid_number: &CidNumberBound,
            revision: u64,
        ) -> Option<VotingIdentity<BlockNumberFor<T>>> {
            let count = VotingEligibilityVersionCount::<T>::get(cid_number);
            if count == 0 {
                return None;
            }
            let mut low = 0u64;
            let mut high = count;
            while low < high {
                let mid = low.saturating_add(high.saturating_sub(low) / 2);
                let version = VotingEligibilityVersions::<T>::get(cid_number, mid)?;
                if version.valid_from_revision <= revision {
                    low = mid.saturating_add(1);
                } else {
                    high = mid;
                }
            }
            if low == 0 {
                return None;
            }
            let version = VotingEligibilityVersions::<T>::get(cid_number, low.saturating_sub(1))?;
            if version
                .valid_until_revision
                .map(|until| revision >= until)
                .unwrap_or(false)
            {
                return None;
            }
            Some(version.identity)
        }

        /// 使用 citizen-identity 自有历史验证账户在投票引擎快照时点是否具备资格。
        pub fn voting_subject_at_population_data(
            who: &T::AccountId,
            population_data: &PopulationData,
        ) -> Option<CitizenSubject<T::AccountId>> {
            // 历史资格跟随永久 CID；账户只负责当前交易签名和 CID 反向解析。
            let cid_number = CidByAccountId::<T>::get(who)?;
            if AccountIdByCid::<T>::get(&cid_number).as_ref() != Some(who) {
                return None;
            }
            let identity =
                Self::identity_at_revision(&cid_number, population_data.eligibility_revision)?;
            (Self::identity_counts_as_voter(&identity)
                && Self::passport_window_valid_on(&identity, population_data.eligibility_date)
                && Self::scope_matches(&identity, &population_data.scope))
            .then(|| CitizenSubject {
                cid_number,
                account_id: who.clone(),
            })
        }
    }

    impl<T: Config> crate::CitizenIdentityProvider<T::AccountId> for Pallet<T> {
        fn citizen_subject(who: &T::AccountId) -> Option<CitizenSubject<T::AccountId>> {
            Pallet::<T>::citizen_subject(who)
        }

        // 消费端全量校验:身份存在(注册时已锁定 CID↔账户一对一并验公民签名)、
        // 状态 NORMAL、护照有效期窗口内、居住地在作用域内。
        fn voting_subject(
            who: &T::AccountId,
            scope: &PopulationScope,
        ) -> Option<CitizenSubject<T::AccountId>> {
            let subject = Pallet::<T>::citizen_subject(who)?;
            VotingIdentityByCid::<T>::get(&subject.cid_number).and_then(|identity| {
                (Self::identity_counts_as_voter(&identity)
                    && Self::passport_window_valid(&identity)
                    && Self::scope_matches(&identity, scope))
                .then_some(subject)
            })
        }

        fn candidate_subject(
            who: &T::AccountId,
            scope: &PopulationScope,
        ) -> Option<CitizenSubject<T::AccountId>> {
            let subject = Self::voting_subject(who, scope)?;
            CandidateIdentityByCid::<T>::contains_key(&subject.cid_number).then_some(subject)
        }

        fn population_data(scope: &PopulationScope) -> Option<PopulationData> {
            Self::governance_population_data(scope)
        }

        fn voting_subject_at(
            who: &T::AccountId,
            population_data: &PopulationData,
        ) -> Option<CitizenSubject<T::AccountId>> {
            Self::voting_subject_at_population_data(who, population_data)
        }
    }
}

#[cfg(test)]
mod tests;
