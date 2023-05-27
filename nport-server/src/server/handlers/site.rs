use axum::http::StatusCode;
use axum::{extract::State, response::IntoResponse};

use crate::server::state::SharedState;

pub async fn home(State(_state): State<SharedState>) -> impl IntoResponse {
    (StatusCode::OK, "hello")
}
