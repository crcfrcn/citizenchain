use crate::types::ResolutionDestroyDetail;

pub fn format_destroy_summary(d: &ResolutionDestroyDetail) -> String {
    let amount: u128 = d.amount_fen.parse().unwrap_or(0);
    let inst_name = nodeinstitution::registry::find_institution(&d.actor_cid_number)
        .map(|item| item.cid_full_name().to_string())
        .unwrap_or_else(|| {
            if d.actor_cid_number.is_empty() {
                "未知机构".to_string()
            } else {
                d.actor_cid_number.clone()
            }
        });
    format!(
        "决议销毁 {} 元：{inst_name}",
        nodeclient::signing::format_amount(amount as f64 / 100.0)
    )
}
