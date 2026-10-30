use crate::types::ResolutionIssuanceDetail;

use nodeclient::codec::truncate_chars;

pub fn format_issuance_summary(d: &ResolutionIssuanceDetail) -> String {
    let total: u128 = d.total_amount_fen.parse().unwrap_or(0);
    let reason_short = truncate_chars(&d.reason, 30);
    format!(
        "决议发行 {}.{:02} 元（{}条分配）：{reason_short}",
        total / 100,
        total % 100,
        d.allocations.len()
    )
}
