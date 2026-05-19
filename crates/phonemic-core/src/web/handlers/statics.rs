//! 静态文件服务（`GET /` + `/assets/*`），任务 5.8。
//!
//! 通过 `tower_http::services::ServeDir` 暴露
//! `apps/desktop/src-tauri/resources/web/`。当目录不存在时（开发期、未运行
//! `pnpm build`），返回简短占位 HTML 而非 500。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::Path;
use tower::ServiceExt;
use tower_http::services::ServeDir;

use crate::web::state::AppState;

/// 处理 `/` 与 `/assets/*` 路径。
///
/// 这是一个普通 axum handler 而非直接挂 `ServeDir` 的 service，目的是当
/// 静态目录缺失时仍能给出友好响应（Tauri 开发期常见）。
pub async fn serve_static(
    state: axum::extract::State<AppState>,
    req: Request<Body>,
) -> Response {
    let root = state.0.static_root();
    if !Path::new(root).exists() {
        return placeholder_response();
    }
    let svc = ServeDir::new(root);
    match svc.oneshot(req).await {
        Ok(resp) => resp.into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "static serve failed");
            placeholder_response()
        }
    }
}

fn placeholder_response() -> Response {
    let body = "<!doctype html><meta charset=\"utf-8\"><title>PhoneMic</title>\
        <h1>PhoneMic Web 静态资源未构建</h1>\
        <p>请运行 <code>pnpm build</code> 后重启桌面端。</p>";
    let mut resp = (StatusCode::OK, body).into_response();
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}
