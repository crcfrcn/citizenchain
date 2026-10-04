//! 公权机构账户派生 Benchmark。
//!
//! 机构登记、维护与关闭都依赖同一个确定性账户派生入口；这里基准该单源 helper。
//! 交易流水线的正式权重仍由 runtime benchmark CLI 对各 call 单独生成。

#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use frame_benchmarking::v2::*;

use crate::{AccountKind, CidNumberOf, Config, Pallet, RESERVED_NAME_MAIN};

fn benchmark_cid<T: Config>() -> Result<CidNumberOf<T>, BenchmarkError> {
    let number = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "public-manage-benchmark",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "CGOV",
        },
    )
    .map_err(|_| BenchmarkError::Stop("benchmark cid should generate"))?;
    number
        .into_bytes()
        .try_into()
        .map_err(|_| BenchmarkError::Stop("benchmark cid should fit"))
}

#[benchmarks(where T: Config)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn derive_institution_account() -> Result<(), BenchmarkError> {
        let cid_number = benchmark_cid::<T>()?;
        let derived;

        #[block]
        {
            derived =
                Pallet::<T>::derive_institution_account(cid_number.as_slice(), RESERVED_NAME_MAIN)
                    .map_err(|_| BenchmarkError::Stop("institution account_id should derive"))?;
        }

        let (_, kind) = derived;
        if !matches!(kind, AccountKind::InstitutionMain { .. }) {
            return Err(BenchmarkError::Stop("unexpected account_id kind"));
        }
        Ok(())
    }
}
