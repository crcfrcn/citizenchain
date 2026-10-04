//! 运行期启动辅助和显式维护动作。
//!
//! 公权机构不在这里生成或对账;其唯一真源是链上 PublicManage,
//! 本地只通过 gov::service 同步链投影缓存。

use crate::AppState;

/// 审计日志只存"事实"——detail 是结构化 JSON(键小写蛇形,值存系统原值:
/// 代码/布尔/原始字段),不得写展示文案;人话翻译统一归前端操作记录渲染器
/// (OperationRecords 的键名/值映射),文案改版零后端改动且历史行同样适用。
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_audit_log(
    state: &AppState,
    action: &'static str,
    actor_account_id: &str,
    target_cid: Option<String>,
    detail: serde_json::Value,
) {
    let actor_account_id = crate::crypto::pubkey::normalize_account_id(actor_account_id);
    let action = action.to_string();
    let log_action = action.clone();
    // 审计归属「办理该动作的注册局」= 本节点自身作用域(省/市),与目标 CID 无关。
    // 目标 CID 只作关联列 target_cid:人主体目标 R5 去地域化不载省市,机构目标省市也不代表办理局。
    // 单一真源 resolve_node_scope(与审计读侧管理员作用域同源);节点未绑定机构 = 非法调用,
    // 丢弃不写(禁止兜底/退化,绝不落到错误分区)。
    let (province_code, city_code) = match crate::domains::projection::resolve_node_scope(&state.db)
    {
        Ok(Some((_is_federal, scope))) => {
            let city = (scope.city_code != "000").then_some(scope.city_code);
            (scope.province_code, city)
        }
        Ok(None) => {
            tracing::warn!(action = %log_action, "append audit dropped: node unbound, no registrar scope");
            return;
        }
        Err(err) => {
            tracing::warn!(action = %log_action, error = %err, "append audit dropped: resolve node scope failed");
            return;
        }
    };
    if let Err(err) = state.db.with_client(move |conn| {
        conn.execute(
            "INSERT INTO audit(
                province_code, city_code, actor_account_id, action, target_cid, detail
             )
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &province_code,
                &city_code,
                &actor_account_id,
                &action,
                &target_cid,
                &detail,
            ],
        )
        .map_err(|e| format!("insert audit failed: {e}"))?;
        Ok(())
    }) {
        tracing::warn!(action = %log_action, error = %err, "append audit failed");
    }
}
