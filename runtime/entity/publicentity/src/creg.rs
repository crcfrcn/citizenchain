//! 从机构账户登记中识别市注册局主账户。

use crate::{AccountNameOf, CidNumberOf, Config};
use entitybase::RegisteredInstitution;
use primitives::{account_derive::RESERVED_NAME_MAIN, cid::number::parse_cid_number_parts_bytes};

pub fn main_cid<T: Config>(
    account: &RegisteredInstitution<CidNumberOf<T>, AccountNameOf<T>>,
) -> Option<&CidNumberOf<T>> {
    if account.account_name.as_slice() != RESERVED_NAME_MAIN {
        return None;
    }
    let parts = parse_cid_number_parts_bytes(&account.cid_number).ok()?;
    (parts.institution == *b"CREG").then_some(&account.cid_number)
}
