use axum::http::StatusCode;
use axum::{extract::State, response::IntoResponse};

use crate::server::build_info::BuildInfo;
use crate::server::state::SharedState;

pub async fn home(State(_state): State<SharedState>) -> impl IntoResponse {
    (StatusCode::OK, "hello")
}

pub async fn build_info() -> impl IntoResponse {
    let build_info = BuildInfo::default();
    serde_json::to_string_pretty(&build_info).unwrap()
}
