//! `GET /api/health` 实现（任务 5.7）。

use axum::extract::State;
use axum::Json;
use phonemic_protocol::http::HealthResponse;

use crate::web::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        version: state.version().to_owned(),
        uptime: state.uptime_secs(),
    })
}
