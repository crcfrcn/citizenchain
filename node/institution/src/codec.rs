use crate::admins::AdminDecoded;
use crate::admins::InstitutionAdminsDecoded;
use adminbase::Admin;
use adminbase::InstitutionAdmins;
use codec::Decode;

/// 解码机构管理员模块的 `InstitutionAdmins`。
///
/// 公权、私权机构统一采用 `account_id + cid_number + family_name + given_name`。
/// 机构 CID 只存在于 storage key；记录内的 `cid_number` 是管理员公民 CID 引用。
/// 岗位、任期和来源另查 entity。
pub(crate) fn decode_institution_admins(
    data: &[u8],
    is_public: bool,
) -> Result<InstitutionAdminsDecoded, String> {
    if is_public {
        return decode_public_institution_admins(data);
    }
    decode_private_institution_admins(data)
}

fn decode_private_institution_admins(data: &[u8]) -> Result<InstitutionAdminsDecoded, String> {
    type RawInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
    let mut input = data;
    let decoded = RawInstitutionAdmins::decode(&mut input)
        .map_err(|e| format!("InstitutionAdmins SCALE 解码失败: {e}"))?;
    if !input.is_empty() {
        return Err("InstitutionAdmins 存在尾随字节".to_string());
    }
    let admins = decoded
        .admins
        .into_iter()
        .map(|admin| {
            let cid_number = String::from_utf8(admin.cid_number.into_inner())
                .map_err(|_| "管理员 cid_number 不是 UTF-8".to_string())?;
            let family_name = String::from_utf8(admin.family_name.into_inner())
                .map_err(|_| "管理员 family_name 不是 UTF-8".to_string())?;
            let given_name = String::from_utf8(admin.given_name.into_inner())
                .map_err(|_| "管理员 given_name 不是 UTF-8".to_string())?;
            if family_name.is_empty() || given_name.is_empty() {
                return Err("管理员 family_name/given_name 不得为空".to_string());
            }
            Ok(AdminDecoded {
                account_id: format!("0x{}", hex::encode(admin.account_id)),
                cid_number,
                family_name,
                given_name,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(InstitutionAdminsDecoded {
        institution_code: decoded.institution_code,
        admins,
    })
}

fn decode_public_institution_admins(data: &[u8]) -> Result<InstitutionAdminsDecoded, String> {
    type RawInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
    let mut input = data;
    let decoded = RawInstitutionAdmins::decode(&mut input)
        .map_err(|e| format!("PublicInstitutionAdmins SCALE 解码失败: {e}"))?;
    if !input.is_empty() {
        return Err("PublicInstitutionAdmins 存在尾随字节".to_string());
    }
    let admins = decoded
        .admins
        .into_iter()
        .map(|admin| {
            Ok(AdminDecoded {
                account_id: format!("0x{}", hex::encode(admin.account_id)),
                cid_number: String::from_utf8(admin.cid_number.into_inner())
                    .map_err(|_| "公权管理员 cid_number 不是 UTF-8".to_string())?,
                family_name: String::from_utf8(admin.family_name.into_inner())
                    .map_err(|_| "公权管理员 family_name 不是 UTF-8".to_string())?,
                given_name: String::from_utf8(admin.given_name.into_inner())
                    .map_err(|_| "公权管理员 given_name 不是 UTF-8".to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(InstitutionAdminsDecoded {
        institution_code: decoded.institution_code,
        admins,
    })
}
#[cfg(test)]
#[path = "../tests/codec.rs"]
mod tests;
