//! 地址版本、授权上下文、真实存储和输入边界的单元测试。

use super::*;
use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32};
use sp_runtime::{BuildStorage, DispatchError};

type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;
    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type Address = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
}

pub struct Authority;
impl AddressUpdateAuthority<u64> for Authority {
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority(action_code: u32) -> Option<crate::benchmarks::Authority<u64>> {
        if action_code > 4 {
            return None;
        }
        // 与本夹具现有权限检查完全一致，基准不能扩大主体或辖区。
        Some(crate::benchmarks::Authority {
            caller: 1,
            actor_cid_number: b"registrar".to_vec().try_into().ok()?,
            actor_role_code: b"REG".to_vec().try_into().ok()?,
            province_code: b"GD".to_vec(),
            city_code: b"001".to_vec(),
            town_code: b"002".to_vec(),
        })
    }

    fn can_update_catalog(who: &u64, cid: &[u8], role: &[u8], action: u32) -> bool {
        *who == 1 && cid == b"registrar" && role == b"REG" && action == 0
    }
    fn can_update_address(
        who: &u64,
        cid: &[u8],
        role: &[u8],
        province: &[u8],
        city: &[u8],
        action: u32,
    ) -> bool {
        *who == 1
            && cid == b"registrar"
            && role == b"REG"
            && province == b"GD"
            && city == b"001"
            && (1..=4).contains(&action)
    }
}
impl Config for Test {
    type WeightInfo = ();
    type RuntimeEvent = RuntimeEvent;
    type AddressAuthority = Authority;
    type MaxCodeLen = ConstU32<3>;
    type MaxVersionLen = ConstU32<8>;
    type MaxAddressNameCodeLen = ConstU32<3>;
    type MaxAddressLocalNoLen = ConstU32<4>;
    type MaxAddressNameLen = ConstU32<16>;
    type MaxAddressDetailLen = ConstU32<16>;
}
fn ext() -> sp_io::TestExternalities {
    let mut ext = sp_io::TestExternalities::new(
        frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("测试创世存储"),
    );
    ext.execute_with(|| System::set_block_number(1));
    ext
}

// 每个真实基准只从根目录执行一次；所有权限、存储、事件断言归原基准实现。
#[cfg(feature = "runtime-benchmarks")]
mod benchmark_tests {
    use super::*;

    fn run_case(name: &[u8]) {
        ext().execute_with(|| {
            crate::benchmarks::test_case::<Test>(name).expect("真实地址基准必须通过");
        });
    }

    #[test]
    fn set_catalog_version() {
        run_case(b"set_catalog_version");
    }

    #[test]
    fn set_address_name() {
        run_case(b"set_address_name");
    }

    #[test]
    fn remove_address_name() {
        run_case(b"remove_address_name");
    }

    #[test]
    fn set_address() {
        run_case(b"set_address");
    }

    #[test]
    fn remove_address() {
        run_case(b"remove_address");
    }
}

fn actor() -> ActorCidNumber {
    b"registrar".to_vec().try_into().expect("测试CID")
}
fn role() -> ActorRoleCode {
    b"REG".to_vec().try_into().expect("测试岗位")
}
fn name_key() -> pallet::AddressNameKeyOf<Test> {
    (
        b"GD".to_vec().try_into().unwrap(),
        b"001".to_vec().try_into().unwrap(),
        b"002".to_vec().try_into().unwrap(),
        b"101".to_vec().try_into().unwrap(),
    )
}
fn address_key() -> pallet::AddressKeyOf<Test> {
    let (p, c, t, n) = name_key();
    (
        p,
        c,
        t,
        n,
        b"0001".to_vec().try_into().unwrap(),
        b"house".to_vec().try_into().unwrap(),
    )
}
fn set_name(who: u64, code: Vec<u8>, name: Vec<u8>) -> frame_support::dispatch::DispatchResult {
    Address::set_address_name(
        RuntimeOrigin::signed(who),
        actor(),
        role(),
        b"GD".to_vec(),
        b"001".to_vec(),
        b"002".to_vec(),
        code,
        name,
    )
}
fn set_address(
    who: u64,
    local: Vec<u8>,
    detail: Vec<u8>,
) -> frame_support::dispatch::DispatchResult {
    Address::set_address(
        RuntimeOrigin::signed(who),
        actor(),
        role(),
        b"GD".to_vec(),
        b"001".to_vec(),
        b"002".to_vec(),
        b"101".to_vec(),
        local,
        detail,
    )
}

mod unit;
