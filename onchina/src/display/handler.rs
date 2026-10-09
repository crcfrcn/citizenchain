//! 大屏只读 HTTP 入口(**免登录**):`GET /api/public/legislation/display/board`。
//!
//! 机构由**节点绑定**(`active_node_binding`)唯一确定,**不接受任何请求参数**——
//! 大屏只映射本节点自身(fail-closed:未绑定即 404)。只读、无鉴权、无写面,越权面为零。
//! 契合 ADR-030 operator/display 路由分离:操作端登录鉴权,大屏公开只读。
//!
//! 无鉴权端点做两层加固:① **单飞 + 短 TTL 缓存**——持异步锁串行化构建,并发/高频轮询命中同一
//! 新鲜快照,链读扇出每 TTL 窗口至多一次(削减链读放大);② 对外**不回传内部错误细节**,
//! 细节仅 `tracing` 落服务端日志,客户端只得稳定错误码 + 固定文案。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tokio::sync::Mutex;

use crate::auth::repo;
use crate::core::chain_runtime::identity_from_binding_parts;
use crate::core::response::ApiResponse;
use crate::{api_error, AppState};

use super::model::DisplayBoard;
use super::service;

/// 大屏看板缓存 TTL:远短于前端 12s 轮询,主要用于合并并发请求为单次链读扇出。
const BOARD_CACHE_TTL: Duration = Duration::from_secs(3);

struct CachedBoard {
    at: Instant,
    board: DisplayBoard,
}

/// 节点级单例看板缓存(每节点仅一个绑定机构,故全局单例即可,无需按机构分键)。
fn board_cache() -> &'static Mutex<Option<CachedBoard>> {
    static CACHE: OnceLock<Mutex<Option<CachedBoard>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// 辖区文案:省·市;皆空(国家级)为「全国」。
fn scope_label(province: Option<&str>, city: Option<&str>) -> String {
    let parts: Vec<&str> = [province, city]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "全国".to_string()
    } else {
        parts.join(" · ")
    }
}

/// 大屏看板快照(本节点绑定机构:名册 × 活跃立法提案 × 逐席投票)。
pub(crate) async fn display_board(State(state): State<AppState>) -> Response {
    match cached_board(&state).await {
        Ok(board) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: board,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

/// 单飞 + TTL 缓存:持异步锁串行化构建,新鲜期内并发/高频请求直接复用快照,
/// 链读扇出每 TTL 窗口至多一次(对无鉴权端点是链读放大的主要抑制手段)。
async fn cached_board(state: &AppState) -> Result<DisplayBoard, Response> {
    let mut guard = board_cache().lock().await;
    if let Some(cached) = guard.as_ref() {
        if cached.at.elapsed() < BOARD_CACHE_TTL {
            return Ok(cached.board.clone());
        }
    }
    let board = build_board_uncached(state).await?;
    *guard = Some(CachedBoard {
        at: Instant::now(),
        board: board.clone(),
    });
    Ok(board)
}

/// 冷路径:解析节点绑定 → 身份 → 装配看板。错误细节仅落日志,对外回固定文案。
async fn build_board_uncached(state: &AppState) -> Result<DisplayBoard, Response> {
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                1004,
                "node is not bound to any institution",
            ));
        }
        Err(err) => {
            tracing::error!(error = %err, "display board: read node binding failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "service temporarily unavailable",
            ));
        }
    };
    // 绑定表只存链上身份键；名称与授权作用域在使用点从各自真源临时派生。
    let candidate = match state
        .db
        .with_client(move |conn| repo::candidate_for_binding_conn(conn, &binding))
    {
        Ok(candidate) => candidate,
        Err(err) => {
            tracing::warn!(error = %err, "display board: binding metadata derivation failed");
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "node configuration error",
            ));
        }
    };
    let identity = match identity_from_binding_parts(
        &candidate.institution_code,
        candidate.institution_cid_number.as_deref(),
        candidate.frg_province_code.as_deref(),
    ) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!(error = %err, "display board: node binding is misconfigured");
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "node configuration error",
            ));
        }
    };
    let label = scope_label(
        candidate.scope_province_name.as_deref(),
        candidate.scope_city_name.as_deref(),
    );
    service::build_display_board(
        &identity,
        candidate.institution_code.clone(),
        candidate.cid_short_name.clone(),
        label,
    )
    .await
    .map_err(|err| {
        tracing::warn!(error = %err, "display board: assemble from chain failed");
        api_error(
            StatusCode::BAD_GATEWAY,
            5002,
            "upstream chain data unavailable",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::scope_label;

    #[test]
    fn scope_label_joins_province_and_city() {
        assert_eq!(
            scope_label(Some("广东省"), Some("深圳市")),
            "广东省 · 深圳市"
        );
    }

    #[test]
    fn scope_label_national_when_empty() {
        assert_eq!(scope_label(None, None), "全国");
        assert_eq!(scope_label(Some(" "), Some("")), "全国");
    }

    #[test]
    fn scope_label_province_only() {
        assert_eq!(scope_label(Some("广东省"), None), "广东省");
    }
}
