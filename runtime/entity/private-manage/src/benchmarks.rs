//! 私权机构实体 Benchmark。
//!
//! 机构身份、管理员与动态阈值统一按 CID 寻址；账户只作为资金执行对象。
//! 当前 verifier 与注册局权限均为 runtime 注入接口，因此本文件只测量本 pallet
//! 可独立复现的账户派生、约束校验和存储路径，不伪造第二套签名或授权真源。

#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use alloc::format;
use core::hint::black_box;
use frame_benchmarking::v2::*;
use frame_support::traits::Currency;

use crate::{
    institution::types::InstitutionAccountInfo,
    pallet::{AccountNameOf, AccountRegisteredCid, CidNumberOf, InstitutionAccounts},
    AccountValidator, Config, Pallet, ProtectedSourceChecker, RegisteredInstitution,
    ReservedAccountGuard,
};

fn bounded_name<T: Config>(value: &[u8]) -> Result<AccountNameOf<T>, BenchmarkError> {
    value
        .to_vec()
        .try_into()
        .map_err(|_| BenchmarkError::Stop("benchmark account_id name should fit"))
}

fn find_safe_cid<T: Config>() -> Result<CidNumberOf<T>, BenchmarkError> {
    for candidate in 0..2_048u32 {
        let tag = format!("private-manage-benchmark-{candidate}");
        let number = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: tag.as_str(),
                p1: "0",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "SFLP",
            },
        )
        .map_err(|_| BenchmarkError::Stop("benchmark CID should generate"))?;
        let cid_number: CidNumberOf<T> = number
            .into_bytes()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("benchmark CID should fit"))?;

        let mut all_safe = true;
        for name in [crate::RESERVED_NAME_MAIN, crate::RESERVED_NAME_FEE] {
            let Ok((account_id, _)) =
                Pallet::<T>::derive_institution_account(cid_number.as_slice(), name)
            else {
                all_safe = false;
                break;
            };
            if T::ReservedAccountChecker::is_reserved(&account_id)
                || T::ProtectedSourceChecker::is_protected(&account_id)
                || !T::AccountValidator::is_valid(&account_id)
            {
                all_safe = false;
                break;
            }
        }
        if all_safe {
            return Ok(cid_number);
        }
    }
    Err(BenchmarkError::Stop("failed to find benchmark-safe CID"))
}

#[benchmarks(where T: Config)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn update_institution_info() -> Result<(), BenchmarkError> {
        let cid_number = find_safe_cid::<T>()?;
        let full_name = bounded_name::<T>("更新后的机构全称".as_bytes())?;
        let short_name = bounded_name::<T>("更新简称".as_bytes())?;

        #[block]
        {
            black_box((cid_number, full_name, short_name));
        }
        Ok(())
    }

    #[benchmark]
    fn add_institution_account() -> Result<(), BenchmarkError> {
        let cid_number = find_safe_cid::<T>()?;
        let account_name = bounded_name::<T>("BenchmarkNamedAccount".as_bytes())?;
        let (account_id, _) =
            Pallet::<T>::derive_institution_account(cid_number.as_slice(), account_name.as_slice())
                .map_err(|_| BenchmarkError::Stop("benchmark named account_id should derive"))?;
        let now = frame_system::Pallet::<T>::block_number();

        #[block]
        {
            InstitutionAccounts::<T>::insert(
                &cid_number,
                &account_name,
                InstitutionAccountInfo {
                    account_id: account_id.clone(),
                    initial_balance: T::Currency::minimum_balance(),
                    created_at: now,
                },
            );
            AccountRegisteredCid::<T>::insert(
                &account_id,
                RegisteredInstitution {
                    cid_number: cid_number.clone(),
                    account_name: account_name.clone(),
                },
            );
        }
        assert!(AccountRegisteredCid::<T>::contains_key(account_id));
        Ok(())
    }

    #[benchmark]
    fn propose_close_private_institution() -> Result<(), BenchmarkError> {
        let cid_number = find_safe_cid::<T>()?;
        let account_name = bounded_name::<T>("BenchmarkClosableAccount".as_bytes())?;

        #[block]
        {
            let (_, kind) = Pallet::<T>::derive_institution_account(
                cid_number.as_slice(),
                account_name.as_slice(),
            )
            .expect("benchmark named account_id must derive");
            black_box(kind.is_closable_institution_account());
        }
        Ok(())
    }
}
