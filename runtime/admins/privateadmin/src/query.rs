//! 管理员名册及当前身份授权查询。
use crate::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn get_institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<InstitutionAdminsOf<T>> {
        let cid_number = Self::bounded_cid(cid_number)?;
        let value = AdminAccounts::<T>::get(cid_number)?;
        if value.institution_code != institution_code
            || !can_store_private_admin_code(&institution_code)
        {
            return None;
        }
        Some(value)
    }
}

impl<T: pallet::Config> InstitutionAdminQuery<T::AccountId> for pallet::Pallet<T> {
    fn institution_admins_exist(institution_code: InstitutionCode, cid_number: &[u8]) -> bool {
        Self::get_institution_admins(institution_code, cid_number).is_some()
    }

    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &T::AccountId,
    ) -> bool {
        // 名册成员判断按 account_id（供枚举/投票快照使用）。调用者门的 CID 解析走
        // `resolve_admin_account`，二者语义不同，不可互相替换。
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.iter().any(|admin| &admin.account_id == who))
            .unwrap_or(false)
    }

    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &T::AccountId,
    ) -> Option<T::AccountId> {
        let value = Self::get_institution_admins(institution_code, cid_number)?;
        let operation = T::ChainPhase::is_operation();
        // 运行期才需要 LR 回落：私权 LR 的四要素强制在 `InstitutionInfo.legal_representative`
        // 身份记录上，其名册条目的 cid 可能为空；回落后 LR 同样按 CID 身份锚定。
        let (lr_account, lr_cid) = if operation {
            (
                T::LegalRepresentativeQuery::legal_representative(cid_number),
                T::LegalRepresentativeQuery::legal_representative_cid(cid_number),
            )
        } else {
            (None, None)
        };
        value.admins.iter().find_map(|admin| {
            // 该管理员的有效 CID：名册优先；名册为空且他是本机构 LR → 用 LR 身份记录的 cid。
            let effective_cid: Option<&[u8]> = if !admin.cid_number.is_empty() {
                Some(admin.cid_number.as_slice())
            } else if lr_account.as_ref() == Some(&admin.account_id) {
                lr_cid.as_deref()
            } else {
                None
            };
            let matched = match effective_cid {
                // 运行期 + 有有效 CID：身份锚定，只认该 CID 当前绑定的钱包
                //（无 account_id 回退 → 换绑后旧钱包掉权）。
                Some(cid_bytes) if operation => {
                    T::CitizenIdentityBinding::matches_citizen_account(cid_bytes, caller)
                }
                // 创世期，或无 CID 管理员（私权非 LR / 个人多签）：钱包锚定（现状）。
                _ => &admin.account_id == caller,
            };
            matched.then(|| admin.account_id.clone())
        })
    }

    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<T::AccountId>> {
        Self::get_institution_admins(institution_code, cid_number).map(|value| {
            value
                .admins
                .into_inner()
                .into_iter()
                .map(|admin| admin.account_id)
                .collect()
        })
    }

    fn institution_admins_len(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.len() as u32)
    }
}
