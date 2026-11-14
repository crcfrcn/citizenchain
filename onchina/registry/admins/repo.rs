//! 注册局详情目标按准确链上CID校验；本地admin表不再提供名册、计数或删除授权。
use onchina::authentication::login::AdminAuthContext;

pub(crate) fn registry_target_scope(
    ctx: &AdminAuthContext,
    cid: &str,
) -> Result<(String, String), String> {
    let parts = primitives::cid::number::parse_cid_number_parts_bytes(cid.as_bytes())?;
    if parts.institution != *b"CREG" {
        return Err("目标不是市注册局".into());
    }
    let (province, city) = primitives::cid::number::cid_scope_codes(cid.as_bytes())?;
    if city == *b"000" {
        return Err("省级占位市不是办理目标".into());
    }
    let actor_province = ctx
        .scope_province_name
        .as_deref()
        .and_then(onchina::codes::china::province_code_by_name)
        .ok_or("管理员省域缺失")?;
    if actor_province.as_bytes() != province
        || !(ctx.institution_code == "FRG"
            || (ctx.institution_code == "CREG" && ctx.institution_cid_number == cid))
    {
        return Err("目标不在当前管理员机构辖区".into());
    }
    let (province, city, _) = onchina::codes::china::area_name_by_codes(
        actor_province,
        Some(core::str::from_utf8(&city).map_err(|_| "市码无效")?),
        None,
    )
    .ok_or("市注册局行政区无效")?;
    Ok((province.into(), city.ok_or("市注册局行政区缺失")?.into()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "../tests/admins_repo.rs"]
mod tests;
